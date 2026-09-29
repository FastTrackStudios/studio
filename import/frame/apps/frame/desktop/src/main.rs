//! `frame` — the desktop surface player.
//!
//! ```text
//! frame [FILE.fm | FILE.frame] [--theme stage-dark|daylight] [--stats] [--size WxH]
//! frame [FILE] --screenshot OUT.png [--frames N] [--script drag-first-knob]
//! frame [FILE] --engine ws://127.0.0.1:4040/vox [--rig guitar] [...]
//! frame DIR                        # gallery: ] / Tab next, [ / Shift-Tab previous
//! ```
//! Runs a control surface live against the in-process mock engine
//! (`surface_memory::Memory::demo()`): moving meters, knobs/faders/buttons
//! driven by mouse and keyboard. With no file it plays the surface
//! generated from the demo catalog (`--theme` picks its look). With a
//! markup or bundle file it hot-reloads on save — edit the `.fm` (by hand
//! or with a model) and the window follows; a parse error keeps the last
//! good surface and shows `line:col: message` in a red banner.
//!
//! `--engine` plays against a live engine (e.g. signal serving a rig)
//! instead of the mock: meters and parameter changes stream in, edits go
//! out, and with no file the surface is generated from the engine's own
//! catalog. `--rig` picks the engine instance (scope) on that endpoint. If
//! the link drops, a banner says so, the surface keeps its last state, and
//! the player reconnects with backoff.
//!
//! `--screenshot` runs the same loop offscreen for N frames and writes the
//! last one — a headless check that input, engine and rendering agree.

mod headless;
mod input;
mod player;
mod remote;
mod source;
mod window;

use std::path::PathBuf;
use std::time::Duration;

use surface_runtime::Surface;
use surface_runtime::sync::Engine as _;
use surface_widgets::Theme;

use crate::player::Backend;
use crate::remote::Remote;
use crate::source::Source;

/// How long startup waits for `--engine` to answer.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A live engine to play against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineArg {
    pub url: String,
    /// Instance scope on the endpoint (`guitar`); `None` = unscoped.
    pub rig: Option<String>,
}

pub struct Args {
    pub source: Source,
    /// With a directory argument: every surface in it, `source` first.
    /// `]`/Tab and `[`/Shift-Tab step through them in the window.
    pub gallery: Vec<Source>,
    pub stats: bool,
    /// Initial window / screenshot size, logical px.
    pub size: (u32, u32),
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    pub script: Option<Script>,
    pub engine: Option<EngineArg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    DragFirstKnob,
}

const USAGE: &str = "usage: frame [FILE.fm|FILE.frame] [--theme stage-dark|daylight] [--stats] [--size WxH]\n       \
                     frame [FILE] --screenshot OUT.png [--frames N] [--script drag-first-knob]\n       \
                     frame [FILE] --engine ws://HOST:PORT/vox [--rig NAME] [...]\n       \
                     frame DIR   (gallery of every .fm/.frame in DIR; ] / Tab next, [ / Shift-Tab previous)";

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut file = None;
    let mut theme = Theme::stage_dark();
    let mut rig = None;
    let mut out = Args {
        source: Source::Generated { theme: Box::new(Theme::stage_dark()) },
        gallery: Vec::new(),
        stats: false,
        size: (1280, 480),
        screenshot: None,
        frames: 60,
        script: None,
        engine: None,
    };
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{a} needs a value\n{USAGE}"));
        match a.as_str() {
            "--theme" => {
                theme = match value()?.as_str() {
                    "daylight" => Theme::daylight(),
                    "stage-dark" => Theme::stage_dark(),
                    other => return Err(format!("unknown theme {other:?} (stage-dark|daylight)")),
                }
            }
            "--stats" => out.stats = true,
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size wants WxH")?;
                out.size = (w.parse().map_err(|_| "--size wants WxH")?, h.parse().map_err(|_| "--size wants WxH")?);
            }
            "--screenshot" => out.screenshot = Some(value()?.into()),
            "--frames" => out.frames = value()?.parse().map_err(|_| "--frames wants a count")?,
            "--script" => {
                out.script = match value()?.as_str() {
                    "drag-first-knob" => Some(Script::DragFirstKnob),
                    other => return Err(format!("unknown script {other:?} (drag-first-knob)")),
                }
            }
            "--engine" => out.engine = Some(EngineArg { url: value()?, rig: None }),
            "--rig" => rig = Some(value()?),
            "-h" | "--help" => return Err(USAGE.into()),
            flag if flag.starts_with('-') => return Err(format!("unknown flag {flag}\n{USAGE}")),
            path if file.is_none() => file = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument {extra}\n{USAGE}")),
        }
    }
    match file {
        Some(dir) if dir.is_dir() => {
            out.gallery = gallery(&dir)?;
            out.source = out.gallery[0].clone();
        }
        Some(path) => out.source = Source::from_path(path),
        None => out.source = Source::Generated { theme: Box::new(theme) },
    }
    match (&mut out.engine, rig) {
        (Some(e), rig) => e.rig = rig,
        (None, Some(_)) => return Err(format!("--rig needs --engine\n{USAGE}")),
        (None, None) => {}
    }
    Ok(out)
}

/// Every `.fm` / `.frame` in `dir`, sorted by name.
fn gallery(dir: &std::path::Path) -> Result<Vec<Source>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "fm" || x == "frame"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("{}: no .fm or .frame surfaces", dir.display()));
    }
    Ok(paths.into_iter().map(Source::from_path).collect())
}

/// The engine (mock, or connected live) and the first surface to play. A
/// broken file isn't fatal: it yields the placeholder plus its error.
///
/// # Errors
/// When `--engine` can't be reached.
pub fn start(args: &Args) -> Result<(Backend, Surface, Option<String>), String> {
    let engine = match &args.engine {
        None => Backend::Local(surface_memory::Memory::demo()),
        Some(e) => {
            eprintln!("frame: connecting to {}{}", e.url, e.rig.as_ref().map_or(String::new(), |r| format!(" (rig {r})")));
            let mut remote = Remote::start(&e.url, e.rig.as_deref())?;
            remote.wait_connected(CONNECT_TIMEOUT)?;
            Backend::Remote(remote)
        }
    };
    let catalog = pollster::block_on(engine.catalog()).map_err(|e| format!("engine: {e}"))?;
    Ok(match args.source.load(&catalog) {
        Ok(s) => (engine, s, None),
        Err(e) => {
            eprintln!("{e}");
            (engine, source::placeholder(), Some(e))
        }
    })
}

fn main() {
    let result = parse_args(std::env::args().skip(1))
        .and_then(|args| if args.screenshot.is_some() { headless::run(&args) } else { window::run(&args) });
    if let Err(err) = result {
        eprintln!("frame: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Args, String> {
        parse_args(s.split_whitespace().map(String::from))
    }

    #[test]
    fn no_args_plays_the_demo() {
        let a = args("").unwrap();
        assert!(matches!(a.source, Source::Generated { .. }));
        assert!(a.screenshot.is_none());
        assert!(a.engine.is_none());
    }

    #[test]
    fn files_and_flags() {
        let a = args("s.fm --theme daylight --stats --size 800x600").unwrap();
        assert!(matches!(a.source, Source::Markup(_)));
        assert!(a.stats);
        assert_eq!(a.size, (800, 600));
        let a = args("--screenshot o.png --frames 5 --script drag-first-knob x.frame").unwrap();
        assert!(matches!(a.source, Source::Bundle(_)));
        assert_eq!((a.frames, a.script), (5, Some(Script::DragFirstKnob)));
    }

    #[test]
    fn engine_and_rig() {
        let a = args("--engine ws://127.0.0.1:4040/vox --rig guitar").unwrap();
        assert!(matches!(a.source, Source::Generated { .. }), "no file → generated from the engine's catalog");
        assert_eq!(a.engine, Some(EngineArg { url: "ws://127.0.0.1:4040/vox".into(), rig: Some("guitar".into()) }));
        let a = args("--rig keys s.fm --engine ws://h/vox").unwrap();
        assert_eq!(a.engine.unwrap().rig.as_deref(), Some("keys"), "flag order doesn't matter");
        assert_eq!(args("--engine ws://h/vox").unwrap().engine.unwrap().rig, None);
        assert!(args("--rig guitar").is_err(), "--rig without --engine");
        assert!(args("--engine").is_err());
    }

    #[test]
    fn rejects_junk() {
        assert!(args("--nope").is_err());
        assert!(args("a.fm b.fm").is_err());
        assert!(args("--theme neon").is_err());
        assert!(args("--size 800").is_err());
    }
}

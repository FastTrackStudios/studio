//! `frame` — the desktop surface player.
//!
//! ```text
//! frame [FILE.fm | FILE.frame] [--theme stage-dark|daylight] [--stats] [--size WxH]
//! frame [FILE] --screenshot OUT.png [--frames N] [--script drag-first-knob]
//! ```
//! Runs a control surface live against the in-process mock engine
//! (`surface_memory::Memory::demo()`): moving meters, knobs/faders/buttons
//! driven by mouse and keyboard. With no file it plays the surface
//! generated from the demo catalog (`--theme` picks its look). With a
//! markup or bundle file it hot-reloads on save — edit the `.fm` (by hand
//! or with a model) and the window follows; a parse error keeps the last
//! good surface and shows `line:col: message` in a red banner.
//!
//! `--screenshot` runs the same loop offscreen for N frames and writes the
//! last one — a headless check that input, engine and rendering agree.

mod headless;
mod input;
mod player;
mod source;
mod window;

use std::path::PathBuf;

use surface_widgets::Theme;

use crate::source::Source;

pub struct Args {
    pub source: Source,
    pub stats: bool,
    /// Initial window / screenshot size, logical px.
    pub size: (u32, u32),
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    pub script: Option<Script>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    DragFirstKnob,
}

const USAGE: &str = "usage: frame [FILE.fm|FILE.frame] [--theme stage-dark|daylight] [--stats] [--size WxH]\n       \
                     frame [FILE] --screenshot OUT.png [--frames N] [--script drag-first-knob]";

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut file = None;
    let mut theme = Theme::stage_dark();
    let mut out = Args {
        source: Source::Demo { theme: Box::new(Theme::stage_dark()) },
        stats: false,
        size: (1280, 480),
        screenshot: None,
        frames: 60,
        script: None,
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
            "-h" | "--help" => return Err(USAGE.into()),
            flag if flag.starts_with('-') => return Err(format!("unknown flag {flag}\n{USAGE}")),
            path if file.is_none() => file = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument {extra}\n{USAGE}")),
        }
    }
    out.source = file.map_or(Source::Demo { theme: Box::new(theme) }, Source::from_path);
    Ok(out)
}

fn main() {
    let result = parse_args(std::env::args().skip(1))
        .and_then(|args| if args.screenshot.is_some() { headless::run(&args) } else { window::run(args) });
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
        assert!(matches!(a.source, Source::Demo { .. }));
        assert!(a.screenshot.is_none());
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
    fn rejects_junk() {
        assert!(args("--nope").is_err());
        assert!(args("a.fm b.fm").is_err());
        assert!(args("--theme neon").is_err());
        assert!(args("--size 800").is_err());
    }
}

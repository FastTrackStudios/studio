//! `daw-view-tools`: the DAW views' headless tools.
//!
//! ```sh
//! daw-view-tools gallery --pack <folder|owner/repo[@rev]|fts|reaper|ableton|none>
//!                        [--theme <name>] [--shot <name>]… --out <dir>
//! daw-view-tools shots                  # the gallery's shots, one a line
//! daw-view-tools verify                 # each token: the shot whose pixels it changes
//! ```
//!
//! `gallery` draws a theme pack's showcase shots from the golden session
//! into `<dir>/<theme>/<shot>.png`, and, when it drew every shot,
//! `<dir>/README.md`: a folder a pack repository commits as its
//! `gallery/`. It answers a JSON summary on stdout (what it wrote, how
//! long each theme took). A linked pack is installed into
//! `$XDG_CACHE_HOME/fts/theme-packs`, where Studio keeps them.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use daw_view_tools::gallery;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("gallery") => Gallery::parse(args.get(1..).unwrap_or_default()).and_then(Gallery::run),
        Some("shots") => Ok(shot_list()),
        Some("verify") => verify(),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(text) => {
            let _ = writeln!(std::io::stdout().lock(), "{text}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            let _ = writeln!(std::io::stderr().lock(), "daw-view-tools: {why}");
            ExitCode::from(2)
        }
    }
}

const USAGE: &str = "usage: daw-view-tools gallery --pack <pack|none> [--theme <name>] [--shot <name>]… --out <dir> | shots | verify";

/// Each token of the spec, the first shot whose pixels it changes (or
/// `-`), and the shots that list it: a tab-separated table.
fn verify() -> Result<String, String> {
    let (golden, planned) = gallery::stage_session().map_err(|e| e.to_string())?;
    let seen = gallery::verify::verify(&golden, &planned).map_err(|e| e.to_string())?;
    Ok(seen
        .iter()
        .map(|s| {
            format!(
                "{}\t{}\t{}",
                s.token,
                s.shot.unwrap_or("-"),
                s.listed.join(",")
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The shots, one a line: name, size, title.
fn shot_list() -> String {
    daw_theme::gallery::SHOTS
        .iter()
        .map(|s| {
            format!(
                "{}\t{}x{}\t{}",
                s.name, s.size.width, s.size.height, s.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A `gallery` run's arguments.
struct Gallery {
    pack: String,
    theme: Option<String>,
    shots: Vec<String>,
    out: PathBuf,
}

impl Gallery {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut pack = None;
        let mut theme = None;
        let mut shots = Vec::new();
        let mut out = None;
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let mut value = || {
                it.next()
                    .cloned()
                    .ok_or_else(|| format!("{flag} wants a value"))
            };
            match flag.as_str() {
                "--pack" => pack = Some(value()?),
                "--theme" => theme = Some(value()?),
                "--shot" => shots.push(value()?),
                "--out" => out = Some(PathBuf::from(value()?)),
                other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
            }
        }
        Ok(Self {
            pack: pack.ok_or_else(|| format!("--pack is wanted\n{USAGE}"))?,
            theme,
            shots,
            out: out.ok_or_else(|| format!("--out is wanted\n{USAGE}"))?,
        })
    }

    fn run(self) -> Result<String, String> {
        let looks = gallery::Looks::resolve(&self.pack, self.theme.as_deref(), &cache_dir())
            .map_err(|e| e.to_string())?;
        let shots = gallery::shots(&self.shots).map_err(|e| e.to_string())?;
        let made = gallery::run(&looks, &shots, &self.out).map_err(|e| e.to_string())?;
        Ok(summary(&self.out, &made))
    }
}

/// Where linked packs are installed: Studio's cache.
fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("fts")
        .join("theme-packs")
}

/// The run as JSON: the folder, every picture, the page, each theme's time.
fn summary(out: &std::path::Path, made: &gallery::Made) -> String {
    serde_json::json!({
        "out": out,
        "pictures": made.pictures,
        "readme": made.readme,
        "themes": made.per_theme_ms.iter().map(|(theme, ms)| serde_json::json!({
            "theme": theme, "ms": ms,
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

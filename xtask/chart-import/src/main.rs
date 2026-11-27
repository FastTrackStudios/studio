//! `chart-import`: a setlist's songs' `.kf` charts brought into their
//! sessions, the import Studio makes when it opens a song
//! (`keyflow_session::import`), over a whole set at once.
//!
//! ```text
//! chart-import --setlist <Set.setlist>... --songs <Songs folder>
//!              [--map <map.tsv>] [--search <folder>]... [--apply]
//! ```
//!
//! - **The songs**: each line of a setlist names a song by its session id
//!   (`<id>  <title>`), found among `<Songs folder>/<song folder>/*.session`
//!   by the id each file holds; a line that is a folder names the song in
//!   it. Commented lines (`#`) are not songs.
//! - **Each song's chart**, in this order: the map's (`<title>\t<.kf path>`
//!   a line, `#` a comment), the `.kf` beside the song (as Studio finds
//!   it), else by title in the `--search` folders, the first folder with
//!   a match deciding. A title matches a `.kf` named for it (case,
//!   punctuation and a leading track number aside); `Archive`,
//!   `Arrangements` and `Backups` folders are not looked in. Two files in
//!   that folder that differ are **ambiguous**: none is used.
//! - **Dry run** unless `--apply`: prints `song → source → result` and
//!   writes nothing (a session is read from its bytes). With `--apply`,
//!   each song with one source and no chart of its own gets it, as one
//!   undo step, through the song's file (`song_file::SongFile`); the file
//!   is then read again and the chart checked to be there and to parse.
//!   A session's own chart is never replaced.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use keyflow_session::import::{self, Found, Outcome};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(why) => {
            report(&format!("chart-import: {why}"));
            ExitCode::from(2)
        }
    }
}

/// Prints one line of the report (this is a CLI: its output is the point).
#[allow(clippy::print_stdout)]
fn report(line: &str) {
    println!("{line}");
}

/// What the command was asked.
#[derive(Debug, Default)]
struct Args {
    setlists: Vec<PathBuf>,
    songs: Option<PathBuf>,
    map: Option<PathBuf>,
    search: Vec<PathBuf>,
    apply: bool,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = argv.into_iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} wants a value"));
        match arg.as_str() {
            "--setlist" => args.setlists.push(value()?.into()),
            "--songs" => args.songs = Some(value()?.into()),
            "--map" => args.map = Some(value()?.into()),
            "--search" => args.search.push(value()?.into()),
            "--apply" => args.apply = true,
            "-h" | "--help" => {
                return Err(
                    "chart-import --setlist <Set.setlist>... --songs <Songs folder> \
                            [--map <map.tsv>] [--search <folder>]... [--apply]"
                        .to_owned(),
                );
            }
            other => return Err(format!("not an argument: {other}")),
        }
    }
    if args.setlists.is_empty() {
        return Err("give at least one --setlist".to_owned());
    }
    Ok(args)
}

/// A song a setlist names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Named {
    /// By its session's id, with its title.
    Id { id: String, title: String },
    /// By its folder.
    Folder(PathBuf),
}

impl Named {
    fn title(&self) -> String {
        match self {
            Self::Id { title, .. } => title.clone(),
            Self::Folder(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
        }
    }
}

/// The songs a setlist file names, in its order: commented lines are not.
fn setlist_songs(text: &str) -> Vec<Named> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            if line.starts_with('/') {
                return Named::Folder(PathBuf::from(line));
            }
            let (id, title) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            Named::Id {
                id: id.to_owned(),
                title: title.trim().to_owned(),
            }
        })
        .collect()
}

/// A title as matched: lower case, letters and digits only, a leading
/// two-digit track number (`03 For You`) left off. One digit is a title's
/// own (`7 Nation Army`).
fn normal(title: &str) -> String {
    let t = title.trim_start();
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let t = if digits == 2 && t[digits..].starts_with(' ') {
        &t[digits..]
    } else {
        t
    };
    t.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The `.session` files in each song folder under `songs`, by the song id
/// each holds.
fn index_songs(songs: &Path) -> Result<HashMap<String, Vec<PathBuf>>, String> {
    let mut by_id: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let folders = std::fs::read_dir(songs).map_err(|e| format!("{}: {e}", songs.display()))?;
    for folder in folders
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
    {
        for file in sessions_in(&folder) {
            if let Some(id) = std::fs::read(&file)
                .ok()
                .and_then(|b| session_format::read::song_id(&b))
            {
                by_id.entry(id).or_default().push(file);
            }
        }
    }
    Ok(by_id)
}

/// The `.session` files directly in `folder`.
fn sessions_in(folder: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "session"))
        .collect();
    v.sort();
    v
}

/// Every `.kf` under `dir`, not in an `Archive`, `Arrangements`,
/// `Backups` or hidden folder.
fn kf_under(dir: &Path) -> Vec<PathBuf> {
    const SKIP: [&str; 3] = ["Archive", "Arrangements", "Backups"];
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for p in std::fs::read_dir(&d)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
        {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    stack.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "kf") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Where a song's chart comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    One(PathBuf),
    Ambiguous(Vec<PathBuf>),
    Nothing,
}

/// The one chart for `title` among `tiers` (each a folder's `.kf` files,
/// in order): the first tier with a file named for the title decides;
/// files there that differ are ambiguous. Files the same are one chart.
fn by_title(title: &str, tiers: &[Vec<PathBuf>]) -> Source {
    let want = normal(title);
    for tier in tiers {
        let named: Vec<&PathBuf> = tier
            .iter()
            .filter(|p| {
                p.file_stem()
                    .is_some_and(|s| normal(&s.to_string_lossy()) == want)
            })
            .collect();
        if named.is_empty() {
            continue;
        }
        let mut distinct: Vec<(Vec<u8>, &PathBuf)> = Vec::new();
        for p in named {
            let bytes = std::fs::read(p).unwrap_or_default();
            if !distinct.iter().any(|(b, _)| *b == bytes) {
                distinct.push((bytes, p));
            }
        }
        return match distinct.as_slice() {
            [(_, one)] => Source::One((*one).clone()),
            _ => Source::Ambiguous(distinct.into_iter().map(|(_, p)| p.clone()).collect()),
        };
    }
    Source::Nothing
}

/// The map file: `<title>\t<.kf path>` a line.
fn read_map(path: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(t, p)| (normal(t), PathBuf::from(p.trim())))
        .collect())
}

/// The chart a session file holds now, read from its bytes (nothing
/// written): empty when it has none.
fn chart_read_only(file: &Path) -> Result<String, String> {
    use session_format::read::{MAIN, ProjectFile, SESSION};
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let project = ProjectFile::read(&bytes).map_err(|e| e.to_string())?;
    if !project.has_part(SESSION, MAIN) {
        return Ok(String::new());
    }
    let doc = project.part(SESSION, MAIN).map_err(|e| e.to_string())?;
    Ok(keyflow_session::written_chart(
        &fts_session::doc::SessionDoc::from_doc(doc),
    ))
}

/// One song's line of the report, and how it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Converted,
    WouldConvert,
    HasChart,
    NoSource,
    Ambiguous,
    Failed,
}

struct Row {
    title: String,
    source: String,
    result: String,
    kind: Kind,
}

#[allow(clippy::too_many_lines)] // One song's whole story, read top to bottom.
fn song(
    named: &Named,
    songs: &HashMap<String, Vec<PathBuf>>,
    map: &BTreeMap<String, PathBuf>,
    tiers: &[Vec<PathBuf>],
    apply: bool,
) -> Row {
    let title = named.title();
    let row = |source: String, result: String, kind| Row {
        title: title.clone(),
        source,
        result,
        kind,
    };
    let files = match named {
        Named::Id { id, .. } => songs.get(id).cloned().unwrap_or_default(),
        Named::Folder(path) => sessions_in(path),
    };
    let file = match files.as_slice() {
        [one] => one.clone(),
        [] => return row("-".into(), "no session found".into(), Kind::Failed),
        many => {
            return row(
                "-".into(),
                format!("{} sessions hold this song: {many:?}", many.len()),
                Kind::Failed,
            );
        }
    };
    let held = match chart_read_only(&file) {
        Ok(chart) => chart,
        Err(e) => return row("-".into(), format!("session unreadable: {e}"), Kind::Failed),
    };
    let source = map.get(&normal(&title)).map_or_else(
        || match import::kf_for(&file, None) {
            Found::One(p) => Source::One(p),
            Found::Several(v) => Source::Ambiguous(v),
            Found::Nothing => by_title(&title, tiers),
        },
        |p| Source::One(p.clone()),
    );
    let path = match source {
        Source::Nothing if !held.trim().is_empty() => {
            return row("-".into(), "has a chart already".into(), Kind::HasChart);
        }
        Source::Nothing => return row("-".into(), "no source found".into(), Kind::NoSource),
        Source::Ambiguous(v) => {
            let list = v
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" | ");
            let kind = if held.trim().is_empty() {
                Kind::Ambiguous
            } else {
                Kind::HasChart
            };
            return row(list, "ambiguous: several differing charts".into(), kind);
        }
        Source::One(p) => p,
    };
    let shown = path.display().to_string();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return row(shown, format!("source unreadable: {e}"), Kind::Failed),
    };
    if !held.trim().is_empty() {
        let same = held.trim_end() == text.trim_end();
        let result = if same {
            "has a chart already (the same)"
        } else {
            "has a chart already (differs from the source: kept)"
        };
        return row(shown, result.into(), Kind::HasChart);
    }
    if let Err(e) = keyflow_lang::text::chart::parse_document(&text) {
        return row(shown, format!("source does not parse: {e}"), Kind::Failed);
    }
    if !apply {
        return row(shown, "would import".into(), Kind::WouldConvert);
    }
    let outcome = song_file::SongFile::open(&file)
        .map_err(|e| e.to_string())
        .and_then(|f| {
            let outcome =
                import::import_file(f.session().session(), &path).map_err(|e| e.to_string());
            f.close().map_err(|e| e.to_string())?;
            outcome
        });
    match outcome {
        Ok(Outcome::Imported) => {}
        Ok(other) => {
            return row(
                shown,
                format!("not imported: {}", other.as_str()),
                Kind::HasChart,
            );
        }
        Err(e) => return row(shown, format!("import failed: {e}"), Kind::Failed),
    }
    match chart_read_only(&file) {
        Ok(chart) if chart == text => match keyflow_lang::text::chart::parse_document(&chart) {
            Ok(_) => row(shown, "imported, read back, parses".into(), Kind::Converted),
            Err(e) => row(
                shown,
                format!("imported, but reads back unparsed: {e}"),
                Kind::Failed,
            ),
        },
        Ok(_) => row(
            shown,
            "imported, but reads back different".into(),
            Kind::Failed,
        ),
        Err(e) => row(
            shown,
            format!("imported, but would not read back: {e}"),
            Kind::Failed,
        ),
    }
}

fn run(argv: Vec<String>) -> Result<bool, String> {
    let args = parse_args(argv)?;
    let songs = match &args.songs {
        Some(dir) => index_songs(dir)?,
        None => HashMap::new(),
    };
    let map = match &args.map {
        Some(path) => read_map(path)?,
        None => BTreeMap::new(),
    };
    let tiers: Vec<Vec<PathBuf>> = args.search.iter().map(|d| kf_under(d)).collect();
    report(if args.apply {
        "chart-import: applying"
    } else {
        "chart-import: dry run (--apply writes)"
    });
    let mut rows = Vec::new();
    for setlist in &args.setlists {
        let text =
            std::fs::read_to_string(setlist).map_err(|e| format!("{}: {e}", setlist.display()))?;
        report(&format!("\n== {}", setlist.display()));
        for named in setlist_songs(&text) {
            let r = song(&named, &songs, &map, &tiers, args.apply);
            report(&format!("{} → {} → {}", r.title, r.source, r.result));
            rows.push(r);
        }
    }
    report("");
    for (kind, label) in [
        (Kind::Converted, "converted"),
        (Kind::WouldConvert, "would convert"),
        (Kind::HasChart, "has a chart already"),
        (Kind::NoSource, "no source"),
        (Kind::Ambiguous, "ambiguous"),
        (Kind::Failed, "failed"),
    ] {
        let titles: Vec<&str> = rows
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.title.as_str())
            .collect();
        report(&format!(
            "{label}: {} ({})",
            titles.len(),
            titles.join(", ")
        ));
    }
    Ok(!rows.iter().any(|r| r.kind == Kind::Failed))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_setlist_names_its_songs_not_its_comments() {
        let text = "# id: x\n# Act I\nabc-1  Drive To LA\n\n# 9f  Better Strangers\n/run/media/x/unbreakable\n";
        assert_eq!(
            setlist_songs(text),
            vec![
                Named::Id {
                    id: "abc-1".into(),
                    title: "Drive To LA".into()
                },
                Named::Folder("/run/media/x/unbreakable".into()),
            ]
        );
    }

    #[test]
    fn titles_match_without_case_punctuation_or_track_number() {
        assert_eq!(normal("03 For You"), normal("For You"));
        assert_eq!(
            normal("Rockin Around the Christmas Tree"),
            normal("Rockin' Around the Christmas Tree")
        );
        assert_eq!(normal("7 Nation Army"), "7nationarmy");
        assert_ne!(normal("Know Me (Extended)"), normal("Know Me"));
    }

    #[test]
    fn the_first_folder_with_a_match_decides_and_differing_copies_are_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(a.join("x")).unwrap();
        std::fs::create_dir_all(b.join("Archive")).unwrap();
        std::fs::write(a.join("x/Silent Night.kf"), "Verse: C\n").unwrap();
        std::fs::write(b.join("Silent Night.kf"), "Verse: D\n").unwrap();
        std::fs::write(b.join("Archive/Silent Night.kf"), "Verse: E\n").unwrap();
        std::fs::write(b.join("Sleigh Ride.kf"), "Verse: C\n").unwrap();
        std::fs::write(dir.path().join("Sleigh Ride.kf"), "Verse: G\n").unwrap();
        let tiers = vec![kf_under(&a), kf_under(&b)];
        assert_eq!(
            by_title("Silent Night", &tiers),
            Source::One(a.join("x/Silent Night.kf"))
        );
        assert_eq!(
            by_title("Sleigh Ride", &tiers),
            Source::One(b.join("Sleigh Ride.kf"))
        );
        assert_eq!(by_title("Feliz Navidad", &tiers), Source::Nothing);
        let both = vec![kf_under(dir.path())];
        assert!(matches!(by_title("Sleigh Ride", &both), Source::Ambiguous(v) if v.len() == 2));
    }
}

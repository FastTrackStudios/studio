//! `song-file`: a song's one file, from the command line.
//!
//! ```text
//! song-file convert [--dry-run] [--trash-into <dir>] <song folder>…
//!                                                fold the old pair into one file,
//!                                                or give a lone store an id
//! song-file new [--bpm <n>] [--meter <n/d>] <folder> <title>
//!                                                a new song: <folder>/<title>.session
//!                                                with a minted id, nothing else in it
//! song-file inspect <file>                       what a song's file holds
//! ```
//!
//! `convert` writes a new file, verifies it against the old pair, then
//! moves the old pair to the Trash; it never deletes and never writes over
//! a file. `--dry-run` builds and verifies in a temporary folder and
//! changes nothing. Its report goes to stdout; spans go to stderr with
//! `RUST_LOG`.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use song_file::{Options, Outcome, Report};

fn usage() -> ExitCode {
    let _ = writeln!(
        std::io::stderr(),
        "usage: song-file convert [--dry-run] [--trash-into <dir>] <song folder>…\n       song-file new [--bpm <n>] [--meter <n/d>] <folder> <title>\n       song-file inspect <file>"
    );
    ExitCode::from(2)
}

fn report(
    out: &mut impl std::io::Write,
    r: &Report,
    dry: bool,
    seeded: bool,
) -> std::io::Result<()> {
    let verb = match (seeded, dry) {
        (false, true) => "would convert",
        (false, false) => "converted",
        (true, true) => "would seed (new id, session from the project's timing)",
        (true, false) => "seeded (new id, session from the project's timing)",
    };
    writeln!(out, "  {} {} -> {}", verb, r.title, r.file.display())?;
    writeln!(out, "  id {}", r.id)?;
    writeln!(
        out,
        "  engine: branches {:?}, {} blobs",
        r.project_branches, r.blobs
    )?;
    for (branch, s) in &r.sessions {
        writeln!(
            out,
            "  session@{branch}: {} tempo points, {} markers, {} regions, {} lanes, {} items, bar one {}",
            s.tempo_points, s.markers, s.regions, s.lanes, s.items, s.bar_one
        )?;
        for (ext, entries) in &s.extensions {
            writeln!(out, "    {ext}: {}", entries.join(", "))?;
        }
    }
    for t in &r.trashed {
        writeln!(out, "  to the Trash: {}", t.display())?;
    }
    for (from, to) in &r.proxies {
        writeln!(
            out,
            "  proxy {} {} -> {}",
            if dry { "would copy" } else { "copied" },
            from.display(),
            to.display()
        )?;
    }
    Ok(())
}

/// `new`'s arguments: the tempo (`--bpm`, with `--meter`, 4/4 when not
/// given) and the rest in order. None when one does not read.
fn new_args(rest: &[String]) -> Option<(Option<song_file::Tempo>, Vec<String>)> {
    let mut bpm = None;
    let mut meter = (4, 4);
    let mut positional = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bpm" => bpm = Some(it.next()?.parse::<f64>().ok()?),
            "--meter" => {
                let (n, d) = it.next()?.split_once('/')?;
                meter = (n.trim().parse().ok()?, d.trim().parse().ok()?);
            }
            s if s.starts_with("--") => return None,
            s => positional.push(s.to_owned()),
        }
    }
    let tempo = bpm.map(|bpm| song_file::Tempo {
        bpm,
        beats_per_bar: meter.0,
        beat_unit: meter.1,
    });
    Some((tempo, positional))
}

fn main() -> ExitCode {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .try_init();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((verb, rest)) = args.split_first() else {
        return usage();
    };
    let mut out = std::io::stdout().lock();
    match verb.as_str() {
        "convert" => {
            let dry_run = rest.iter().any(|a| a == "--dry-run");
            // `--trash-into <dir>`: the old files moved there instead of
            // the desktop's Trash.
            let trash_dir = rest
                .iter()
                .position(|a| a == "--trash-into")
                .and_then(|i| rest.get(i + 1))
                .map(PathBuf::from);
            let folders: Vec<PathBuf> = rest
                .iter()
                .enumerate()
                .filter(|(i, a)| !a.starts_with("--") && !(*i > 0 && rest[i - 1] == "--trash-into"))
                .map(|(_, a)| PathBuf::from(a))
                .collect();
            if folders.is_empty() {
                return usage();
            }
            let options = Options { dry_run, trash_dir };
            let mut failed = false;
            for folder in folders {
                let _ = writeln!(out, "{}", folder.display());
                let written = match song_file::convert(&folder, &options) {
                    Ok(Outcome::Nothing) => writeln!(out, "  no song here"),
                    Ok(Outcome::AlreadyOneFile { file, id }) => writeln!(
                        out,
                        "  one file already: {} (id {})",
                        file.display(),
                        id.as_deref().unwrap_or("none")
                    ),
                    Ok(Outcome::Converted(r)) => report(&mut out, &r, dry_run, false),
                    Ok(Outcome::Seeded(r)) => report(&mut out, &r, dry_run, true),
                    Err(e) => {
                        failed = true;
                        writeln!(out, "  NOT converted: {e}")
                    }
                };
                if written.is_err() {
                    return ExitCode::FAILURE;
                }
            }
            if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        "new" => {
            let Some((tempo, positional)) = new_args(rest) else {
                return usage();
            };
            let [folder, title] = positional.as_slice() else {
                return usage();
            };
            match song_file::new_song(&PathBuf::from(folder), title, tempo) {
                Ok(made) => {
                    let s = &made.session;
                    let _ = writeln!(out, "made {}", made.file.display());
                    let _ = writeln!(out, "  id {}", made.id);
                    let _ = writeln!(
                        out,
                        "  session@main: {} tempo points, {} markers, {} regions",
                        s.tempo_points, s.markers, s.regions
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    let _ = writeln!(out, "NOT made: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "inspect" => {
            let [file] = rest else { return usage() };
            let path = PathBuf::from(file);
            let format = session_format::sniff_path(&path);
            let _ = writeln!(out, "{}: {format:?}", path.display());
            if format == session_format::FileFormat::Store {
                let Ok(bytes) = std::fs::read(&path) else {
                    return ExitCode::FAILURE;
                };
                match session_format::read::ProjectFile::read(&bytes) {
                    Ok(read) => {
                        let _ = writeln!(out, "  id {}", read.song_id().unwrap_or("none"));
                        let _ = writeln!(
                            out,
                            "  session part on main: {}",
                            read.has_part(session_format::read::SESSION, "main")
                        );
                        if let Ok(doc) = read.doc("main") {
                            let p = session_sync::SessionDoc::from_loro(doc).read();
                            let _ = writeln!(
                                out,
                                "  engine@main: {} tracks, {} items, {} markers, {} regions, tempo {}",
                                p.tracks.len(),
                                p.items.len(),
                                p.markers.len(),
                                p.regions.len(),
                                p.tempo.first().map_or_else(
                                    || "none".to_owned(),
                                    |t| format!(
                                        "{} bpm {}/{}",
                                        t.bpm, t.beats_per_bar, t.beat_unit
                                    )
                                )
                            );
                        }
                        if let Ok(part) = read.part(session_format::read::SESSION, "main") {
                            let s = song_file::SessionSummary::of(&part);
                            let _ = writeln!(
                                out,
                                "  session@main: {} tempo points, {} markers, {} regions, bar one {}",
                                s.tempo_points, s.markers, s.regions, s.bar_one
                            );
                        }
                    }
                    Err(e) => {
                        let _ = writeln!(out, "  not readable alone (its log not folded in?): {e}");
                    }
                }
            }
            ExitCode::SUCCESS
        }
        _ => usage(),
    }
}

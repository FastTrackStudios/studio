//! `song-file`: a song's one file, from the command line.
//!
//! ```text
//! song-file convert [--dry-run] <song folder>…   fold the old pair into one file
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
        "usage: song-file convert [--dry-run] <song folder>…\n       song-file inspect <file>"
    );
    ExitCode::from(2)
}

fn report(out: &mut impl std::io::Write, r: &Report, dry: bool) -> std::io::Result<()> {
    writeln!(
        out,
        "  {} {} -> {}",
        if dry { "would convert" } else { "converted" },
        r.title,
        r.file.display()
    )?;
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
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return ExitCode::FAILURE;
    };
    let mut out = std::io::stdout().lock();
    match verb.as_str() {
        "convert" => {
            let dry_run = rest.iter().any(|a| a == "--dry-run");
            let folders: Vec<PathBuf> = rest
                .iter()
                .filter(|a| !a.starts_with("--"))
                .map(PathBuf::from)
                .collect();
            if folders.is_empty() {
                return usage();
            }
            let options = Options {
                dry_run,
                trash_dir: None,
            };
            let mut failed = false;
            for folder in folders {
                let _ = writeln!(out, "{}", folder.display());
                let written = match rt.block_on(song_file::convert(&folder, &options)) {
                    Ok(Outcome::Nothing) => writeln!(out, "  no song here"),
                    Ok(Outcome::AlreadyOneFile { file, id }) => writeln!(
                        out,
                        "  one file already: {} (id {})",
                        file.display(),
                        id.as_deref().unwrap_or("none")
                    ),
                    Ok(Outcome::Converted(r)) => report(&mut out, &r, dry_run),
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

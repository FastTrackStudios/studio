//! Get songs ready to be heard through a share link, in place: each
//! song's project file folded whole, a compressed rendition of every
//! recording it plays (`…/Proxies/<stem>.ogg`, Ogg Vorbis, with its page
//! index), its waveforms, and its reference bounce
//! (`session_engine::ingest::prepare`, what `studio ingest` runs before it
//! uploads).
//!
//! ```sh
//! link-prepare [--no-reference] <song folder | .session | .RPP | folder of songs> …
//! ```
//!
//! The masters are read and never changed; every rendition is written
//! beside them, where a vault syncs it and a link serves it as any other
//! attachment. Run it on a copy when the songs are not yours to write in.
//! One line per song is printed as JSON: what was made and what it weighs.

use std::path::{Path, PathBuf};

use session_engine::ingest::inputs::{Input, classify, song_at, songs_of};
use session_engine::ingest::prepare::{Steps, prepare};

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,session_engine=info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let mut reference = true;
    let mut inputs = Vec::new();
    for arg in std::env::args_os().skip(1) {
        if arg == "--no-reference" {
            reference = false;
        } else {
            inputs.push(PathBuf::from(arg));
        }
    }
    if inputs.is_empty() {
        eyre::bail!(
            "usage: link-prepare [--no-reference] <song folder | .session | .RPP | folder of songs> …"
        );
    }
    let mut songs = Vec::new();
    for input in &inputs {
        match classify(input)? {
            Input::Show(set) => eyre::bail!(
                "{}: a Live set is split into songs first (`show-split`)",
                set.display()
            ),
            Input::Song(file) => songs.push(song_at(&file)),
            folder @ Input::Folder(_) => songs.extend(songs_of(&folder)?),
        }
    }
    let mut failed = 0usize;
    for song in &songs {
        let began = std::time::Instant::now();
        // The reference a link's listener hears, first: the song's own
        // reference recording where it has one, else its stems' mix
        // (`bounce_for_link`). `prepare` then folds the project's log,
        // which the bounce's open wrote to, and makes the renditions.
        // Kept when it is there and newer than the project: a second run
        // does nothing.
        let bounce = song
            .folder
            .join("Media")
            .join("Proxies")
            .join("Reference.ogg");
        let age = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let current = bounce.is_file() && age(&bounce) >= age(&song.file);
        let bounced = if reference && !current {
            session_engine::reference::bounce_for_link(&song.file).map(|_| true)
        } else {
            Ok(false)
        };
        let prepared = bounced.and_then(|bounced| {
            prepare(song, Steps { reference: false }).map(|done| (done, bounced))
        });
        match prepared.map(|(done, bounced)| {
            let mut done = done;
            if bounced {
                done.reference = session_engine::ingest::prepare::Done::Made;
            }
            done
        }) {
            Ok(done) => {
                let waveforms = link_waveforms(&song.folder);
                let (masters, renditions) = weights(&song.folder);
                let line = serde_json::json!({
                    "song": song.title,
                    "folder": song.folder,
                    "proxies_made": done.proxies.made,
                    "proxies_current": done.proxies.current,
                    "proxies_failed": done.proxies.failed,
                    "peaks_made": done.peaks.made,
                    "link_waveforms": waveforms,
                    "reference": format!("{:?}", done.reference),
                    "outside": done.outside,
                    "masters_bytes": masters,
                    "renditions_bytes": renditions,
                    "seconds": began.elapsed().as_secs_f64(),
                });
                emit(&line);
            }
            Err(why) => {
                failed += 1;
                emit(&serde_json::json!({ "song": song.title, "error": why.to_string() }));
            }
        }
    }
    if failed > 0 {
        eyre::bail!("{failed} of {} songs were not prepared", songs.len());
    }
    Ok(())
}

/// One song's line, on standard output: this tool's result, read by the
/// script that builds a vault from it.
#[allow(clippy::print_stdout)]
fn emit(line: &serde_json::Value) {
    println!("{line}");
}

/// Frames a point of a link's waveform stands for: an eighth of the
/// desk's own cache, which is as large as the audio a link streams
/// (640 KB a four-minute track; this is about 80 KB).
const LINK_WAVE_BLOCK: u32 = 2048;

/// A coarse copy of each waveform cache of the song, for a link: every
/// `Peaks/<name>.sessionpeaks` again at [`LINK_WAVE_BLOCK`] in
/// `Proxies/Peaks/` beside it, where a page finds it by the same name. A
/// page that has none builds a take's waveform by decoding its whole
/// rendition, which is the one thing a streamed set must not do. How many
/// were written (those already there and newer than their source are
/// kept).
fn link_waveforms(folder: &Path) -> usize {
    use dawfile_reaper::reapeaks::ReaPeaks;
    fn walk(dir: &Path, made: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "Proxies") {
                    walk(&path, made);
                }
                continue;
            }
            let in_peaks = dir.file_name().is_some_and(|n| n == "Peaks");
            let is_cache = path.extension().is_some_and(|x| x == "sessionpeaks");
            let (Some(media), Some(name)) = (dir.parent(), path.file_name()) else {
                continue;
            };
            if !(in_peaks && is_cache) {
                continue;
            }
            let out = media.join("Proxies").join("Peaks").join(name);
            let age = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            if out.is_file() && age(&out) >= age(&path) {
                continue;
            }
            let Ok(peaks) = ReaPeaks::read(&path) else {
                continue;
            };
            if let Some(dir) = out.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if peaks.coarsened(LINK_WAVE_BLOCK).write(&out).is_ok() {
                *made += 1;
            }
        }
    }
    let mut made = 0;
    walk(folder, &mut made);
    made
}

/// What a song folder's masters and its renditions weigh, in bytes: the
/// recordings outside any `Proxies/`, and the `.ogg` inside one.
fn weights(folder: &Path) -> (u64, u64) {
    fn walk(dir: &Path, in_proxies: bool, sums: &mut (u64, u64)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let proxies = in_proxies || path.file_name().is_some_and(|n| n == "Proxies");
                walk(&path, proxies, sums);
                continue;
            }
            let size = entry.metadata().map_or(0, |m| m.len());
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if in_proxies && ext == "ogg" {
                sums.1 += size;
            } else if !in_proxies
                && matches!(ext.as_str(), "wav" | "wave" | "aif" | "aiff" | "flac")
            {
                sums.0 += size;
            }
        }
    }
    let mut sums = (0, 0);
    walk(folder, false, &mut sums);
    sums
}

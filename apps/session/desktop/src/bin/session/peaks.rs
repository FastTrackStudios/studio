//! `session peaks` — the waveform cache for a session's media, written
//! beside it: `Media/Bass.wav` → `Media/Peaks/Bass.wav.sessionpeaks`.
//!
//! The same shape as [`crate::proxies`], for the same reason: a cache
//! that lives in the session folder is versioned with the session,
//! synced with it, and *served* with it. A share link hands out any
//! non-media file whole, so the browser fetches
//! `<link>/doc/Media/Peaks/Bass.wav.sessionpeaks` and draws the real
//! waveform without downloading a note of audio — a 7-minute stereo stem
//! at 44.1 kHz is 111 MB of WAV, 5 MB of Ogg proxy, and 1 MB of peaks,
//! of which the two coarse levels a whole-song view draws from are the
//! last 70 KB.
//!
//! The file is REAPER's own format (see
//! `dawfile_reaper::sessionpeaks`), which buys the best case for free: a
//! session REAPER has already scanned has `Media/peaks/*.reapeaks`
//! sitting there, and those bytes are simply adopted rather than
//! recomputed.
//!
//! Where there is no WAV — a session that travels as proxies only — the
//! peaks are built from the Ogg. Lossy compression moves a peak by a
//! fraction of a dB, which is nothing a pixel column can show.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dawfile_reaper::reapeaks::ReaPeaks;
use dawfile_reaper::sessionpeaks;

use crate::proxies::{project_file, proxy_of, sources};

/// What happened to one media file.
enum Outcome {
    /// Our cache was already there and still matches the media.
    Current,
    /// REAPER's own cache was there and still matches: copied across,
    /// not recomputed.
    Adopted(PathBuf),
    /// Scanned. Carries what was scanned (the WAV, or the proxy).
    Built(PathBuf),
    /// Nothing to scan, and no cache to keep.
    Skipped(String),
}

/// The cache for one media file, by the cheapest route that is still
/// correct: keep, adopt, or scan.
fn one(media: &Path, force: bool) -> eyre::Result<(Outcome, PathBuf)> {
    let cache = sessionpeaks::cache_path(media)
        .ok_or_else(|| eyre::eyre!("{} has no peaks folder", media.display()))?;
    let exists = media.is_file();

    if !force {
        // Ours, still matching — or, when the media has travelled away
        // and only the cache is left, ours at all: there is nothing to
        // validate against and nothing better to do than keep it.
        if let Ok(peaks) = ReaPeaks::read(&cache)
            && (!exists || sessionpeaks::is_current(&peaks, media))
        {
            return Ok((Outcome::Current, cache));
        }
        // REAPER's, still matching: the same bytes under another name.
        if exists {
            for candidate in sessionpeaks::candidates(media).into_iter().skip(1) {
                if let Ok(peaks) = ReaPeaks::read(&candidate)
                    && sessionpeaks::is_current(&peaks, media)
                {
                    let at = sessionpeaks::write(media, &peaks)?;
                    return Ok((Outcome::Adopted(candidate), at));
                }
            }
        }
    }

    // Scan. The WAV when it is here, the proxy when it is not — a
    // proxy-built cache is stamped with the WAV's size and mtime when
    // there is a WAV, so the next run can tell whether it is still good.
    let proxy = proxy_of(media).filter(|p| p.is_file());
    let (scanned, mut peaks) = if exists {
        (media.to_path_buf(), sessionpeaks::build(media)?)
    } else if let Some(proxy) = proxy {
        let bytes: std::sync::Arc<[u8]> = std::fs::read(&proxy)?.into();
        (proxy, sessionpeaks::build_from_ogg(bytes)?)
    } else {
        return Ok((Outcome::Skipped("no media and no proxy".to_owned()), cache));
    };
    peaks.source_stamp = sessionpeaks::media_stamp(media).unwrap_or(peaks.source_stamp);
    let at = sessionpeaks::write(media, &peaks)?;
    Ok((Outcome::Built(scanned), at))
}

/// Write the peaks cache for every source a session's project names.
///
/// # Errors
///
/// The project could not be read, or a scan failed.
pub fn write(session: &Path, force: bool) -> eyre::Result<()> {
    let project = project_file(session)?;
    let media = sources(&project)?;
    let started = Instant::now();

    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(8);
    let queue = std::sync::Mutex::new(media.into_iter());
    let done = std::sync::Mutex::new(Vec::new());
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().expect("queue").next();
                    let Some(media) = next else { break };
                    match one(&media, force) {
                        Ok(result) => done.lock().expect("done").push((media, result)),
                        Err(e) => failures
                            .lock()
                            .expect("failures")
                            .push(format!("{}: {e}", media.display())),
                    }
                }
            });
        }
    });

    let mut done = done.into_inner().expect("done");
    done.sort_by(|a, b| a.0.cmp(&b.0));
    let mut bytes = 0u64;
    let (mut built, mut adopted, mut kept, mut skipped) = (0, 0, 0, 0);
    for (media, (outcome, cache)) in &done {
        let size = std::fs::metadata(cache).map_or(0, |m| m.len());
        bytes += size;
        match outcome {
            Outcome::Current => {
                kept += 1;
                println!("{}  up to date", cache.display());
            }
            Outcome::Adopted(from) => {
                adopted += 1;
                println!("{}  adopted {}", cache.display(), from.display());
            }
            Outcome::Built(from) => {
                built += 1;
                let name = from
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into());
                println!("{}  {} KB, scanned {name}", cache.display(), size / 1024);
            }
            Outcome::Skipped(why) => {
                skipped += 1;
                println!("{}  skipped — {why}", media.display());
            }
        }
    }
    println!(
        "{} sources: {built} scanned, {adopted} adopted, {kept} kept, {skipped} skipped — \
         {} KB of peaks in {:.1}s",
        done.len(),
        bytes / 1024,
        started.elapsed().as_secs_f64(),
    );

    let failures = failures.into_inner().expect("failures");
    if failures.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!(
            "{} sources failed:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

//! The songs the sequence runs on, each staged as a set in a run folder
//! of its own, so the engine opens a setlist and then a song as Studio
//! does, and never touches the original.
//!
//! - **Unbreakable**: the owner's `.session` (417 MB) is copied once into
//!   a cache in `target/` and kept (the numbers stay comparable while the
//!   owner keeps mixing; `FTS_UNBREAKABLE_REFRESH=1` takes a fresh copy).
//!   Each run gets its own copy of the cached one (a reflink on btrfs, so
//!   instant), in `Crescendum/unbreakable/`, its `Media` a link to the
//!   original's, beside a `Crescendum.setlist` naming that folder.
//! - **Synthetic**: a few seconds of generated drums (a kick, a snare, a
//!   reference snare) in an `.RPP` with the sections the sequence uses,
//!   so the harness runs everywhere, CI included.

use std::path::{Path, PathBuf};

/// The owner's song, unless `FTS_UNBREAKABLE` names another (or `off`).
pub const UNBREAKABLE: &str = "/run/media/AudioHaven/Project/Crescendum-Rockstars-SESSIONS-2026-10-08/Crescendum/unbreakable/unbreakable.comped.session";
/// The set it is opened from.
pub const CRESCENDUM: &str = "/home/cody/Drive/Studio/Setlists/Crescendum.setlist";

/// The song file to run on, or why there is none.
///
/// # Errors
///
/// It is switched off, or not on this machine: the reason, for the skip.
pub fn unbreakable() -> Result<PathBuf, String> {
    let named = std::env::var("FTS_UNBREAKABLE")
        .ok()
        .filter(|v| !v.trim().is_empty());
    if named.as_deref().is_some_and(|v| v == "0" || v == "off") {
        return Err("FTS_UNBREAKABLE is off".to_owned());
    }
    let path = PathBuf::from(named.unwrap_or_else(|| UNBREAKABLE.to_owned()));
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} is not on this machine (FTS_UNBREAKABLE names another)",
            path.display()
        ))
    }
}

/// What the cache holds a copy of.
#[derive(Debug, PartialEq)]
pub struct Source {
    pub path: String,
    pub bytes: u64,
    pub modified_unix: u64,
}

impl Source {
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"path": self.path, "bytes": self.bytes, "modified_unix": self.modified_unix})
    }

    fn read(bytes: &[u8]) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        Some(Self {
            path: v["path"].as_str()?.to_owned(),
            bytes: v["bytes"].as_u64()?,
            modified_unix: v["modified_unix"].as_u64()?,
        })
    }

    fn of(path: &Path) -> std::io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        let modified_unix = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Ok(Self {
            path: path.display().to_string(),
            bytes: meta.len(),
            modified_unix,
        })
    }
}

/// The cached copy of `song` under `cache`: made on first use (or when
/// `FTS_UNBREAKABLE_REFRESH=1`), read only from the original, never
/// written to it. Returns the copy and what it is a copy of.
///
/// # Errors
///
/// The copy could not be made, or the original kept changing under it.
pub fn cached(song: &Path, cache: &Path) -> Result<(PathBuf, Source, bool), String> {
    let name = song.file_name().ok_or("the song has no file name")?;
    let copy = cache.join(name);
    let about = cache.join("source.json");
    let refresh = std::env::var("FTS_UNBREAKABLE_REFRESH").is_ok_and(|v| v == "1");
    if !refresh
        && copy.is_file()
        && let Some(source) = std::fs::read(&about).ok().and_then(|b| Source::read(&b))
    {
        return Ok((copy, source, false));
    }
    std::fs::create_dir_all(cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    for _ in 0..3 {
        let before = Source::of(song).map_err(|e| e.to_string())?;
        let part = cache.join(".copy.part");
        std::fs::copy(song, &part).map_err(|e| format!("copying {}: {e}", song.display()))?;
        // A write-ahead log not yet folded in goes with it, or the copy
        // would miss the newest edits.
        let wal = with_suffix(song, "-wal");
        let wal_part = cache.join(".copy.part-wal");
        let _ = std::fs::remove_file(&wal_part);
        if std::fs::metadata(&wal).is_ok_and(|m| m.len() > 0) {
            std::fs::copy(&wal, &wal_part).map_err(|e| e.to_string())?;
        }
        if Source::of(song).map_err(|e| e.to_string())? != before {
            continue;
        }
        for stale in ["-wal", "-shm", "-tshm"] {
            let _ = std::fs::remove_file(with_suffix(&copy, stale));
        }
        std::fs::rename(&part, &copy).map_err(|e| e.to_string())?;
        if wal_part.exists() {
            std::fs::rename(&wal_part, with_suffix(&copy, "-wal")).map_err(|e| e.to_string())?;
        }
        std::fs::write(&about, before.json().to_string()).map_err(|e| e.to_string())?;
        return Ok((copy, before, true));
    }
    Err(format!(
        "{} kept changing while it was copied",
        song.display()
    ))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The cached `copy` of `original` staged as a set in `run`: the song in
/// `Crescendum/<song folder>/` (its media linked to the original's), and
/// `Crescendum.setlist` naming that folder, as the real setlist names the
/// album's. Returns the setlist.
///
/// # Errors
///
/// The folder or files could not be made.
pub fn stage_unbreakable(copy: &Path, original: &Path, run: &Path) -> Result<PathBuf, String> {
    let folder_name = original
        .parent()
        .and_then(Path::file_name)
        .ok_or("the song is in no folder")?;
    let album = run.join("Crescendum");
    let folder = album.join(folder_name);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let name = copy.file_name().ok_or("no file name")?;
    // Its own copy: the engine saves into it as it works.
    std::fs::copy(copy, folder.join(name)).map_err(|e| format!("staging the song: {e}"))?;
    let wal = with_suffix(copy, "-wal");
    if wal.exists() {
        std::fs::copy(&wal, with_suffix(&folder.join(name), "-wal")).map_err(|e| e.to_string())?;
    }
    if let Some(media) = original.parent().map(|p| p.join("Media"))
        && media.is_dir()
    {
        std::os::unix::fs::symlink(&media, folder.join("Media")).map_err(|e| e.to_string())?;
    }
    // The engine's set format: a song (its folder) per line, relative to
    // the file. The owner's Crescendum.setlist names songs by id, which
    // the window resolves to these same folders before the engine opens
    // them.
    let setlist = run.join("Crescendum.setlist");
    std::fs::write(
        &setlist,
        format!(
            "# Crescendum, with this run's copy of the song.\nCrescendum/{}\n",
            folder_name.to_string_lossy()
        ),
    )
    .map_err(|e| e.to_string())?;
    Ok(setlist)
}

/// Seconds of the synthetic song, and its sections.
pub const SYNTH_SECONDS: f64 = 8.0;
const RATE: u32 = 48_000;

/// The synthetic song staged as a set in `run`: `Synth/synth/synth.RPP`
/// with its media, and `Synth.setlist`. Tracks: `Drums` (`Kick`,
/// `Snare`) and `REFERENCES` (`Snare Ref`); sections `IN` (0–2 s),
/// `CH 1` (2–6 s) and `OUT` (6–8 s). Returns the setlist.
///
/// # Errors
///
/// The files could not be written.
pub fn stage_synthetic(run: &Path) -> Result<PathBuf, String> {
    let folder = run.join("Synth").join("synth");
    let media = folder.join("Media");
    std::fs::create_dir_all(&media).map_err(|e| e.to_string())?;
    write_wav(&media.join("kick.wav"), &hits(0.0, 0.5, |t| kick(t)))?;
    write_wav(
        &media.join("snare.wav"),
        &hits(0.25, 1.0, |t| snare(t, 1.0)),
    )?;
    write_wav(
        &media.join("snare-ref.wav"),
        &hits(0.25, 1.0, |t| snare(t, 0.6)),
    )?;
    std::fs::write(folder.join("synth.RPP"), rpp()).map_err(|e| e.to_string())?;
    let setlist = run.join("Synth.setlist");
    std::fs::write(
        &setlist,
        "# The synthetic song the harness runs everywhere.\nSynth/synth\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(setlist)
}

/// `voice` started every `every` s from `first` s, over the song.
fn hits(first: f64, every: f64, voice: impl Fn(f64) -> f32) -> Vec<f32> {
    let n = (SYNTH_SECONDS * f64::from(RATE)) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            if t < first {
                return 0.0;
            }
            voice((t - first) % every)
        })
        .collect()
}

/// A kick: a falling 60 Hz sine.
fn kick(t: f64) -> f32 {
    let f = 50.0 + 60.0 * (-t * 30.0).exp();
    (0.8 * (-t * 12.0).exp() * (std::f64::consts::TAU * f * t).sin()) as f32
}

/// A snare: a 200 Hz body and a noise burst, `bright` the noise's share.
fn snare(t: f64, bright: f64) -> f32 {
    // A cheap deterministic noise: no random source, the same every run.
    let noise = ((t * 1e6).sin() * 43_758.545_3).fract() * 2.0 - 1.0;
    let body = (std::f64::consts::TAU * 200.0 * t).sin() * (-t * 25.0).exp();
    (0.5 * body + 0.4 * bright * noise * (-t * 18.0).exp()) as f32
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &s in samples {
        wav.write_sample(s).map_err(|e| e.to_string())?;
    }
    wav.finalize().map_err(|e| e.to_string())
}

/// The song's project: folders by `ISBUS` (1 opens one, 2 −1 closes it),
/// one item per track over the whole song, regions for the sections.
fn rpp() -> String {
    let track = |name: &str, isbus: &str, media: Option<&str>| {
        let item = media.map_or_else(String::new, |file| {
            format!(
                "    <ITEM\n      POSITION 0\n      LENGTH {SYNTH_SECONDS}\n      NAME {file}\n      <SOURCE WAVE\n        FILE \"Media/{file}\"\n      >\n    >\n"
            )
        });
        format!("  <TRACK\n    NAME \"{name}\"\n    ISBUS {isbus}\n    NCHAN 2\n{item}  >\n")
    };
    let mut out = String::from(
        "<REAPER_PROJECT 0.1 \"7.0/test\" 1700000000\n  SAMPLERATE 48000 0 0\n  TEMPO 120 4 4 0\n",
    );
    for (i, (name, from, to)) in [
        ("IN", 0.0, 2.0),
        ("CH 1", 2.0, 6.0),
        ("OUT", 6.0, SYNTH_SECONDS),
    ]
    .into_iter()
    .enumerate()
    {
        let n = i + 1;
        out.push_str(&format!(
            "  MARKER {n} {from} \"{name}\" 1 0 1 B {{00000000-0000-0000-0000-0000000000{n:02}}} 0\n  MARKER {n} {to} \"\" 1\n"
        ));
    }
    out.push_str(&track("Drums", "1 1", None));
    out.push_str(&track("Kick", "0 0", Some("kick.wav")));
    out.push_str(&track("Snare", "2 -1", Some("snare.wav")));
    out.push_str(&track("REFERENCES", "1 1", None));
    out.push_str(&track("Snare Ref", "2 -1", Some("snare-ref.wav")));
    out.push_str(">\n");
    out
}

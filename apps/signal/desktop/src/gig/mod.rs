//! The gig page: a band's set in order, each song's chart beside its
//! practice track, and a transport to play along with.
//!
//! A gig is a folder (made by `scripts/signal/gig-prep.py` from a gig's
//! manifest) with a `gig.json` at its root:
//!
//! ```text
//! Gigs/CAA-2026/
//!   gig.json            the set: order, key, tempo, guitar notes, sections
//!   audio/*.wav         one practice mix per song
//!   charts/*.png        the band's chart (the PDF, rendered) and the
//!                       Keyflow render, per song
//!   pdf/*.pdf           the PDFs themselves
//! ```
//!
//! Gigs live under `FTS_GIGS_DIR`, else `~/Music/FastTrackStudio/Gigs` on a
//! Mac and the app's `Documents/FastTrackStudio/Gigs` on an iPad (Finder →
//! the iPad → Files → Signal). `FTS_GIG_DIR` opens one gig folder directly.
//!
//! The chart is what the band reads — the PDF — until Keyflow charts are
//! wired into this page; the Keyflow render rides along as a second tab.

pub mod player;
/// The gig on the iPad rig: its pieces lent to the tablet layout.
#[cfg(feature = "signal-guitar")]
pub mod shell;
mod view;

pub use view::GigWorkspace;

/// How a page outside the gig opens it — provided as context by the shell
/// (the desktop's workspace switch, the phone's router) and read by the rig
/// menu's Gig card.
#[derive(Clone, Copy)]
pub struct OpenGig(pub dioxus::prelude::Callback<()>);

use std::path::{Path, PathBuf};

use serde_json::Value;

/// One section of a song's practice track, in seconds of the track.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub start: f64,
    pub end: f64,
}

/// One chart image for a song.
#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub label: String,
    pub path: PathBuf,
}

/// One song of the set.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub number: u32,
    pub title: String,
    pub artist: String,
    pub key: String,
    pub tempo: String,
    /// The guitar part, as the chart labels it.
    pub guitar: String,
    pub notes: String,
    pub audio: Option<PathBuf>,
    /// What the audio is ("session full mix", "reference", …).
    pub audio_kind: String,
    pub charts: Vec<Chart>,
    pub pdf: Option<PathBuf>,
    pub sections: Vec<Section>,
    /// The song's guitar sound: All-Around unless it names a patch, plus
    /// this song's own overrides on top (STA-186).
    pub profile: SongProfile,
}

/// Which sound a gig song plays, **by reference**: the library's default
/// profile ("All-Around") when `patch` is absent, else that patch id; and
/// the song's overrides on top. Never a copy of the profile, so editing
/// All-Around moves every song that inherits it.
///
/// In `gig.json`: `"profile": {"patch": "<patch id>", "overrides":
/// [{"path": "module.drive.param.gain", "value": 0.8}]}`, every key
/// optional; no `"profile"` at all means All-Around with no overrides.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongProfile {
    pub patch: Option<String>,
    pub overrides: Vec<ParamOverride>,
}

/// One "override for this song": a parameter path and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamOverride {
    pub path: String,
    pub value: f64,
}

/// What the default profile is called on the page.
pub const ALL_AROUND: &str = "All-Around";

impl SongProfile {
    /// True when the song follows the library default.
    pub const fn inherits_default(&self) -> bool {
        self.patch.is_none()
    }

    /// What the song shows as its active profile: "All-Around",
    /// "All-Around + overrides", or the named patch's equivalent.
    pub fn label(&self) -> String {
        let base = if self.inherits_default() {
            ALL_AROUND.to_owned()
        } else {
            "Song patch".to_owned()
        };
        if self.overrides.is_empty() {
            base
        } else {
            format!("{base} + overrides")
        }
    }

    fn read(song: &Value) -> Self {
        let Some(p) = song.get("profile") else {
            return Self::default();
        };
        Self {
            patch: Some(text(p, "patch")).filter(|s| !s.is_empty()),
            overrides: p
                .get("overrides")
                .and_then(Value::as_array)
                .map(|ovs| {
                    ovs.iter()
                        .filter_map(|o| {
                            Some(ParamOverride {
                                path: Some(text(o, "path")).filter(|s| !s.is_empty())?,
                                value: o.get("value")?.as_f64()?,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// A gig: its set, and the facts worth having on the page.
#[derive(Clone, Debug, PartialEq)]
pub struct Gig {
    pub dir: PathBuf,
    pub name: String,
    /// One line each: rehearsal, soundcheck, show …
    pub dates: Vec<String>,
    pub note: String,
    pub songs: Vec<Song>,
}

/// Where gig folders live on this machine.
pub fn gigs_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("FTS_GIGS_DIR") {
        return dir.into();
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "ios") {
        home.join("Documents/FastTrackStudio/Gigs")
    } else {
        home.join("Music/FastTrackStudio/Gigs")
    }
}

/// The gigs that ship inside the app (`<App>.app/Gigs`, put there by the
/// iOS build), so a set plays on stage with no Mac and no network.
pub fn bundled_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.join("Gigs");
    dir.is_dir().then_some(dir)
}

/// The gig folders directly under `root`, by name.
fn gig_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("gig.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

/// Every gig on this machine, by folder name (`FTS_GIG_DIR` alone, if set):
/// the gigs folder's, then the app's own — a gig of the same name put in
/// the gigs folder (Files, on an iPad) wins over the one the app shipped.
pub fn list_gigs() -> Vec<Gig> {
    if let Some(dir) = std::env::var_os("FTS_GIG_DIR") {
        return read_gig(Path::new(&dir)).into_iter().collect();
    }
    let mut gigs: Vec<Gig> = Vec::new();
    let roots = std::iter::once(gigs_root()).chain(bundled_root());
    for dir in roots.flat_map(|r| gig_dirs(&r)) {
        if let Some(gig) = read_gig(&dir)
            && !gigs.iter().any(|g| g.name == gig.name)
        {
            gigs.push(gig);
        }
    }
    gigs
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// A gig folder's `gig.json`; `None` when it is missing or not a gig.
pub fn read_gig(dir: &Path) -> Option<Gig> {
    let raw = std::fs::read_to_string(dir.join("gig.json"))
        .inspect_err(|e| tracing::warn!("gig: {}: {e}", dir.display()))
        .ok()?;
    let root: Value = serde_json::from_str(&raw)
        .inspect_err(|e| tracing::warn!("gig: {}/gig.json: {e}", dir.display()))
        .ok()?;
    let rel = |v: &Value, key: &str| -> Option<PathBuf> {
        let s = text(v, key);
        (!s.is_empty()).then(|| dir.join(s)).filter(|p| p.exists())
    };
    let songs = root
        .get("songs")
        .and_then(Value::as_array)
        .map(|songs| {
            songs
                .iter()
                .zip(1u32..)
                .map(|(s, i)| Song {
                    number: s
                        .get("n")
                        .and_then(Value::as_u64)
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(i),
                    title: text(s, "title"),
                    artist: text(s, "artist"),
                    key: text(s, "key"),
                    tempo: text(s, "tempo"),
                    guitar: text(s, "guitar"),
                    notes: text(s, "notes"),
                    audio: rel(s, "audio"),
                    audio_kind: text(s, "audio_kind"),
                    charts: s
                        .get("charts")
                        .and_then(Value::as_array)
                        .map(|charts| {
                            charts
                                .iter()
                                .filter_map(|c| {
                                    Some(Chart {
                                        label: text(c, "label"),
                                        path: rel(c, "path")?,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    pdf: rel(s, "pdf"),
                    sections: s
                        .get("sections")
                        .and_then(Value::as_array)
                        .map(|sections| {
                            sections
                                .iter()
                                .filter_map(|x| {
                                    Some(Section {
                                        name: text(x, "name"),
                                        start: x.get("start")?.as_f64()?,
                                        end: x.get("end")?.as_f64()?,
                                    })
                                })
                                .filter(|x| x.end > x.start)
                                .collect()
                        })
                        .unwrap_or_default(),
                    profile: SongProfile::read(s),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Gig {
        dir: dir.to_path_buf(),
        name: Some(text(&root, "name"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| {
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            }),
        dates: root
            .get("dates")
            .and_then(Value::as_array)
            .map(|d| {
                d.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        note: text(&root, "note"),
        songs,
    })
}

/// A chart image as a `data:` URI (Blitz draws those without a file
/// loader), encoded once per file.
pub fn image_uri(path: &Path) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(path).cloned()) {
        return Some(hit);
    }
    let bytes = std::fs::read(path)
        .inspect_err(|e| tracing::warn!("gig: chart {}: {e}", path.display()))
        .ok()?;
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg") => "image/jpeg",
        Some(e) if e.eq_ignore_ascii_case("svg") => "image/svg+xml",
        _ => "image/png",
    };
    let uri = format!("data:{mime};base64,{}", base64(&bytes));
    if let Ok(mut c) = cache.lock() {
        c.insert(path.to_path_buf(), uri.clone());
    }
    Some(uri)
}

/// Standard base64, padded.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let sym = |i: u32| {
        char::from(
            ABC.get(usize::try_from(i & 63).unwrap_or(0))
                .copied()
                .unwrap_or(b'A'),
        )
    };
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        let b = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
        let n = (b(0) << 16) | (b(1) << 8) | b(2);
        out.push(sym(n >> 18));
        out.push(sym(n >> 12));
        out.push(if chunk.len() > 1 { sym(n >> 6) } else { '=' });
        out.push(if chunk.len() > 2 { sym(n) } else { '=' });
    }
    out
}

/// `m:ss`.
pub fn clock(seconds: f64) -> String {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions
    )]
    let s = seconds.max(0.0).floor() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn clock_reads_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(65.9), "1:05");
        assert_eq!(clock(-3.0), "0:00");
    }

    #[test]
    fn a_gig_folder_reads_its_set() {
        let dir = std::env::temp_dir().join(format!("gig-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("audio"));
        let _ = std::fs::write(dir.join("audio/a.wav"), b"x");
        let _ = std::fs::write(
            dir.join("gig.json"),
            r#"{"name":"Test","dates":["Sat"],"songs":[
                {"title":"One","artist":"A","key":"C","tempo":120,"audio":"audio/a.wav",
                 "sections":[{"name":"Intro","start":0.5,"end":8.0},{"name":"bad","start":3,"end":1}]},
                {"title":"Two","audio":"audio/missing.wav"}]}"#,
        );
        let gig = read_gig(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let gig = gig.unwrap_or_else(|| Gig {
            dir: PathBuf::new(),
            name: String::new(),
            dates: Vec::new(),
            note: String::new(),
            songs: Vec::new(),
        });
        assert_eq!(gig.name, "Test");
        assert_eq!(gig.songs.len(), 2);
        let one = gig.songs.first().cloned();
        assert_eq!(one.as_ref().map(|s| s.tempo.as_str()), Some("120"));
        assert_eq!(one.as_ref().map(|s| s.sections.len()), Some(1));
        assert!(one.as_ref().is_some_and(|s| s.audio.is_some()));
        assert!(gig.songs.get(1).is_some_and(|s| s.audio.is_none()));
        assert_eq!(gig.songs.get(1).map(|s| s.number), Some(2));
    }

    #[test]
    fn a_song_without_a_profile_inherits_all_around() {
        let none = serde_json::json!({"title": "One"});
        let empty = serde_json::json!({"title": "Two", "profile": {"overrides": []}});
        let over = serde_json::json!({"title": "Three", "profile": {"overrides": [
            {"path": "module.drive.param.gain", "value": 0.8},
            {"path": "", "value": 1.0},
            {"path": "module.eq.param.freq"}
        ]}});
        let patch = serde_json::json!({"title": "Four", "profile": {"patch": "p-1"}});

        for song in [&none, &empty] {
            let p = SongProfile::read(song);
            assert!(p.inherits_default());
            assert_eq!(p.label(), "All-Around");
        }
        let p = SongProfile::read(&over);
        assert!(p.inherits_default());
        assert_eq!(p.overrides.len(), 1, "malformed overrides are dropped");
        assert_eq!(p.label(), "All-Around + overrides");
        let p = SongProfile::read(&patch);
        assert_eq!(p.patch.as_deref(), Some("p-1"));
        assert_eq!(p.label(), "Song patch");
    }
}

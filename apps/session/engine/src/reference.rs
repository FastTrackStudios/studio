//! A song's reference, bounced.
//!
//! The native half of [`session_runtime::reference`] (what plays it, what it leaves out and the constants are there, and
//! re-exported here). Bouncing opens the song the native way
//! ([`crate::open_core`]), so it stays with Session's engine.

pub use session_runtime::reference::*;

#[cfg(feature = "native")]
use std::collections::HashSet;
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

#[cfg(feature = "native")]
use daw::service::{Items, ProjectContext, TrackRef, Tracks};
#[cfg(feature = "native")]
use daw::standalone::Standalone;

/// Frames rendered at a time.
#[cfg(feature = "native")]
const BLOCK: usize = 8_192;

/// libvorbis quality of the proxy (0.4 ≈ 128 kbps stereo — the stems').
#[cfg(feature = "native")]
const QUALITY: f32 = 0.4;

/// The preview's rate, channels and quality: ~35 kbps.
#[cfg(feature = "native")]
const PREVIEW: (u32, f32) = (24_000, 0.1);

/// Bounce `song` (its `.session`, `.RPP`, or the folder holding it) to its
/// reference. Returns the reference's WAV.
///
/// # Errors
///
/// The song could not be opened, or a file could not be written.
#[cfg(feature = "native")]
pub fn bounce(song: &Path) -> eyre::Result<PathBuf> {
    let song = song_file(song)?;
    let prepare = crate::prepare::Prepare::for_song(&song);
    let daw = Standalone::new();
    crate::open::equip(&daw);
    let (opened, plan) = crate::open_core::open_song_into(&daw, &song, &prepare)?;
    let project = opened.project_guid.clone();
    let media = plan
        .open
        .parent()
        .map_or_else(|| PathBuf::from("Media"), |dir| dir.join("Media"));
    std::fs::create_dir_all(&media)?;
    let wav = media.join(NAME);
    let own = daw::standalone::audio_engine::materialize::pending_media(&daw, &project)
        .iter()
        .any(|m| {
            Path::new(&m.path)
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(NAME))
        });
    let seconds = if own {
        None
    } else {
        Some(render(&daw, &project, &wav)?)
    };

    // The proxy, written aside and moved into place (as `session proxies`
    // does), so a sync agent never picks up half of one.
    let proxy = media.join("Proxies").join("Reference.ogg");
    if let Some(dir) = proxy.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = proxy.with_extension("ogg.partial");
    fts_sample::cache::write_ogg_proxy(&wav, &partial, QUALITY)
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    std::fs::rename(&partial, &proxy)?;
    let index = fts_sample::ogg_index::OggIndex::path_for;
    std::fs::rename(index(&partial), index(&proxy))?;

    preview(&wav, &media.join("Proxies").join("Reference Preview.ogg"))?;

    let peaks = dawfile_reaper::sessionpeaks::build(&wav)
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    dawfile_reaper::sessionpeaks::write(&wav, &peaks)?;
    tracing::info!(
        reference.song = %song.display(),
        reference.own = own,
        reference.seconds = seconds,
        reference.proxy_kb = std::fs::metadata(&proxy).map_or(0, |m| m.len() / 1024),
        "reference bounced"
    );
    Ok(wav)
}

/// The preview proxy of `wav` at `ogg`, and its page index: resampled,
/// folded to mono, encoded small.
#[cfg(feature = "native")]
fn preview(wav: &Path, ogg: &Path) -> eyre::Result<()> {
    let (rate, quality) = PREVIEW;
    let audio = fts_sample::load_audio(wav, f64::from(rate))
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    #[allow(clippy::cast_possible_truncation)]
    let mono: Vec<f32> = audio
        .data
        .iter()
        .map(|[l, r]| ((l + r) * 0.5) as f32)
        .collect();
    let bytes = fts_sample::cache::encode_ogg_vorbis(&mono, 1, rate, quality)
        .map_err(|e| eyre::eyre!("{}: {e}", ogg.display()))?;
    let partial = ogg.with_extension("ogg.partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, ogg)?;
    fts_sample::cache::write_ogg_index(ogg, u64::from(rate))
        .map_err(|e| eyre::eyre!("{}: {e}", ogg.display()))?;
    Ok(())
}

/// Render `project`'s mix, less what [`left_out`] names, into `wav`.
/// Returns its length, seconds.
#[cfg(feature = "native")]
fn render(daw: &Standalone, project: &str, wav: &Path) -> eyre::Result<f64> {
    for guid in left_out(daw, project) {
        let _ = Tracks::set_muted(daw, ctx(project), TrackRef::Guid(guid), true);
    }
    let frames = (song_end(daw, project) * f64::from(RATE)).ceil() as u64;
    let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(daw, project, RATE);
    let capacity = usize::try_from(frames).unwrap_or(0);
    let (mut left, mut right) = (Vec::with_capacity(capacity), Vec::with_capacity(capacity));
    let mut at = 0u64;
    while at < frames {
        let n = BLOCK.min(usize::try_from(frames - at).unwrap_or(BLOCK));
        let block = renderer.render_block(at, n);
        for frame in block.samples.chunks_exact(2).take(n) {
            left.push(frame[0]);
            right.push(frame[1]);
        }
        at += n as u64;
    }
    fts_sample::write_wav_f32(wav, RATE, &[left, right])
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    Ok(frames as f64 / f64::from(RATE))
}

/// libvorbis quality of a recording (0.45 ≈ 140 kbps stereo at 48 kHz).
#[cfg(feature = "native")]
const RECORDING_QUALITY: f32 = 0.45;

/// What a song's recording ([`record`]) was rendered from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// The track asked for, alone: the song's own recording, placed and
    /// warped on the arrangement.
    Track(String),
    /// The whole mix less what [`left_out`] names: the song had no track of
    /// that name, or nothing on it.
    Mix,
}

/// Render `song`'s recording to `ogg` (Ogg Vorbis, stereo, [`RATE`]): the
/// track named `track` (any case) alone — soloed, and heard even where the
/// song mutes it or sends it elsewhere ([`heard_alone`]) — or, with no
/// such track or nothing on it in the song, the whole mix as [`bounce`]
/// renders it. Either way from 0:00 for the song's whole length (its
/// `SONGEND` mark, else its last item's end), so it stays in step with the
/// song, edits and warps included.
///
/// # Errors
///
/// The song could not be opened, or `ogg` could not be written.
#[cfg(feature = "native")]
#[tracing::instrument(
    name = "reference.record",
    skip_all,
    fields(reference.was_muted = tracing::field::Empty, reference.was_unsent = tracing::field::Empty)
)]
pub fn record(song: &Path, track: Option<&str>, ogg: &Path) -> eyre::Result<(Recorded, f64)> {
    let song = song_file(song)?;
    let prepare = crate::prepare::Prepare::for_song(&song);
    let daw = Standalone::new();
    crate::open::equip(&daw);
    let (opened, _) = crate::open_core::open_song_into(&daw, &song, &prepare)?;
    let project = opened.project_guid.clone();
    let seconds = song_length(&daw, &project);
    let alone = track.and_then(|name| track_with_items(&daw, &project, name, seconds));
    match &alone {
        Some(guid) => {
            let heard = heard_alone(&daw, &project, guid);
            tracing::Span::current().record("reference.was_muted", heard.was_muted);
            tracing::Span::current().record("reference.was_unsent", heard.was_unsent);
        }
        None => {
            for guid in left_out(&daw, &project) {
                let _ = Tracks::set_muted(&daw, ctx(&project), TrackRef::Guid(guid), true);
            }
        }
    }
    let frames = render_frames(&daw, &project, seconds);
    let bytes = fts_sample::cache::encode_ogg_vorbis(&frames, 2, RATE, RECORDING_QUALITY)
        .map_err(|e| eyre::eyre!("{}: {e}", ogg.display()))?;
    if let Some(dir) = ogg.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = ogg.with_extension("ogg.partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, ogg)?;
    let recorded = match alone {
        Some(_) => Recorded::Track(track.unwrap_or_default().to_owned()),
        None => Recorded::Mix,
    };
    tracing::info!(
        reference.song = %song.display(),
        reference.track = track.unwrap_or(""),
        reference.alone = matches!(recorded, Recorded::Track(_)),
        reference.seconds = seconds,
        "recording rendered"
    );
    Ok((recorded, seconds))
}

/// The song's length: its `SONGEND` (or `=END`) mark, else where its last
/// item ends.
#[cfg(feature = "native")]
fn song_length(daw: &Standalone, project: &str) -> f64 {
    daw::service::Markers::all(daw, ctx(project))
        .iter()
        .filter(|m| matches!(m.name.trim(), "SONGEND" | "=END"))
        .filter_map(|m| m.position.time.map(|t| t.as_seconds()))
        .reduce(f64::min)
        .filter(|end| *end > 0.0)
        .unwrap_or_else(|| song_end(daw, project))
}

/// The guid of the track named `name` (any case) with an item that sounds
/// before `seconds`.
#[cfg(feature = "native")]
fn track_with_items(daw: &Standalone, project: &str, name: &str, seconds: f64) -> Option<String> {
    let named: HashSet<String> = Tracks::all(daw, ctx(project))
        .into_iter()
        .filter(|t| t.name.trim().eq_ignore_ascii_case(name.trim()))
        .map(|t| t.guid)
        .collect();
    Items::get_all_items(daw, ctx(project))
        .into_iter()
        .find(|item| {
            named.contains(&item.track_guid)
                && item.position.as_seconds() < seconds
                && item.position.as_seconds() + item.length.as_seconds() > 0.0
        })
        .map(|item| item.track_guid)
}

/// What [`heard_alone`] had to change for the track to be heard.
#[cfg(feature = "native")]
struct HeardAlone {
    /// It, or a folder it sits in, was muted.
    was_muted: bool,
    /// It, or a folder it sits in, did not send to its parent: a set's
    /// reference often goes to an output of its own, not the mix (Live's
    /// `AudioOut/None`, an AbleSet `+G:` guide output).
    was_unsent: bool,
}

/// `keep` soloed, and it and the folders it sits in unmuted and sending to
/// the master: the track alone, heard.
#[cfg(feature = "native")]
fn heard_alone(daw: &Standalone, project: &str, keep: &str) -> HeardAlone {
    use daw::service::Routing;
    let tracks = Tracks::all(daw, ctx(project));
    let mut heard = HeardAlone {
        was_muted: false,
        was_unsent: false,
    };
    let mut at = Some(keep.to_owned());
    let mut seen: HashSet<String> = HashSet::new();
    while let Some(guid) = at.filter(|g| seen.insert(g.clone())) {
        let Some(track) = tracks.iter().find(|t| t.guid == guid) else {
            break;
        };
        heard.was_muted |= track.muted;
        heard.was_unsent |= !track.parent_send;
        let _ = Tracks::set_muted(daw, ctx(project), TrackRef::Guid(guid.clone()), false);
        let _ = Routing::set_parent_send_enabled(daw, ctx(project), TrackRef::Guid(guid), true);
        at = track.parent_guid.clone();
    }
    let _ = Tracks::set_soloed(daw, ctx(project), TrackRef::Guid(keep.to_owned()), true);
    heard
}

/// `project` rendered from 0 for `seconds`, interleaved stereo at [`RATE`].
#[cfg(feature = "native")]
fn render_frames(daw: &Standalone, project: &str, seconds: f64) -> Vec<f32> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds.max(0.0) * f64::from(RATE)).round() as u64;
    let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(daw, project, RATE);
    let mut out = Vec::with_capacity(usize::try_from(frames * 2).unwrap_or(0));
    let mut at = 0u64;
    while at < frames {
        let n = BLOCK.min(usize::try_from(frames - at).unwrap_or(BLOCK));
        let block = renderer.render_block(at, n);
        out.extend(block.samples.chunks_exact(2).take(n).flatten());
        at += n as u64;
    }
    out
}

/// The song file under `path`: itself, or the `.session` (else `.RPP`) in
/// the folder it names.
#[cfg(feature = "native")]
fn song_file(path: &Path) -> eyre::Result<PathBuf> {
    if !path.is_dir() || crate::open_core::is_session(path) {
        return Ok(path.to_path_buf());
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    [format!("{name}.session"), format!("{name}.RPP")]
        .into_iter()
        .map(|file| path.join(file))
        .find(|file| file.exists())
        .ok_or_else(|| eyre::eyre!("{}: no {name}.session or {name}.RPP in it", path.display()))
}

/// Where the song's last item ends, in seconds.
#[cfg(feature = "native")]
fn song_end(daw: &Standalone, project: &str) -> f64 {
    Items::get_all_items(daw, ProjectContext::Project(project.to_owned()))
        .iter()
        .map(|item| item.position.as_seconds() + item.length.as_seconds())
        .fold(0.0, f64::max)
}

#[cfg(feature = "native")]
fn ctx(project: &str) -> ProjectContext {
    ProjectContext::Project(project.to_owned())
}

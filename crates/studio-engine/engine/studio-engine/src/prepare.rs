//! Prepare a multitrack session for playing: organize it, build the song
//! from its chart, and generate the click and guide — on the engine this
//! window opened, before the view reads it.
//!
//! Each step is the shared one, not a window-only copy:
//! `dynamic_template::apply::organize` (the `--apply-buses` pass, run live),
//! `engine_domain::keyflow::from_chart::build_from_chart`, and
//! `engine_domain::guide::Guide::generate`. Each is optional; they run in that
//! order because the guide reads the song the chart stamps, and files its
//! tracks into the folder the organizer builds.

use std::path::PathBuf;

use daw::service::ProjectContext;

/// What to do to a freshly opened session.
#[derive(Debug, Default, Clone)]
pub struct Prepare {
    pub organize: bool,
    pub chart: Option<PathBuf>,
    pub guide: bool,
}

impl Prepare {
    /// Everything a song gets on its first open: organized, built from the
    /// chart beside it, and given its guide. What a setlist's songs get,
    /// and what `engine-host --engine` gives a song.
    #[must_use]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn for_song(path: &std::path::Path) -> Self {
        Self::for_song_in(crate::folder::disk(), path)
    }

    /// [`Self::for_song`] for a song in `folder`.
    #[must_use]
    pub fn for_song_in(folder: &dyn crate::folder::Folder, path: &std::path::Path) -> Self {
        // `FTS_SESSION_PREPARE=0`: the multitrack as it is — a template
        // already laid out (the golden session) opens as the scenes render
        // it rather than organized a second time.
        if std::env::var("FTS_SESSION_PREPARE").as_deref() == Ok("0") {
            return Self {
                organize: false,
                chart: None,
                guide: false,
            };
        }
        let chart = path
            .parent()
            .and_then(|dir| folder.only_with_extension(dir, "kf"));
        Self {
            organize: true,
            chart,
            guide: true,
        }
    }

    /// From `FTS_BLITZ_ORGANIZE=1`, `FTS_BLITZ_CHART=<file.kf>` and
    /// `FTS_BLITZ_GUIDE=1`.
    #[must_use]
    pub fn from_env() -> Self {
        let on = |key: &str| std::env::var(key).is_ok_and(|v| !v.is_empty() && v != "0");
        Self {
            organize: on("FTS_BLITZ_ORGANIZE"),
            chart: std::env::var_os("FTS_BLITZ_CHART").map(PathBuf::from),
            guide: on("FTS_BLITZ_GUIDE"),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.organize && self.chart.is_none() && !self.guide
    }

    /// Run the requested steps against `opened`. Stops at the first step
    /// that fails — a guide built for a song the chart never stamped is
    /// worse than no guide.
    ///
    /// # Errors
    ///
    /// The step that failed, and why.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&self, opened: &crate::open::Opened) -> eyre::Result<()> {
        self.run_in(crate::folder::disk(), opened)
    }

    /// [`Self::run`], reading the chart and lyrics from `folder`.
    ///
    /// # Errors
    ///
    /// The step that failed, and why.
    pub fn run_in(
        &self,
        folder: &dyn crate::folder::Folder,
        opened: &crate::open::Opened,
    ) -> eyre::Result<()> {
        // This window's runtime when it has one; otherwise whatever
        // runtime the caller runs on (`engine-host --engine`).
        #[cfg(feature = "native")]
        let _entered = crate::open::runtime().map(|runtime| runtime.enter());
        let chart = match &self.chart {
            Some(path) => Some(
                folder
                    .read_to_string(path)
                    .map_err(|e| eyre::eyre!("chart {}: {e}", path.display()))?,
            ),
            None => None,
        };
        steps(
            &opened.daw,
            &opened.project_guid,
            self.organize,
            chart.as_deref(),
            self.guide,
        )?;
        // The song's synced lyrics, when a `.lrc` sits beside its chart:
        // onto the LINES track, from where the song starts.
        if let (Some(path), Some(text)) = (&self.chart, chart.as_deref())
            && let Some(lrc) = path
                .parent()
                .and_then(|dir| folder.only_with_extension(dir, "lrc"))
        {
            let lyrics = folder
                .read_to_string(&lrc)
                .map_err(|e| eyre::eyre!("lyrics {}: {e}", lrc.display()))?;
            let lines = stamp_lyrics(&opened.daw, &opened.project_guid, text, &lyrics)?;
            tracing::info!(lyrics.lines = lines, "prepare: lyrics stamped");
        }
        Ok(())
    }
}

/// The one `.lrc` beside a chart, if there is exactly one.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn lrc_beside(chart: &std::path::Path) -> Option<PathBuf> {
    crate::folder::Folder::only_with_extension(&crate::folder::Disk, chart.parent()?, "lrc")
}

/// Stamp the synced lyrics `lrc` (an `.lrc`'s text) onto the song's LINES track: the
/// recording's 0 at the song's start (SONGSTART, from the chart), the
/// last line held to its end. Returns how many lines went on.
///
/// # Errors
///
/// The chart does not lay out, or a line's item could not be made.
pub fn stamp_lyrics(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    chart: &str,
    lrc: &str,
) -> eyre::Result<usize> {
    let layout = engine_domain::setlist::chart_import::chart_to_layout(chart)
        .map_err(|e| eyre::eyre!("chart: {e:?}"))?;
    let text = lrc;
    let mut lyrics = engine_domain::lyrics::Lyrics::from_lrc(
        text,
        layout.song_start_seconds,
        layout.song_end_seconds,
    );
    let project = ProjectContext::Project(project_guid.to_owned());
    // Pinned to the song where the `.lrc` says (see `lyrics::Anchor`): its
    // section's downbeat, from the regions the chart just laid out, and
    // the beats from there at the song's tempo.
    if let Some(anchor) = engine_domain::lyrics::Anchor::in_lrc(text) {
        use daw::service::Regions as _;
        let downbeat = daw
            .all(project.clone())
            .into_iter()
            .filter(|r| anchor.names(&r.name))
            .map(|r| r.time_range.start_seconds())
            .reduce(f64::min);
        let placed = downbeat.is_some_and(|downbeat| {
            let at = anchor
                .beats
                .mul_add(60.0 / layout.tempo_bpm.max(1.0), downbeat);
            lyrics.align(
                &anchor.line,
                at,
                anchor.drop_before,
                layout.song_end_seconds,
            )
        });
        if !placed {
            tracing::warn!(
                lyrics.anchor_section = %anchor.section,
                lyrics.anchor_found = downbeat.is_some(),
                "prepare: the lyrics' anchor matched nothing; they stay where the recording has them"
            );
        }
    }
    engine_domain::lyrics::stamp_lines(daw, project, &lyrics)
        .map_err(|e| eyre::eyre!("lyrics: {e}"))
}

/// The song's name, "Title - Artist": the project's name less a working
/// copy's suffix, title-cased (`unbreakable.current` is "Unbreakable"),
/// and the artist the folder above the song's own is named for, less a
/// snapshot's date (`crescendum-2026-10-08/unbreakable/` is "Crescendum").
fn song_name(daw: &daw_standalone::sync::Standalone, project: &ProjectContext) -> Option<String> {
    use daw::service::Projects as _;
    let info = daw.info(project.clone()).ok()?;
    let title = title_case(strip_working_suffix(&info.name));
    if title.is_empty() {
        return None;
    }
    let artist = std::path::Path::new(&info.path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .filter(|folder| names_an_artist(folder))
        .map(|folder| title_case(&strip_date(folder)))
        .filter(|artist| !artist.is_empty());
    Some(match artist {
        Some(artist) => format!("{title} - {artist}"),
        None => title,
    })
}

/// Remove the markers that only name the song (`unbreakable.organized`,
/// a title an earlier tool left at the start): the SONG region names it
/// now. Returns their names.
fn drop_title_markers(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
) -> Vec<String> {
    use daw::service::{Markers, Projects as _};
    let Ok(info) = daw.info(project.clone()) else {
        return Vec::new();
    };
    let title = strip_working_suffix(&info.name).to_lowercase();
    let mut dropped = Vec::new();
    for marker in Markers::all(daw, project.clone()) {
        if strip_working_suffix(&marker.name).to_lowercase() == title
            && let Some(id) = marker.id
            && Markers::remove(daw, project.clone(), id).is_ok()
        {
            dropped.push(marker.name);
        }
    }
    dropped
}

/// `unbreakable.current` → `unbreakable`.
fn strip_working_suffix(name: &str) -> &str {
    [".current", ".organized", ".raw"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name)
        .trim()
}

/// Whether a song's grandparent folder can name its artist: not one the
/// app files songs under — a stream's cache (`streamed/share-…/Washed.RPP`,
/// `task-…`, `peer-…`) or a plain `Session(s)`/`Projects` folder — which
/// would make every song of a set "… - Streamed".
fn names_an_artist(folder: &str) -> bool {
    let lower = folder.trim().to_lowercase();
    let filing = [
        "streamed",
        "session",
        "sessions",
        "songs",
        "projects",
        "reaper media",
    ];
    !filing.contains(&lower.as_str())
        && !["share-", "task-", "peer-"]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
}

/// `crescendum-2026-10-08` → `crescendum`: a trailing `-YYYY-MM-DD`.
fn strip_date(folder: &str) -> String {
    let parts: Vec<&str> = folder.split('-').collect();
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    match parts.as_slice() {
        [head @ .., year, month, day]
            if !head.is_empty() && digits(year, 4) && digits(month, 2) && digits(day, 2) =>
        {
            head.join("-").trim().to_string()
        }
        _ => folder.trim().to_string(),
    }
}

/// Each word's first letter capitalised, the rest as written
/// (`it knows my name` → `It Knows My Name`, `AG GTR` stays). A name
/// that already has capitals is a title somebody wrote, and stays as it
/// is (`What Christmas Means to Me`).
fn title_case(text: &str) -> String {
    if text.chars().any(char::is_uppercase) {
        return text.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where the song ends: where most of what is still playing at its last
/// marker stops — the median of those tracks' last item ends. Not the
/// latest end: a guitar take left running, a click rendered long, a
/// rehearsal in the references runs on for minutes past the song. The
/// references and the guide are not counted; with nothing else, all of it.
fn media_end(daw: &daw_standalone::sync::Standalone, project: &ProjectContext) -> MediaEnd {
    use daw::service::{Items as _, Markers, Tracks};
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    // Under REFERENCES, the song's Reference or the Guide, however deep.
    let referenced = |guid: &str| {
        let mut at = by_guid.get(guid).copied();
        while let Some(track) = at {
            let name = track.name.trim();
            if name.eq_ignore_ascii_case(dynamic_template::apply::REFERENCES_FOLDER)
                || (track.parent_guid.is_none()
                    && (name.eq_ignore_ascii_case("Guide")
                        || name.eq_ignore_ascii_case("Reference")))
            {
                return true;
            }
            at = track
                .parent_guid
                .as_deref()
                .and_then(|p| by_guid.get(p).copied());
        }
        false
    };
    let last_marker = Markers::all(daw, project.clone())
        .iter()
        .filter_map(|m| m.position.seconds())
        .fold(0.0, f64::max);
    let items = daw.get_all_items(project.clone());
    let ends = |song_only: bool| {
        let mut by_track: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
        for item in items
            .iter()
            .filter(|item| !song_only || !referenced(&item.track_guid))
        {
            let end = item.position.as_seconds() + item.length.as_seconds();
            let slot = by_track.entry(item.track_guid.as_str()).or_insert(0.0);
            *slot = slot.max(end);
        }
        let mut ends: Vec<f64> = by_track
            .into_values()
            .filter(|end| *end > last_marker)
            .collect();
        ends.sort_by(f64::total_cmp);
        Some((*ends.get(ends.len() / 2)?, *ends.last()?))
    };
    let (median, latest) = ends(true)
        .or_else(|| ends(false))
        .unwrap_or((last_marker, last_marker));
    MediaEnd {
        last_marker,
        median,
        latest,
        hidden: tracks
            .iter()
            .filter(|t| t.parent_guid.is_none() && referenced(&t.guid) && !t.muted)
            .map(|t| t.guid.clone())
            .collect(),
    }
}

/// Where a song's media runs to, past its last marker ([`media_end`]).
struct MediaEnd {
    last_marker: f64,
    /// The median end of the tracks still playing at the last marker.
    median: f64,
    /// The latest of them.
    latest: f64,
    /// The top-level folders not counted (REFERENCES, the Guide), unmuted.
    hidden: Vec<String>,
}

/// Where the song ends: the bar line after its sound dies away.
///
/// The mix from the last marker to where the media runs out is rendered
/// — the references and the guide muted for it — and the song ends at the
/// first bar line after the last moment it is within [`HEARD_DB`] of its
/// loudest there: the last hit's ring-out heard, the silence after it not.
/// unbreakable's guitars hit on bar 181 and ring through 184; its media
/// runs on to 317 s. Without a render (a page), the median end
/// ([`media_end`]), on a bar line too.
fn song_end(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    project: &ProjectContext,
) -> f64 {
    use daw::service::TempoMap as _;
    let media = media_end(daw, project);
    #[cfg(feature = "native")]
    let heard = heard_end(daw, project_guid, project, &media);
    #[cfg(not(feature = "native"))]
    let heard: Option<f64> = {
        let _ = project_guid;
        None
    };
    let end = heard.unwrap_or(media.median);
    // On a bar line: this one when it is on one, else the next.
    let (measure, beat, fraction) = daw.time_to_musical(project.clone(), end);
    if beat == 1 && fraction < 1e-3 {
        return end;
    }
    daw.musical_to_time(project.clone(), measure.saturating_add(1), 1, 0.0)
}

/// How long a silence past the last marker ends the song: what plays
/// after one is not the song's — a take recorded after it (the ballad's
/// acoustic runs on four seconds past the drums' last ring, to 493 s).
const SILENCE_SECONDS: u64 = 3;

/// The last chunk heard (`peak >= floor`) before the first silence of
/// `gap` frames that follows something heard: `chunks` are `(frame,
/// peak)`, `chunk` frames each.
fn last_heard(chunks: &[(u64, f32)], floor: f32, chunk: u64, gap: u64) -> Option<&(u64, f32)> {
    let mut heard: Option<&(u64, f32)> = None;
    for each in chunks {
        if each.1 >= floor {
            heard = Some(each);
        } else if let Some(last) = heard
            && each.0.saturating_sub(last.0.saturating_add(chunk)) >= gap
        {
            break;
        }
    }
    heard
}

/// How far under the song's loudest its end still counts as heard: the
/// last hit's ring at -48 dB is the song's, the room's hiss under it not.
const HEARD_DB: f32 = 48.0;

/// The last moment the mix past the last marker is heard ([`HEARD_DB`]),
/// rendered with the references and the guide muted.
#[cfg(feature = "native")]
fn heard_end(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    project: &ProjectContext,
    media: &MediaEnd,
) -> Option<f64> {
    use daw::service::{TrackRef, Tracks};
    use std::time::Duration;
    const RATE: u32 = 48_000;
    // 50 ms, in frames: one block and one peak.
    const CHUNK: usize = 2_400;
    const CHUNK_FRAMES: u64 = 2_400;
    let frame = |seconds: f64| {
        let nanos = Duration::try_from_secs_f64(seconds.max(0.0))
            .ok()?
            .as_nanos();
        u64::try_from(nanos.checked_mul(u128::from(RATE))? / 1_000_000_000).ok()
    };
    let (from, to) = (frame(media.last_marker)?, frame(media.latest)?);
    for guid in &media.hidden {
        let _ = Tracks::set_muted(daw, project.clone(), TrackRef::Guid(guid.clone()), true);
    }
    let renderer =
        daw::standalone::audio_engine::render::ProjectRenderer::new(daw, project_guid, RATE);
    let mut peaks: Vec<(u64, f32)> = Vec::new();
    let mut at = from;
    while at < to {
        let block = renderer.render_block(at, CHUNK);
        let peak = block.samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        peaks.push((at, peak));
        at = at.saturating_add(CHUNK_FRAMES);
    }
    for guid in &media.hidden {
        let _ = Tracks::set_muted(daw, project.clone(), TrackRef::Guid(guid.clone()), false);
    }
    let loudest = peaks.iter().fold(0.0_f32, |p, (_, peak)| p.max(*peak));
    if loudest <= 0.0 {
        return None;
    }
    let floor = loudest * 10.0_f32.powf(-HEARD_DB / 20.0);
    last_heard(
        &peaks,
        floor,
        CHUNK_FRAMES,
        u64::from(RATE).saturating_mul(SILENCE_SECONDS),
    )
    .map(|(at, _)| {
        let frames = at.saturating_add(CHUNK_FRAMES);
        Duration::from_nanos(
            frames
                .saturating_mul(1_000_000_000)
                .checked_div(u64::from(RATE))
                .unwrap_or(0),
        )
        .as_secs_f64()
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub use engine_runtime::client::prepare::chart_beside;

/// Prepare the song at `rpp` and save it as `Song.session` beside it —
/// what the app does the first time it opens a multitrack, done ahead of
/// time (a whole setlist before a service, say). Returns where the session
/// was written. Any session already there is replaced: this is the
/// explicit way to prepare again.
///
/// One song per process — the daw facade the steps go through is
/// process-wide, and it is bound to the first engine opened.
///
/// # Errors
///
/// The song did not open, a step failed (nothing is saved then), or the
/// session could not be written.
#[cfg(feature = "native")]
pub fn prepare_and_save(rpp: &std::path::Path, chart: Option<PathBuf>) -> eyre::Result<PathBuf> {
    prepare_and_save_as(rpp, chart, None)
}

/// [`prepare_and_save`], the song named `title`: its SONG region and its
/// session (`<title>.session`, beside the project) take that name, not
/// the project file's. For a project another DAW named for its own
/// purposes (`Last Christmas - Tempo Mapped.RPP` is "Last Christmas").
///
/// # Errors
///
/// As [`prepare_and_save`].
#[cfg(feature = "native")]
pub fn prepare_and_save_as(
    rpp: &std::path::Path,
    chart: Option<PathBuf>,
    title: Option<&str>,
) -> eyre::Result<PathBuf> {
    let prepare = Prepare {
        organize: true,
        chart,
        guide: true,
    };
    save_prepared(rpp, &prepare, title)
}

/// `rpp` opened, prepared as `prepare` says and saved: `<title>.session`
/// beside it, else the project's own name.
#[cfg(feature = "native")]
fn save_prepared(
    rpp: &std::path::Path,
    prepare: &Prepare,
    title: Option<&str>,
) -> eyre::Result<PathBuf> {
    let opened = crate::open::open_silent(rpp)?;
    let title = title.map(str::trim).filter(|title| !title.is_empty());
    if let Some(title) = title {
        opened
            .daw
            .write_project(&opened.project_guid, |p| title.clone_into(&mut p.info.name));
    }
    prepare.run(&opened)?;
    let dir = title.map_or_else(
        || rpp.with_extension("session"),
        |title| rpp.with_file_name(format!("{title}.session")),
    );
    crate::session_file::save_session(&opened.daw, &opened.project_guid, &dir)
        .map_err(|e| eyre::eyre!("saving {}: {e}", dir.display()))
}

/// A session for a song that is only a recording: `recording` (a WAV, an
/// MP3, an Ogg) on one track, filed as the song's reference in a
/// top-level `Reference` folder, the song named `title`, saved as
/// `<title>.session` beside the recording. `bpm` is the song's one tempo
/// when somebody knows it (its chart's), in `meter` (beats a bar, and the
/// beat's note value); nothing says where a bar line of
/// the recording is, so the song gets no sections, no tempo map and no
/// guide.
///
/// A separation of the recording beside it comes in with it: each
/// `<recording> (<Stem>).wav` ([`stems_beside`]: `Vocals`, `Bass`,
/// `Guitar`, `Piano`, `Other`, and the kit's `Kick`, `Snare`, `Hi-Hat`,
/// `Toms`, `Cymbals`, `Drums Other`) is a track, the kit's in a `DRUMS`
/// folder, as the stems of a record are laid out in REAPER. The song is
/// then heard by its reference with the stems behind the switch.
///
/// The song is written as a REAPER project beside the recording
/// (`<title>.RPP`) and prepared like any other, so what a recording
/// becomes is what a project holding its mix (and its stems) becomes.
///
/// # Errors
///
/// The recording could not be read, or as [`prepare_and_save`].
#[cfg(feature = "native")]
pub fn reference_session(
    recording: &std::path::Path,
    title: &str,
    bpm: Option<f64>,
    meter: (u32, u32),
) -> eyre::Result<PathBuf> {
    let file = recording
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| eyre::eyre!("{}: not a file", recording.display()))?;
    let audio = fts_sample::load_audio(recording, f64::from(crate::reference::RATE))
        .map_err(|e| eyre::eyre!("{}: {e}", recording.display()))?;
    #[expect(clippy::cast_precision_loss, reason = "a length in frames, as seconds")]
    let seconds = audio.data.len() as f64 / f64::from(crate::reference::RATE);
    let kind = match recording
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => "MP3",
        Some("ogg") => "VORBIS",
        Some("flac") => "FLAC",
        _ => "WAVE",
    };
    let (beats, unit) = meter;
    let tempo = bpm.map_or_else(String::new, |bpm| format!("  TEMPO {bpm} {beats} {unit}\n"));
    // One track of the project: `file` on it from zero when it has one,
    // `muted`, `folder` its place in a folder (REAPER's `ISBUS`).
    let track = |name: &str, file: Option<(&str, &str)>, muted: bool, folder: (i32, i32)| {
        let item = file.map_or_else(String::new, |(file, kind)| {
            format!(
                "    <ITEM\n      POSITION 0\n      LENGTH {seconds}\n      IGUID {{{item}}}\n      NAME \"{file}\"\n      <SOURCE {kind}\n        FILE \"{file}\"\n      >\n    >\n",
                item = uuid::Uuid::new_v4().to_string().to_uppercase(),
            )
        });
        format!(
            "  <TRACK {{{guid}}}\n    NAME \"{name}\"\n    MUTESOLO {muted} 0 0\n    ISBUS {} {}\n{item}  >\n",
            folder.0,
            folder.1,
            guid = uuid::Uuid::new_v4().to_string().to_uppercase(),
            muted = u8::from(muted),
        )
    };
    let mut tracks = track("MIX", Some((file, kind)), false, (0, 0));
    // Its separation, when it sits beside it: each stem muted under the
    // mix, as a REAPER project of a record and its stems has them.
    let stems = stems_beside(recording);
    let stem = |name: &str| {
        stems
            .iter()
            .find(|(stem, _)| stem.eq_ignore_ascii_case(name))
    };
    let kit: Vec<&(String, String)> = KIT_STEMS.iter().filter_map(|name| stem(name)).collect();
    for name in ["Vocals"] {
        if let Some((_, file)) = stem(name) {
            tracks += &track(&name.to_uppercase(), Some((file, "WAVE")), true, (0, 0));
        }
    }
    if !kit.is_empty() {
        tracks += &track("DRUMS", None, false, (1, 1));
        for (i, (name, file)) in kit.iter().enumerate() {
            let last = i + 1 == kit.len();
            let folder = if last { (2, -1) } else { (0, 0) };
            tracks += &track(name, Some((file, "WAVE")), true, folder);
        }
    }
    for name in ["Bass", "Guitar", "Piano", "Other"] {
        if let Some((_, file)) = stem(name) {
            tracks += &track(&name.to_uppercase(), Some((file, "WAVE")), true, (0, 0));
        }
    }
    let project = format!("<REAPER_PROJECT 0.1 \"7.0/studio\" 0\n{tempo}{tracks}>\n");
    let rpp = recording.with_file_name(format!("{title}.RPP"));
    std::fs::write(&rpp, project)?;
    // No click, count or cues: with no bar line known they would play
    // against the recording, not with it.
    let prepare = Prepare {
        organize: true,
        chart: None,
        guide: false,
    };
    save_prepared(&rpp, &prepare, Some(title))
}

/// The kit's stems, in the order they are laid out.
#[cfg(feature = "native")]
const KIT_STEMS: [&str; 6] = ["Kick", "Snare", "Hi-Hat", "Toms", "Cymbals", "Drums Other"];

/// The stems of a separation that sit beside `recording`: every
/// `<recording's name> (<Stem>).wav` in its folder, as `(stem, file name)`.
#[cfg(feature = "native")]
fn stems_beside(recording: &std::path::Path) -> Vec<(String, String)> {
    let (Some(dir), Some(name)) = (
        recording.parent(),
        recording.file_stem().and_then(std::ffi::OsStr::to_str),
    ) else {
        return Vec::new();
    };
    let prefix = format!("{name} (");
    let mut stems: Vec<(String, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|file| {
            let stem = file
                .strip_prefix(&prefix)?
                .strip_suffix(".wav")?
                .strip_suffix(')')?
                .to_owned();
            Some((stem, file))
        })
        .collect();
    stems.sort();
    stems
}

/// Lay an edited chart over the open session: rebuild the song's structure
/// from it (tempo, markers, section regions, key and chords — see
/// [`engine_domain::keyflow::from_chart::rebuild_from_chart`]) and, when `guide`,
/// regenerate the click, count and cues to match.
///
/// The guide needs nothing cleared first: it writes one item per role over
/// the whole song, from the count-in, and every song starts there — so the
/// old song's item always overlaps the new one's span, and generating
/// replaces it.
///
/// The live half of editing a chart: Organize mode calls this after each
/// pause in typing. Text that does not parse changes nothing.
///
/// # Errors
///
/// The chart does not parse, or a step failed.
pub fn apply_chart(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    chart: &str,
    guide: bool,
) -> eyre::Result<engine_domain::keyflow::from_chart::ChartRebuilt> {
    let project = ProjectContext::Project(project_guid.to_owned());
    let rebuilt = engine_domain::keyflow::from_chart::rebuild_from_chart(daw, &project, chart)?;
    if guide {
        engine_domain::guide::Guide::new(daw.clone())
            .with_instrument(GUIDE_INSTRUMENT)
            .generate(engine_domain::guide::GuideScope::All)
            .map_err(|e| eyre::eyre!("guide: {e}"))?;
    }
    Ok(rebuilt)
}

/// The FX the Click / Count / Guide tracks play through
/// (`guide_instrument::IDENT`).
pub const GUIDE_INSTRUMENT: &str = "fts.guide";

/// The steps themselves, on any host: the native window's opened engine,
/// or the web demo's (which has the chart's text rather than a file).
///
/// # Errors
///
/// The step that failed, and why.
pub fn steps(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    organize: bool,
    chart: Option<&str>,
    guide: bool,
) -> eyre::Result<()> {
    let project = ProjectContext::Project(project_guid.to_owned());

    let mut target = dynamic_template::apply::DawTarget::on(daw.clone(), project.clone());
    // A record and its separation (a REAPER project with the mix, its
    // stems and a click): something to play along to. It is organized
    // like any song, and left unmixed: a level staged or a chain added
    // and the stems no longer add up to the record.
    let separated = organize && dynamic_template::apply::is_separated_record(&target);
    if organize {
        // Folders first — Drums, Bass, Guitars … with any wrapper the
        // multitrack came in taken apart — so the bus pass routes
        // groups rather than loose tracks.
        let arranged = target.arrange_into_groups()?;
        tracing::info!(
            unwrapped = ?arranged.unwrapped,
            folders = ?arranged.created,
            placed = arranged.placed,
            "prepare: arranged into groups"
        );
        let organized = dynamic_template::apply::organize(&mut target)
            .map_err(|e| eyre::eyre!("organize: {e}"))?;
        let hidden = target.hide_captures()?;
        let buses = target.hide_buses()?;
        let triggers = target.mute_triggers()?;
        // The golden template's drum Process folder: the kit's parallel
        // compressors and rooms, sent from the kit, to DRUM BUS.
        if !separated {
            mix_levels(daw, project_guid, &project, &mut target)?;
        }
        tracing::info!(
            hidden = ?hidden,
            buses = ?buses,
            triggers = ?triggers,
            "prepare: captures and buses hidden, triggers muted"
        );
        tracing::info!(
            tracks = organized.existing,
            buses = organized.buses.len(),
            unsorted = organized.unsorted.len(),
            unrouted = ?organized.routing.unrouted,
            "prepare: organized"
        );
    }
    if chart.is_none() {
        // No chart to lay the song out: its structure is the multitrack's
        // own markers — the sections made regions, the song's region and
        // bounds placed, the notes left marks — before the guide, whose
        // cues follow the sections.
        let mut converted = engine_domain::keyflow::actions::convert_markers_in(
            daw,
            &project,
            song_name(daw, &project).as_deref(),
            Some(song_end(daw, project_guid, &project)),
        )
        .map_err(|e| eyre::eyre!("markers: {e}"))?;
        let dropped = drop_title_markers(daw, &project);
        converted.marks.retain(|mark| !dropped.contains(mark));
        tracing::info!(
            dropped = ?dropped,
            sections = ?converted.sections,
            marks = ?converted.marks,
            song = ?converted.song,
            "prepare: markers made the song's sections"
        );
    }
    if let Some(text) = chart {
        // Rebuild, not build: a multitrack can arrive with structure of its
        // own — the importer's guessed `Cue N` sections, its SONGSTART —
        // and the chart replaces it rather than landing beside it.
        let built = engine_domain::keyflow::from_chart::rebuild_from_chart(daw, &project, text)
            .map_err(|e| eyre::eyre!("chart: {e}"))?
            .built;
        tracing::info!(
            title = %built.title,
            bpm = built.tempo_bpm,
            sections = built.sections,
            "prepare: song built from chart"
        );
        // The key and the chords read from the ruler's CHORDS lane, so
        // their tracks are out of the track panel, and LINES and HITS
        // with them until something writes melodies and hits there; the
        // Keyflow folder stays, for its Lyrics track — the lines as
        // items, to be moved and trimmed where the song has them.
        for track in ["KEY", "CHORD", "LINES", "HITS"] {
            target.hide_in_tcp(track)?;
        }
    }
    if guide {
        engine_domain::guide::Guide::new(daw.clone())
            .with_instrument(GUIDE_INSTRUMENT)
            .generate(engine_domain::guide::GuideScope::All)
            .map_err(|e| eyre::eyre!("guide: {e}"))?;
        tracing::info!("prepare: click and guide generated");
        // The multitrack's own click and guide stems, renamed and
        // muted by the guide: kept for reference, out of the panel.
        target.hide_in_tcp("Click Audio")?;
        target.hide_in_tcp("Guide Audio")?;
    }
    if organize && separated {
        tracing::info!("prepare: a record and its separation, left unmixed");
    }
    if organize && !separated {
        // Last, after the renders that read the song — the kit's level,
        // where the sound ends — which the reverbs and the pitch shifter
        // below would make cost a minute. The gates change the kit's
        // loudest moment little.
        //
        // The golden template's kit and vocal returns, made of the
        // session's tracks: each piece's Sum and what the template puts
        // beside it, the toms as pieces, the Vox FX the leads feed — then
        // each piece's gate keyed from its trigger where it now sits.
        let kit = target.apply_golden_kit()?;
        // What each kit track is, on the track: the scenes read it there.
        let marked = target.mark_kit_kinds()?;
        let vox_fx = target.apply_vox_fx()?;
        let keyed = target.key_drum_gates()?;
        tracing::info!(
            kit = ?kit,
            kinds_marked = marked,
            vox_fx = vox_fx.len(),
            keyed = ?keyed,
            "prepare: golden template"
        );
        // The mix's processing: the FTS processors built in (Tone on every
        // recorded track and EQ/Comp on the buses, bypassed; the Process
        // chains set).
        #[cfg(feature = "native")]
        {
            let chains = crate::mix_chains::add(daw, &project)?;
            tracing::info!(
                tone = chains.tone,
                buses = chains.buses,
                process = chains.process,
                gates = ?chains.gates,
                one_notes = ?chains.one_notes,
                returns = chains.returns,
                mics = ?chains.mics,
                unknown = ?chains.unknown,
                "prepare: mix chains"
            );
        }
    }
    if organize {
        // The template's top-level order — what is played to first,
        // then the song, then the instruments, routing last — and the
        // routing out of the track panel.
        target.order_top_level(
            &["Guide", "Keyflow"],
            &[
                dynamic_template::apply::REFERENCES_FOLDER,
                "CLICK + GUIDE BUS",
                "MIX BUS",
                "UNSORTED",
            ],
        )?;
        target.hide_in_tcp("CLICK + GUIDE BUS")?;
        target.hide_in_tcp("MIX BUS")?;
        // Last of all, so it stays last: the song's stereo reference in
        // its own top-level folder, and the song heard by it when that is
        // how it came (every part muted under the mix).
        // The engine's (native): a browser prepares nothing by reference.
        #[cfg(feature = "native")]
        {
            let reference = crate::instrument_tracks::file_reference(daw, &project)
                .map_err(|e| eyre::eyre!("reference: {e}"))?;
            let by_reference = crate::instrument_tracks::settle_hearing(daw, &project)
                .map_err(|e| eyre::eyre!("reference: {e}"))?;
            tracing::info!(
                reference = ?reference,
                heard_by_reference = by_reference,
                "prepare: the song's reference"
            );
        }
    }
    Ok(())
}

/// The kit's parallel chains and the song's levels: the golden template's
/// drum Process folder (the kit's compressors and rooms, sent from the
/// kit, to DRUM BUS), the recorded tracks staged to the reference kit's
/// levels, and the kit's folder at the session's reference.
fn mix_levels(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    project: &ProjectContext,
    target: &mut dynamic_template::apply::DawTarget<daw_standalone::sync::Standalone>,
) -> eyre::Result<()> {
    let process = target.add_drum_process()?;
    tracing::info!(process = process.len(), "prepare: drum process");
    // The recorded tracks at the reference kit's levels, as item
    // volume: nothing rendered, the audio as it was recorded.
    let staged = crate::gain_stage::stage(daw, project)?;
    // And the kit at the session's reference, −23 LUFS short-term at
    // its loudest: its folder's fader.
    #[cfg(feature = "native")]
    if let Some((loudest, fader)) =
        crate::gain_stage::kit_headroom(daw, project_guid, 0.0, media_end(daw, project).latest)
    {
        tracing::info!(
            kit.short_term_lufs = loudest,
            kit.fader_db = fader,
            "prepare: kit level"
        );
    }
    #[cfg(not(feature = "native"))]
    let _ = project_guid;
    tracing::info!(
        staged = staged.staged.len(),
        silent = ?staged.silent,
        toms = ?staged.toms,
        "prepare: gain staged"
    );
    for each in &staged.staged {
        tracing::debug!(
            track = %each.name,
            role = ?each.role,
            level_db = each.level,
            gain_db = each.gain_db,
            "prepare: staged"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::names_an_artist;

    /// A REAPER project's ruler: `lanes` its `RULERLANE` lines, then two
    /// regions side by side on its first lane and a note on its second.
    #[cfg(feature = "native")]
    fn marked_up(lanes: &str) -> (daw::standalone::Standalone, daw::service::ProjectContext) {
        let daw = daw::standalone::Standalone::new();
        let text = format!(
            "<REAPER_PROJECT 0.1 \"7.0/test\" 0\n{lanes}\
  MARKER 1 2 Intro 1 0 1 B {{00000000-0000-0000-0000-000000000001}} 0 1\n\
  MARKER 1 10 \"\" 1\n\
  MARKER 2 10 \"Chorus 1 (Spanish)\" 1 0 1 B {{00000000-0000-0000-0000-000000000002}} 0 1\n\
  MARKER 2 20 \"\" 1\n\
  MARKER 101 4 \"Kick in\" 0 0 1 B {{00000000-0000-0000-0000-000000000003}} 0 2\n>\n"
        );
        let loaded = daw::standalone::project_loader::load_rpp_text(&daw, "Song", "", &text)
            .expect("the project loads");
        (
            daw,
            daw::service::ProjectContext::Project(loaded.project_guid),
        )
    }

    /// A REAPER project marked up with plain regions (its ruler lanes
    /// unnamed) has them as its sections: not the note that reads like
    /// one, which stays a mark.
    #[cfg(feature = "native")]
    #[test]
    fn a_reaper_projects_own_regions_are_its_sections() {
        use daw::service::{Markers, Regions};
        use engine_domain::proto::ruler_lanes::CoreLane;
        let (daw, project) = marked_up("  RULERLANE 1 4 \"\" 0 -1\n  RULERLANE 2 8 \"\" 0 -1\n");
        let converted =
            engine_domain::keyflow::actions::convert_markers_in(&daw, &project, Some("Song"), None)
                .expect("converted");
        assert_eq!(converted.sections, ["Intro", "Chorus 1 (Spanish)"]);
        assert_eq!(converted.song, Some((2.0, 20.0)));
        assert_eq!(converted.marks, ["Kick in"]);
        let regions = Regions::all(&daw, project.clone());
        let on = |lane: CoreLane| -> Vec<String> {
            let mut names: Vec<_> = regions
                .iter()
                .filter(|r| r.lane == Some(lane.lane_index()))
                .map(|r| (r.start_seconds(), r.name.clone()))
                .collect();
            names.sort_by(|a, b| a.0.total_cmp(&b.0));
            names.into_iter().map(|(_, name)| name).collect()
        };
        assert_eq!(on(CoreLane::Sections), ["Intro", "Chorus 1 (Spanish)"]);
        assert_eq!(on(CoreLane::Song), ["Song"]);
        let at = |name: &str| {
            Markers::all(&daw, project.clone())
                .into_iter()
                .find(|m| m.name == name)
                .and_then(|m| m.position.seconds())
        };
        assert_eq!(at("SONGSTART"), Some(2.0));
        assert_eq!(at("SONGEND"), Some(20.0));
        assert_eq!(at("Kick in"), Some(4.0));
    }

    /// Regions side by side on a lane the project names `SONG` are a
    /// set's songs, and stay there.
    #[cfg(feature = "native")]
    #[test]
    fn regions_on_a_lane_named_song_are_songs() {
        use daw::service::Regions;
        use engine_domain::proto::ruler_lanes::CoreLane;
        let (daw, project) = marked_up("  RULERLANE 1 8 SONG 0 -1\n  RULERLANE 2 4 MARKS 0 -1\n");
        let converted =
            engine_domain::keyflow::actions::convert_markers_in(&daw, &project, Some("Song"), None)
                .expect("converted");
        assert!(
            !converted.sections.contains(&"Intro".to_owned()),
            "{converted:?}"
        );
        assert!(
            Regions::all(&daw, project)
                .iter()
                .filter(|r| r.name == "Intro" || r.name.starts_with("Chorus"))
                .all(|r| r.lane == Some(CoreLane::Song.lane_index()))
        );
    }

    /// A chart laid over an empty song: its meters on the tempo map from
    /// 0 (the opening meter there too, so a song saved and opened again
    /// starts in it), and a section with a name of its own named so.
    #[cfg(feature = "native")]
    fn applied(chart: &str) -> (daw::standalone::Standalone, daw::service::ProjectContext) {
        let daw = daw::standalone::Standalone::new();
        let loaded = daw::standalone::project_loader::load_rpp_text(
            &daw,
            "Song",
            "",
            "<REAPER_PROJECT 0.1 \"7.0/test\" 0\n>\n",
        )
        .expect("the project loads");
        // The insert actions it stamps through spawn: a runtime to spawn on.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let _in = runtime.enter();
        super::apply_chart(&daw, &loaded.project_guid, chart, false).expect("applied");
        let project = daw::service::ProjectContext::Project(loaded.project_guid);
        (daw, project)
    }

    #[cfg(feature = "native")]
    fn meters(
        daw: &daw::standalone::Standalone,
        project: &daw::service::ProjectContext,
    ) -> Vec<(f64, u32, u32)> {
        use daw::service::TempoMap;
        TempoMap::get_tempo_points(daw, project.clone())
            .into_iter()
            .filter_map(|p| {
                let ts = p.time_signature?;
                Some((
                    (p.position.seconds().unwrap_or(0.0) * 1000.0).round() / 1000.0,
                    ts.numerator,
                    ts.denominator,
                ))
            })
            .collect()
    }

    /// A song in 3/4 throughout is in 3/4 from 0: a point says so.
    #[cfg(feature = "native")]
    #[test]
    fn a_chart_in_three_four_is_three_four_from_the_start() {
        let (daw, project) =
            applied("Waltz Song - Test\n#A 90bpm 3/4\n\nCount 2\n\nVS 4\nA\nD\nE\nA\n");
        assert_eq!(meters(&daw, &project), [(0.0, 3, 4)]);
    }

    /// Bars of 3/4 in a song of 4/4: the song opens in 4/4 at 0, and each
    /// change lands at its own bar.
    #[cfg(feature = "native")]
    #[test]
    fn a_meter_change_lands_on_its_bar_after_the_opening_meter() {
        // 60 BPM: a 4/4 bar is 4 s, a 3/4 bar 3 s. Count 1 (4 s), then IN:
        // a bar of 4/4, a bar of 3/4, a bar of 4/4.
        const TURN: &str =
            "Meter Song - Test\n#C 60bpm 4/4\n\nCount 1\n\nIN 3\nC\n!T3/4 F\n!T4/4 G\n";
        let layout = engine_domain::setlist::chart_import::chart_to_layout(TURN).expect("laid out");
        assert_eq!(
            layout.meter_changes,
            [(8.0, 3, 4), (11.0, 4, 4)],
            "the layout"
        );
        let (daw, project) = applied(TURN);
        assert_eq!(
            meters(&daw, &project),
            [(0.0, 4, 4), (8.0, 3, 4), (11.0, 4, 4)]
        );
    }

    /// A section with a name of its own (`[End]`) is a region of that
    /// name, not a generic instrumental one.
    #[cfg(feature = "native")]
    #[test]
    fn a_section_with_a_name_of_its_own_keeps_it() {
        use daw::service::Regions;
        use engine_domain::proto::ruler_lanes::CoreLane;
        let (daw, project) =
            applied("Named Song - Test\n#C 120bpm 4/4\n\nVS 2\nC\nG\n\n[End] 1\nC\n");
        let names: Vec<String> = Regions::all(&daw, project)
            .into_iter()
            .filter(|r| r.lane == Some(CoreLane::Sections.lane_index()))
            .map(|r| r.name)
            .collect();
        assert!(names.iter().any(|n| n == "End"), "{names:?}");
    }

    /// A section that repeats (`|: … :|x4`) lasts as long as it is played:
    /// the sections after it start after every pass, as the recording
    /// plays them and as Keyflow's chart counts its bars.
    #[cfg(feature = "native")]
    #[test]
    fn a_repeat_lasts_as_long_as_it_is_played() {
        // 60 BPM 4/4: a bar is 4 s. VS 2 bars (0-8 s), OUT 2 written bars
        // played 3 times (8-32 s), [End] 1 bar (32-36 s).
        const VAMP: &str =
            "Vamp Song - Test\n#C 60bpm 4/4\n\nVS 2\nC\nG\n\nOUT 2\n|: F | G :|x3\n\n[End] 1\nC\n";
        let layout = engine_domain::setlist::chart_import::chart_to_layout(VAMP).expect("laid out");
        let spans: Vec<(f64, f64, u32)> = layout
            .sections
            .iter()
            .map(|s| (s.start_seconds, s.end_seconds, s.measures))
            .collect();
        assert_eq!(spans, [(0.0, 8.0, 2), (8.0, 32.0, 6), (32.0, 36.0, 1)]);
        assert_eq!(layout.measure_starts.len(), 9, "every bar as played");
        assert_eq!(layout.song_end_seconds, 36.0);
    }

    #[test]
    fn filing_folders_do_not_name_an_artist() {
        assert!(names_an_artist("crescendum-2026-10-08"));
        assert!(names_an_artist("Elevation Rhythm"));
        for folder in [
            "streamed",
            "Session",
            "Sessions",
            "share-677ef0a3",
            "task-washed",
        ] {
            assert!(!names_an_artist(folder), "{folder}");
        }
    }
}

// ── The chart, laid over the song ────────────────────────────────────

#[cfg(feature = "native")]
static CHART_APPLY: architect::action::ActionMeta = crate::actions::meta(
    "CHART_APPLY",
    "chart_apply",
    "Lay a chart over the song",
    "Rebuild song `song` (its engine project; the current one when left out) from keyflow chart `text`: its tempo, meter and sections as the chart says, the click, count and cues generated again, and the song saved behind when it was opened from a `.session`. Text that does not parse changes nothing. Says its sections, tempo and where the song starts. What the chart editor does as typing pauses. One undo step.",
    "Structure",
    "",
);

/// Which song, and the chart.
#[cfg(feature = "native")]
#[derive(facet::Facet)]
struct ChartApplyArg {
    song: Option<String>,
    text: String,
}

/// What a chart laid over a song came to.
#[derive(facet::Facet, Clone, Debug, PartialEq)]
pub struct ChartApplied {
    pub sections: u32,
    pub bpm: f64,
    /// Seconds.
    pub song_start: f64,
}

/// `CHART_APPLY` onto `list`.
///
/// # Errors
///
/// The list refused it (a duplicate id).
#[cfg(feature = "native")]
pub fn register(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(
        &CHART_APPLY,
        architect::action::Effect::Mutating,
        |a: ChartApplyArg| {
            let daw = crate::open::local_engine().ok_or("no song open in the engine")?;
            let song = a
                .song
                .filter(|s| !s.is_empty())
                .or_else(crate::open::current_song)
                .ok_or("no song open in the engine")?;
            // The rebuild and the guide spawn on the engine's runtime: run
            // inside it, whichever thread the action came from (a key's
            // runs on a plain thread behind the window).
            let runtime = crate::open::engine_runtime().map_err(|e| e.to_string())?;
            let _inside = runtime.enter();
            let rebuilt = apply_chart(daw, &song, &a.text, true).map_err(|e| e.to_string())?;
            // Saved where the song was opened from, when that is a `.session`.
            let saved = daw
                .read_project(&song, |p| std::path::PathBuf::from(&p.info.path))
                .filter(|path| crate::open::is_session(path));
            if let Some(dir) = saved {
                crate::session_file::save_session_behind(daw, &song, &dir);
            }
            crate::song_model::request_resync();
            Ok(ChartApplied {
                sections: u32::try_from(rebuilt.built.sections).unwrap_or(u32::MAX),
                bpm: rebuilt.built.tempo_bpm,
                song_start: rebuilt.built.song_start_seconds,
            })
        },
    )?;
    Ok(())
}

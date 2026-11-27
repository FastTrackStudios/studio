//! A new session, three ways — `SESSION_NEW`:
//!
//! - **from scratch**: an empty song ([`crate::project_new`]) given a
//!   title, a key, a tempo and a form (its sections in order, each a kind
//!   and how many bars), saved where `to` says;
//! - **from a folder** of multitracks or stems: each audio file in it (and
//!   in folders inside it) becomes a track playing that file from bar one.
//!   The folder gets a REAPER project of them, `<title>.RPP`, which opens
//!   as any `.RPP` does: saved as `<title>.session` beside it;
//! - **from a REAPER project**: the `.RPP` opened as SESSION_OPEN opens
//!   one, which saves it as the `.session` beside it with its tracks,
//!   items, markers and tempo.
//!
//! Each opens as the engine's set, a set of one, in the background:
//! `SESSION_OPENING` says when it is there. `SONG_INFO` reads the song
//! back: its title, file, tempo, meter, key, tracks, items and markers.

use std::path::{Path, PathBuf};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Items as _, ProjectContext, TempoMap as _, TrackRef};
use facet::Facet;

use crate::setlist::{Setlist, SetlistOpen as _};

static NEW: ActionMeta = crate::actions::meta(
    "SESSION_NEW",
    "session_new",
    "New session",
    "Make a new session, one of three ways. From scratch (no `from`): an empty song — COUNT-IN on bar 1, SONGSTART after the two-bar count-in, SONGEND eight bars on, the guide (Click, Count, Guide) over them, the window in Organize mode — with `title`, `key` (`Eb`, `C#m`, `D minor`), `tempo` (bpm) and `form`, its sections from SONGSTART in order, each a kind and its bars (`[\"Intro 4\", \"Verse 8\", \"Chorus\"]`; intro and outro are 4 bars when not said, the rest 8), each given; saved to `to` (a `.session`, named for the song: `<title>.session`; the title is its name when `title` is left out) when given, else untitled until its first SESSION_SAVE with `to`. From a folder (`from` a folder of multitracks or stems): a track per audio file in it or in folders inside it (WAV, FLAC, MP3, Ogg, AIFF), each playing its file from the start, at `tempo` (120 when not said), named `title` (the folder's name when not said), in `key` when given; the folder gets `<title>.RPP` and `<title>.session`. From a REAPER project (`from` an `.RPP`): it opens as SESSION_OPEN opens one and is saved as the `.session` beside it, its tracks, items, markers and tempo as they are; refused when that `.session` is already there (open it). The song opens in the background as the engine's set: the action returns at once, and the song is there once SESSION_OPENING is idle. The song open now is left as it is — SESSION_SAVE it first if it should be. SONG_INFO reads the new song back.",
    "Session",
    "",
);

static INFO: ActionMeta = crate::actions::meta(
    "SONG_INFO",
    "song_info",
    "About the song",
    "The current song: its `title`, its `file` (none until it is first saved), its tempo (`bpm`) and `meter` at the start, how many `tempo_points` its map has, its `key` (the KEY track's first, `Eb major`), how many `tracks` and `items`, and its `markers` (each `name` and `at`, seconds; sections are SESSION_SECTIONS). Read-only.",
    "Song",
    "",
);

/// `SESSION_NEW`'s arguments.
#[derive(Facet)]
struct NewArg {
    title: Option<String>,
    key: Option<String>,
    tempo: Option<f64>,
    form: Option<Vec<String>>,
    from: Option<String>,
    to: Option<String>,
    /// A template to start from — none yet.
    template: Option<String>,
}

/// What a new session is made from, as asked and checked.
#[derive(Debug, Clone, PartialEq)]
pub enum Start {
    /// An empty song, given what it is.
    Scratch(Scratch),
    /// A folder of stems, a track per file.
    Folder {
        dir: PathBuf,
        title: String,
        bpm: f64,
        key: Option<String>,
    },
    /// A REAPER project, as it is.
    Rpp(PathBuf),
}

/// A song from scratch: what it is given.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scratch {
    pub title: Option<String>,
    /// As asked (`Eb`, `C#m`): checked by [`key_named`].
    pub key: Option<String>,
    pub bpm: Option<f64>,
    pub form: Vec<Section>,
    /// The `.session` it is saved to.
    pub to: Option<PathBuf>,
}

/// One section of a form: its kind (as SECTION_INSERT takes it) and bars.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub kind: String,
    pub bars: u32,
}

impl Start {
    /// What the song is called while it opens.
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::Scratch(s) => s
                .title
                .clone()
                .unwrap_or_else(|| crate::project_new::UNTITLED.to_owned()),
            Self::Folder { title, .. } => title.clone(),
            Self::Rpp(path) => stem(path),
        }
    }
}

/// The slowest and fastest tempo taken, as TEMPO_SET takes them.
const BPM: std::ops::RangeInclusive<f64> = 10.0..=400.0;

/// A folder of stems' tempo when none is said.
const FOLDER_BPM: f64 = 120.0;

/// The audio files a folder's tracks are made of, by extension.
const AUDIO: &[&str] = &["wav", "flac", "mp3", "ogg", "oga", "aif", "aiff"];

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_rpp(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("rpp"))
}

/// A section of a form as written: `Verse 8`, `Pre-Chorus 4`, `Chorus` (its
/// kind's default: intro and outro 4 bars, the rest 8).
///
/// # Errors
///
/// It names no kind, or its bars are not a whole number above 0.
pub fn section(text: &str) -> Result<Section, String> {
    let text = text.trim();
    let (kind, bars) = match text.rsplit_once(char::is_whitespace) {
        Some((kind, bars)) if bars.chars().all(|c| c.is_ascii_digit()) => {
            let bars: u32 = bars
                .parse()
                .map_err(|_| format!("{text:?}: {bars} is not a number of bars"))?;
            (kind.trim(), Some(bars))
        }
        _ => (text, None),
    };
    if kind.is_empty() {
        return Err(format!(
            "{text:?}: a section is its kind, then its bars (`Verse 8`)"
        ));
    }
    if bars == Some(0) {
        return Err(format!("{text:?}: a section is a bar long at the least"));
    }
    let squashed: String = kind
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let short = matches!(squashed.as_str(), "intro" | "in" | "outro");
    Ok(Section {
        kind: kind.to_owned(),
        bars: bars.unwrap_or(if short { 4 } else { 8 }),
    })
}

/// The key `text` names, as the KEY track writes it: `Eb` and `Eb major`
/// are Eb major, `C#m` and `C# minor` C# minor, `D dorian` D dorian.
///
/// # Errors
///
/// It is not a key.
pub fn key_named(text: &str) -> Result<keyflow::key::Key, String> {
    use engine_domain::key::{key_from_name, parse_key};
    let text = text.trim();
    let named = parse_key(text).or_else(|| {
        if text.contains(char::is_whitespace) {
            return None;
        }
        match text.strip_suffix('m') {
            Some(root) if !root.is_empty() => key_from_name(root, false),
            _ => key_from_name(text, true),
        }
    });
    named.ok_or_else(|| {
        format!("{text:?} is not a key: a root (`Eb`), with `m` or a mode (`C#m`, `D minor`)")
    })
}

/// What `SESSION_NEW`'s arguments ask for, checked against the disk.
///
/// # Errors
///
/// What was asked does not make a session: an argument the way does not
/// take, a tempo out of range, a key or section that is not one, a `from`
/// that is neither a folder nor an `.RPP`, or a session already there.
fn start_of(a: NewArg) -> Result<Start, String> {
    if let Some(template) = a.template.filter(|t| !t.trim().is_empty()) {
        return Err(format!(
            "no templates yet ({template}): leave `template` out"
        ));
    }
    let title = a
        .title
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty());
    if let Some(title) = &title
        && title.contains(['/', '\\'])
    {
        return Err(format!(
            "{title:?}: a title names the song's files, so it has no `/` or `\\` in it"
        ));
    }
    let key = a.key.filter(|k| !k.trim().is_empty());
    if let Some(key) = &key {
        key_named(key)?;
    }
    if let Some(bpm) = a.tempo
        && !BPM.contains(&bpm)
    {
        return Err(format!(
            "{bpm} bpm is not a tempo: {} to {}",
            BPM.start(),
            BPM.end()
        ));
    }
    let form = a
        .form
        .unwrap_or_default()
        .iter()
        .map(|s| section(s))
        .collect::<Result<Vec<_>, _>>()?;
    let Some(from) = a.from.filter(|f| !f.trim().is_empty()) else {
        let to = a.to.filter(|t| !t.trim().is_empty()).map(PathBuf::from);
        if let Some(to) = &to {
            if !crate::open::is_session(to) {
                return Err(format!(
                    "`to` is the `.session` it is saved to, not {}",
                    to.display()
                ));
            }
            if to.exists() {
                return Err(format!(
                    "{} is already there: open it (SESSION_OPEN), or save the new one elsewhere",
                    to.display()
                ));
            }
            // A session is named for its file.
            let named = stem(to);
            if title.as_ref().is_some_and(|t| *t != named) {
                return Err(format!(
                    "a session is named for its file: save {title:?} as `{}.session`, or leave `title` out",
                    title.as_deref().unwrap_or_default()
                ));
            }
        }
        let title = title.or_else(|| to.as_deref().map(stem));
        return Ok(Start::Scratch(Scratch {
            title,
            key,
            bpm: a.tempo,
            form,
            to,
        }));
    };
    let from = PathBuf::from(from);
    if !form.is_empty() {
        return Err(
            "`form` is for a song from scratch: a song from `from` has the form it has".to_owned(),
        );
    }
    if a.to.is_some_and(|t| !t.trim().is_empty()) {
        return Err(
            "`to` is for a song from scratch: a song from `from` is saved beside what it is made from".to_owned(),
        );
    }
    if from.is_dir() {
        let title = title.unwrap_or_else(|| {
            from.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| crate::project_new::UNTITLED.to_owned())
        });
        for made in [format!("{title}.session"), format!("{title}.RPP")] {
            let there = from.join(&made);
            if there.exists() {
                return Err(format!(
                    "{} is already there: open it (SESSION_OPEN), or name the new one with `title`",
                    there.display()
                ));
            }
        }
        return Ok(Start::Folder {
            dir: from,
            title,
            bpm: a.tempo.unwrap_or(FOLDER_BPM),
            key,
        });
    }
    if !is_rpp(&from) {
        return Err(format!(
            "{} is neither a folder of stems nor a REAPER project (`.RPP`)",
            from.display()
        ));
    }
    if !from.is_file() {
        return Err(format!("{} is not there", from.display()));
    }
    if title.is_some() || key.is_some() || a.tempo.is_some() {
        return Err(
            "a REAPER project keeps its own title, key and tempo: leave `title`, `key` and `tempo` out".to_owned(),
        );
    }
    let session = from.with_extension("session");
    if session.exists() {
        return Err(format!(
            "{} is already a session: open it (SESSION_OPEN)",
            session.display()
        ));
    }
    Ok(Start::Rpp(from))
}

/// One stem of a folder: the file, from the folder, and how long it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Stem {
    /// The file's path from the folder (`Drums/Kick.wav`).
    pub file: PathBuf,
    pub seconds: f64,
}

/// The audio files in `dir` and the folders inside it (hidden ones left
/// out), in path order, each with its length.
///
/// # Errors
///
/// The folder cannot be read, it holds no audio, or a file's length cannot
/// be read.
pub fn stems(dir: &Path) -> Result<Vec<Stem>, String> {
    let mut files = Vec::new();
    collect(dir, Path::new(""), &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!(
            "{} holds no audio ({})",
            dir.display(),
            AUDIO.join(", ")
        ));
    }
    files
        .into_iter()
        .map(|file| {
            let seconds = length_of(&dir.join(&file))?;
            Ok(Stem { file, seconds })
        })
        .collect()
}

fn collect(root: &Path, at: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let dir = root.join(at);
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let rel = at.join(&name);
        let path = entry.path();
        // A link to a file is a stem; a link to a folder is not followed
        // (one to a folder above would never end).
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if kind.is_dir() {
            collect(root, &rel, out)?;
        } else if (kind.is_file() || (kind.is_symlink() && path.is_file()))
            && path
                .extension()
                .is_some_and(|e| AUDIO.iter().any(|a| e.eq_ignore_ascii_case(a)))
        {
            out.push(rel);
        }
    }
    Ok(())
}

/// How long the recording at `path` is: a WAV's header, anything else
/// decoded.
fn length_of(path: &Path) -> Result<f64, String> {
    let wav = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
    let seconds = if wav {
        fts_sample::probe(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .duration_secs()
    } else {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        daw::standalone::audio_engine::decode_audio_with_extension(&bytes, &ext)
            .ok_or_else(|| format!("{} would not decode", path.display()))?
            .duration_seconds()
    };
    if seconds > 0.0 {
        Ok(seconds)
    } else {
        Err(format!("{} holds no audio", path.display()))
    }
}

/// The REAPER project of a folder's stems: a track per stem, named for its
/// file, playing it from the start, at `bpm` in 4/4.
#[must_use]
pub fn stems_project(stems: &[Stem], bpm: f64) -> String {
    use dawfile_reaper::RppSerialize as _;
    use dawfile_reaper::builder::ReaperProjectBuilder;
    let mut builder = ReaperProjectBuilder::new()
        .tempo_with_time_sig(bpm, 4, 4)
        .sample_rate(48_000);
    for s in stems {
        let name = stem(&s.file);
        // Forward slashes whatever the system: the project's paths are
        // REAPER's, from the project's folder.
        let file = s.file.to_string_lossy().replace('\\', "/");
        let ext = s
            .file
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let item_name = name.clone();
        builder = builder.track(&name, |track| {
            track.item(0.0, s.seconds, |item| {
                let item = item.name(item_name);
                match ext.as_str() {
                    "flac" => item.source_flac(file),
                    "mp3" => item.source_mp3(file),
                    "ogg" | "oga" => item.take(file, dawfile_reaper::types::SourceType::Vorbis),
                    // REAPER's WAVE source reads AIFF as well.
                    _ => item.source_wave(file),
                }
            })
        });
    }
    builder.build().to_rpp_string()
}

/// Make the song `start` asks for and open it as a set of one — the
/// background half of `SESSION_NEW` (`crate::studio_engine::set::open_new`).
///
/// # Errors
///
/// The song could not be made, opened, given what was asked or saved.
pub fn make(start: Start) -> eyre::Result<Setlist> {
    match start {
        Start::Scratch(scratch) => from_scratch(&scratch).map_err(|e| eyre::eyre!(e)),
        Start::Folder {
            dir,
            title,
            bpm,
            key,
        } => {
            let stems = stems(&dir).map_err(|e| eyre::eyre!(e))?;
            let rpp = dir.join(format!("{title}.RPP"));
            std::fs::write(&rpp, stems_project(&stems, bpm))
                .map_err(|e| eyre::eyre!("{}: {e}", rpp.display()))?;
            tracing::info!(
                song.new = "folder",
                song.stems = stems.len(),
                song.project = %rpp.display(),
                "a folder of stems made into a project"
            );
            let set = Setlist::open(&[rpp])?;
            if let Some(key) = key {
                set_key(&key).map_err(|e| eyre::eyre!(e))?;
                save(None).map_err(|e| eyre::eyre!(e))?;
            }
            Ok(set)
        }
        Start::Rpp(rpp) => Setlist::open(&[rpp]),
    }
}

/// An empty song given what `scratch` says, current, as a set of one.
fn from_scratch(scratch: &Scratch) -> Result<Setlist, String> {
    let guid = crate::project_new::create(None)?;
    let (daw, project) = crate::actions::song()?;
    if let Some(title) = &scratch.title {
        daw.write_project(&guid, |p| title.clone_into(&mut p.info.name));
    }
    if let Some(bpm) = scratch.bpm {
        // On their bars: COUNT-IN, SONGSTART and SONGEND move with it.
        run(
            "TEMPO_SET",
            &serde_json::json!({ "bpm": bpm, "follow": true }),
        )?;
    }
    if !scratch.form.is_empty() {
        lay_out(daw, &project, &scratch.form)?;
    }
    if let Some(key) = &scratch.key {
        set_key(key)?;
    }
    if let Some(to) = &scratch.to {
        save(Some(to))?;
    }
    let title = scratch
        .title
        .clone()
        .unwrap_or_else(|| crate::project_new::UNTITLED.to_owned());
    crate::project_new::setlist_titled(title, guid).map_err(|e| e.to_string())
}

/// The form's sections, one after another from SONGSTART, each over its
/// bars, through SECTION_INSERT (the Organize toolbar's).
fn lay_out(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    form: &[Section],
) -> Result<(), String> {
    let start = daw::service::Markers::all(daw, project.clone())
        .into_iter()
        .find(|m| m.name == "SONGSTART")
        .and_then(|m| m.position.seconds())
        .ok_or("the new song has no SONGSTART")?;
    let bpm = daw.get_tempo_at(project.clone(), start);
    let (beats, _) = daw.get_time_signature_at(project.clone(), start);
    let bar = 60.0 / bpm * f64::from(beats.max(1));
    let mut at = start;
    for s in form {
        let end = at + bar * f64::from(s.bars);
        run(
            "SECTION_INSERT",
            &serde_json::json!({ "kind": s.kind, "from": format!("{at}s"), "to": format!("{end}s") }),
        )
        .map_err(|e| format!("{} {}: {e}", s.kind, s.bars))?;
        at = end;
    }
    Ok(())
}

/// The current song in `key` from its start, on its KEY track.
fn set_key(key: &str) -> Result<(), String> {
    let key = key_named(key)?;
    let (daw, project) = crate::actions::song()?;
    engine_domain::key::set_key_at(daw, project, 0.0, &key).map_err(|e| format!("the key: {e}"))?;
    crate::song_model::request_resync();
    Ok(())
}

/// SESSION_SAVE, to `to` when given.
fn save(to: Option<&Path>) -> Result<(), String> {
    let args = to.map_or_else(
        || serde_json::json!({}),
        |to| serde_json::json!({ "to": to.display().to_string() }),
    );
    run("SESSION_SAVE", &args).map(|_| ())
}

/// Another action of the engine's, run here.
fn run(id: &str, args: &serde_json::Value) -> Result<String, String> {
    crate::actions::list()
        .run_json(id, args.to_string().as_bytes())
        .map(|out| String::from_utf8_lossy(&out).into_owned())
        .map_err(|e| format!("{id}: {e}"))
}

/// One marker, as SONG_INFO says it.
#[derive(Facet)]
struct MarkerRow {
    name: String,
    /// Seconds.
    at: f64,
}

/// The current song, as SONG_INFO says it.
#[derive(Facet)]
struct SongInfo {
    title: String,
    file: Option<String>,
    bpm: f64,
    meter: String,
    tempo_points: u32,
    key: Option<String>,
    tracks: u32,
    items: u32,
    markers: Vec<MarkerRow>,
}

fn song_info() -> Result<SongInfo, String> {
    let (daw, project) = crate::actions::song()?;
    let ProjectContext::Project(guid) = &project else {
        return Err("no song open".to_owned());
    };
    let (title, file) = daw
        .read_project(guid, |p| (p.info.name.clone(), p.info.path.clone()))
        .ok_or("no song open")?;
    let tracks = daw::service::Tracks::all(daw, project.clone());
    let items: usize = tracks
        .iter()
        .map(|t| {
            daw.get_items(project.clone(), TrackRef::Guid(t.guid.clone()))
                .len()
        })
        .sum();
    let (num, den) = daw.get_time_signature_at(project.clone(), 0.0);
    let mut markers: Vec<MarkerRow> = daw::service::Markers::all(daw, project.clone())
        .into_iter()
        .filter_map(|m| {
            Some(MarkerRow {
                at: m.position.seconds()?,
                name: m.name,
            })
        })
        .collect();
    markers.sort_by(|a, b| a.at.total_cmp(&b.at));
    Ok(SongInfo {
        title,
        file: Some(file).filter(|f| !f.is_empty()),
        bpm: daw.get_tempo_at(project.clone(), 0.0),
        meter: format!("{num}/{den}"),
        tempo_points: daw.tempo_point_count(project.clone()),
        key: engine_domain::key::key_changes(daw, &project)
            .first()
            .map(|c| engine_domain::key::format_key(&c.key)),
        tracks: u32::try_from(tracks.len()).unwrap_or(u32::MAX),
        items: u32::try_from(items).unwrap_or(u32::MAX),
        markers,
    })
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&NEW, Effect::Mutating, |a: NewArg| {
        let start = start_of(a)?;
        let title = start.title();
        crate::studio_engine::set::open_new(start).map_err(|e| e.to_string())?;
        Ok(format!(
            "{title}, opening in the background: wait for SESSION_OPENING to be idle before acting on it, then SONG_INFO reads it back."
        ))
    })?;
    list.register(&INFO, Effect::ReadOnly, |(): ()| song_info())?;
    Ok(())
}

#[cfg(test)]
mod tests;

//! A show recorded as one Live set, made a setlist of songs.
//!
//! A recital or a gig is often one Live set: every song end to end on one
//! arrangement, marked the way AbleSet reads it (`ableton::setlist`). [`split`]
//! turns it into songs Session opens one by one:
//!
//! 1. the set is converted to an `.RPP` beside it and opened (media left
//!    unloaded), then saved as the show's own project file,
//!    `<Show>.session` — the whole show as one model;
//! 2. each song is that model cropped to the song's range and moved to start
//!    at zero (`session::sync::crop`), written fresh into
//!    `<into>/<slug>/<Song>.session`;
//! 3. the show's media folders (`Samples/` …) are linked into each song's
//!    folder, so a song names the very files the show does: nothing is
//!    copied, and the songs of a show share its recordings.
//!
//! What becomes of the songs — projects, a setlist in order — is the
//! caller's (`seed.sh jt`, the studio CLI).

use std::path::{Path, PathBuf};

use session::sync::{ORIGIN_LOCAL, SessionDoc};
use session_format::store::{MAIN, Store};

/// One song made from a show.
#[derive(Debug, Clone)]
pub struct Song {
    pub name: String,
    pub slug: String,
    /// Its project file.
    pub file: PathBuf,
    /// Where it sat in the show, seconds.
    pub start: f64,
    pub end: f64,
    /// Items it holds.
    pub items: usize,
}

/// Split the Live set at `set` into songs under `into` (one folder each,
/// named by slug). Returns them in show order.
///
/// # Errors
///
/// The set would not read, convert or open, or a song would not be written.
pub fn split(set: &Path, into: &Path) -> eyre::Result<Vec<Song>> {
    let live =
        ableton::io::read_live_set(set).map_err(|e| eyre::eyre!("{}: {e}", set.display()))?;
    let ranges = ableton::setlist::songs(&live);
    if ranges.is_empty() {
        eyre::bail!(
            "{}: no songs — mark each with a locator, and its end with SONG END",
            set.display()
        );
    }
    let show_dir = set.parent().unwrap_or(Path::new(".")).to_path_buf();
    let stem = set
        .file_stem()
        .map_or_else(|| "Show".to_owned(), |s| s.to_string_lossy().into_owned());

    // 1. The whole show as one model.
    let rpp = show_dir.join(format!("{stem}.RPP"));
    studio::file::convert(set, &rpp)
        .map_err(|e| eyre::eyre!("converting {}: {e}", set.display()))?;
    let model = show_model(&rpp, &show_dir.join(format!("{stem}.session")))?;
    // A set made on another machine names its media where it was there
    // (`/Users/jt/Music/Ableton/…/Samples/…`): re-rooted under the show.
    let model = relocate(model, &show_dir);

    // 2–3. Each song, cropped, beside links to the show's media.
    std::fs::create_dir_all(into)?;
    let mut songs = Vec::with_capacity(ranges.len());
    let titles: Vec<String> = ranges.iter().map(|r| r.name.clone()).collect();
    let cards = ableton::setlist::sections(&live);
    let map = ableton::tempo::TempoMap::of(&live);
    for range in ranges {
        let mut cropped = session::sync::crop::crop(&model, range.start_seconds, range.end_seconds);
        name_song(&mut cropped, &range.name, &titles);
        let song_cards: Vec<Card> = ableton::setlist::sections_of(&cards, &range)
            .into_iter()
            .map(|c| Card {
                start: map.seconds_at(c.start_beats) - range.start_seconds,
                end: map.seconds_at(c.end_beats) - range.start_seconds,
                name: section_name(&c.name, &range.name),
                color: c.color,
            })
            .collect();
        lay_out_song(&mut cropped, &song_cards, range.seconds());
        fresh_track_guids(&mut cropped);
        let dir = into.join(&range.slug);
        ShowPlace {
            show: relative_to(&show_dir.join(format!("{stem}.session")), &dir),
            start: range.start_seconds,
            end: range.end_seconds,
        }
        .write(&mut cropped);
        std::fs::create_dir_all(&dir)?;
        bring_media(&cropped, &show_dir, &dir)?;
        let file = dir.join(format!("{}.session", file_name(&range.name)));
        write_fresh(&file, &cropped)?;
        tracing::info!(
            show.song = %range.name,
            show.items = cropped.items.len(),
            show.seconds = range.seconds(),
            "show: song made"
        );
        songs.push(Song {
            name: range.name,
            slug: range.slug,
            file,
            start: range.start_seconds,
            end: range.end_seconds,
            items: cropped.items.len(),
        });
    }
    Ok(songs)
}

/// The show at `rpp` (media unloaded), saved as the project file `session`
/// and read back as one model.
fn show_model(rpp: &Path, session: &Path) -> eyre::Result<session::sync::SessionModel> {
    // Imported fresh every time: saving into an earlier import's file would
    // keep what that import made beside what this one does.
    for stale in [
        session.to_path_buf(),
        session.with_extension("session-wal"),
        session.with_extension("session-shm"),
        session.with_extension("session-tshm"),
    ] {
        if stale.exists() {
            std::fs::remove_file(&stale)?;
        }
    }
    let daw = daw::standalone::Standalone::new();
    let prepare = crate::prepare::Prepare::for_song(rpp);
    let (opened, _) =
        crate::open::open_song_into_with(&daw, rpp, &prepare, crate::open::Media::Deferred)?;
    crate::project_file::save(
        &daw,
        &opened.project_guid,
        session,
        MAIN,
        "Imported from Live",
    )
    .map_err(|e| eyre::eyre!("saving {}: {e}", session.display()))?;
    let store = Store::open(session).map_err(|e| eyre::eyre!("{}: {e}", session.display()))?;
    let doc = store
        .load(MAIN)
        .map_err(|e| eyre::eyre!("{}: {e}", session.display()))?;
    Ok(SessionDoc::from_loro(doc).read())
}

/// The song's own region (on the SONG lane — what its tab and the setlist
/// call it) named `name`: cut from the show, it still carried the show's
/// (`Sr. Recital Showcase - Session` on every tab). And no section named
/// for a song of the show (`titles`): the show's song locators became
/// sections when it was prepared, so `Cinematic Intro` ran on over Drive
/// To LA.
fn name_song(song: &mut session::sync::SessionModel, name: &str, titles: &[String]) {
    let lane = session::ruler_lanes::CoreLane::Song.lane_index();
    song.regions.retain(|_, region| {
        region.lane == Some(lane)
            || !titles
                .iter()
                .any(|t| t.trim().eq_ignore_ascii_case(region.name.trim()))
    });
    for region in song.regions.values_mut() {
        if region.lane == Some(lane) {
            name.clone_into(&mut region.name);
        }
    }
}

/// A section card of the show, in the song's own seconds.
#[derive(Debug, Clone, PartialEq)]
struct Card {
    name: String,
    start: f64,
    end: f64,
    color: Option<u32>,
}

/// A section card's name as the song's section: a card named after the
/// song itself (a set's first card, `NO ONE KNOWS`) is its `Intro`.
fn section_name(card: &str, song: &str) -> String {
    let key = |text: &str| -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    if !key(card).is_empty() && key(card) == key(song) {
        "Intro".to_owned()
    } else {
        card.to_owned()
    }
}

/// The song's structure from its section cards (`ableton::setlist::sections`):
/// each card a region on the Sections lane, in its colour; a `Count In`
/// card the `COUNT-IN` mark, with `SONGSTART` where it ends (else at the
/// first card, else at 0); `SONGEND` and `=END` at the song's end. What the
/// show's own markers made of it (one section over the whole show, its
/// `SONGSTART` and `SONGEND`) goes: a song cut from it had no sections,
/// and its progress bars and section strip were empty.
fn lay_out_song(song: &mut session::sync::SessionModel, cards: &[Card], length: f64) {
    use session::ruler_lanes::CoreLane;
    use session::sync::model::{MarkerState, RegionState};
    let guid = || format!("{{{}}}", uuid::Uuid::new_v4().hyphenated()).to_uppercase();
    let (song_lane, sections, marks) = (
        CoreLane::Song.lane_index(),
        CoreLane::Sections.lane_index(),
        CoreLane::Marks.lane_index(),
    );
    let structural = |name: &str| {
        let upper = name.trim().to_ascii_uppercase();
        ableton::setlist::is_count_in(&upper)
            || matches!(upper.as_str(), "SONGSTART" | "SONGEND" | "=END" | "=START")
    };
    song.markers.retain(|_, m| !structural(&m.name));
    if !cards.is_empty() {
        song.regions.retain(|_, r| r.lane == Some(song_lane));
    }
    let mark = |song: &mut session::sync::SessionModel, name: &str, at: f64| {
        song.markers.insert(
            guid(),
            MarkerState {
                at,
                name: name.to_owned(),
                color: None,
                lane: Some(marks),
            },
        );
    };
    let mut start = cards
        .iter()
        .find(|c| !ableton::setlist::is_count_in(&c.name))
        .map(|c| c.start);
    for card in cards {
        if ableton::setlist::is_count_in(&card.name) {
            mark(song, "COUNT-IN", card.start.max(0.0));
            start = Some(card.end);
            continue;
        }
        song.regions.insert(
            guid(),
            RegionState {
                start: card.start.max(0.0),
                end: card.end.min(length),
                name: card.name.clone(),
                color: card.color,
                lane: Some(sections),
            },
        );
    }
    mark(song, "SONGSTART", start.unwrap_or(0.0).clamp(0.0, length));
    mark(song, "SONGEND", length);
    mark(song, "=END", length);
}

/// The track ext-state key a song's track keeps its show track's guid
/// under: what writing the song back into the show matches tracks by.
pub const SHOW_TRACK: &str = "FTS:show_track";

/// New guids for every track of `song`, and every reference to one moved
/// with it (a folder's children, sends, items): cut from one show, every
/// song had the show's 160 track guids, and two songs open at once had
/// the same tracks — anything keyed by a track across songs collided (the
/// window panicked on duplicate keys as a set's songs arrived together).
/// Each track keeps its show track's guid under [`SHOW_TRACK`].
fn fresh_track_guids(song: &mut session::sync::SessionModel) {
    let fresh: std::collections::HashMap<String, String> = song
        .tracks
        .iter()
        .map(|t| {
            let guid = format!("{{{}}}", uuid::Uuid::new_v4().hyphenated()).to_uppercase();
            (t.guid.clone(), guid)
        })
        .collect();
    let moved = |guid: &mut String| {
        if let Some(new) = fresh.get(guid.as_str()) {
            new.clone_into(guid);
        }
    };
    for track in &mut song.tracks {
        track.ext.insert(SHOW_TRACK.to_owned(), track.guid.clone());
        moved(&mut track.guid);
        if let Some(parent) = track.parent.as_mut() {
            moved(parent);
        }
        for send in &mut track.sends {
            moved(&mut send.dest);
        }
    }
    for item in song.items.values_mut() {
        moved(&mut item.track);
    }
}

/// Where a song cut from a show sits in it: the show's project file
/// (relative to the song's folder) and the song's span in show seconds.
/// Kept in the song's ext-state, so the set can be laid out on the show's
/// timeline again (show timecode) and a song's edits written back into
/// the combined session.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowPlace {
    pub show: String,
    pub start: f64,
    pub end: f64,
}

impl ShowPlace {
    /// The ext-state section it is kept under.
    const SECTION: &'static str = "FTS_SHOW";

    /// Recorded on `song`.
    pub fn write(&self, song: &mut session::sync::SessionModel) {
        let section = song
            .project
            .ext
            .entry(Self::SECTION.to_owned())
            .or_default();
        section.insert("show".to_owned(), self.show.clone());
        section.insert("start".to_owned(), self.start.to_string());
        section.insert("end".to_owned(), self.end.to_string());
    }

    /// Where `song` sits in its show, when it was cut from one.
    #[must_use]
    pub fn of(song: &session::sync::SessionModel) -> Option<Self> {
        let section = song.project.ext.get(Self::SECTION)?;
        Some(Self {
            show: section.get("show")?.clone(),
            start: section.get("start")?.parse().ok()?,
            end: section.get("end")?.parse().ok()?,
        })
    }
}

/// `path` relative to the folder `from` (`../show/Show.session`).
fn relative_to(path: &Path, from: &Path) -> String {
    let (path, from): (Vec<_>, Vec<_>) = (path.components().collect(), from.components().collect());
    let common = path.iter().zip(&from).take_while(|(a, b)| a == b).count();
    let up = std::iter::repeat_n(std::path::Component::ParentDir, from.len() - common);
    up.chain(path[common..].iter().copied())
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}

/// `model` with every take's media re-rooted under `show_dir` when it is
/// not where the set says: the shortest tail of its path that is there
/// (`Samples/Imported/Drums.wav`) — a set carried from the machine it was
/// made on keeps its media inside its own project folder.
fn relocate(
    mut model: session::sync::SessionModel,
    show_dir: &Path,
) -> session::sync::SessionModel {
    let mut moved = 0_usize;
    for item in model.items.values_mut() {
        for take in std::iter::once(&mut item.take).chain(item.other_takes.iter_mut()) {
            let Some(source) = take.source.as_deref() else {
                continue;
            };
            if let Some(rel) = rooted_under(Path::new(source), show_dir) {
                take.source = Some(rel);
                moved += 1;
            }
        }
    }
    tracing::info!(
        show.relocated = moved,
        "show: media re-rooted under the show"
    );
    model
}

/// `path` as the shortest tail that exists under `dir`, relative; `None`
/// when it already resolves where it says, or no tail of it does.
fn rooted_under(path: &Path, dir: &Path) -> Option<String> {
    if path.is_relative() && dir.join(path).exists() {
        return None;
    }
    if path.is_absolute() && path.exists() {
        return None;
    }
    let parts: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    (1..parts.len()).rev().find_map(|from| {
        let tail: PathBuf = parts[parts.len() - from..].iter().collect();
        dir.join(&tail)
            .exists()
            .then(|| tail.to_string_lossy().into_owned())
    })
}

/// The files `song` plays, put in its folder `dir` at the paths it names
/// them by (relative to the show): hard links to the show's own files, so a
/// song's folder holds real files a library serves and a device copies —
/// and costs no space, sharing the show's bytes on disk. Each file's proxy
/// (`Proxies/<stem>.ogg` and its page index) comes too when the show has
/// one. A link that cannot be made (another filesystem) is a copy.
fn bring_media(
    song: &session::sync::SessionModel,
    show_dir: &Path,
    dir: &Path,
) -> eyre::Result<()> {
    let mut files: Vec<PathBuf> = song
        .items
        .values()
        .flat_map(|i| i.takes())
        .filter_map(|t| t.source.as_deref())
        .map(PathBuf::from)
        .filter(|p| p.is_relative())
        .collect();
    files.sort();
    files.dedup();
    for rel in files {
        let from = show_dir.join(&rel);
        if !from.is_file() {
            continue;
        }
        link_file(&from, &dir.join(&rel))?;
        if let (Some(parent), Some(stem)) = (rel.parent(), rel.file_stem()) {
            let proxy = parent
                .join("Proxies")
                .join(format!("{}.ogg", stem.to_string_lossy()));
            for each in [
                proxy.clone(),
                PathBuf::from(format!("{}.idx", proxy.display())),
            ] {
                let from = show_dir.join(&each);
                if from.is_file() {
                    link_file(&from, &dir.join(&each))?;
                }
            }
        }
    }
    Ok(())
}

/// `to` as a hard link to `from` (made, or left when it already is one).
fn link_file(from: &Path, to: &Path) -> eyre::Result<()> {
    if to.symlink_metadata().is_ok() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::hard_link(from, to).is_err() {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

/// `model` written as a new project file at `file` (its song branch a fresh
/// doc of exactly this): a song starts small, with no history but its own.
fn write_fresh(file: &Path, model: &session::sync::SessionModel) -> eyre::Result<()> {
    if file.exists() {
        std::fs::remove_file(file)?;
    }
    let doc = SessionDoc::new();
    doc.write(model, ORIGIN_LOCAL)
        .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    let store = Store::open(file).map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    store
        .rebase(MAIN, doc.loro())
        .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    // One file, its log folded in: what a library serves and a device
    // copies is the song whole, not half of it in a sidecar.
    store
        .checkpoint()
        .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    Ok(())
}

/// A song name as a file name: path separators and the like taken out.
fn file_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_card_named_after_its_song_is_the_intro() {
        assert_eq!(super::section_name("NO ONE KNOWS", "No One Knows"), "Intro");
        assert_eq!(
            super::section_name("Heart Isn't Ready", "Heart isn't Ready"),
            "Intro"
        );
        assert_eq!(super::section_name("Chorus 1", "No One Knows"), "Chorus 1");
        assert_eq!(super::section_name("", ""), "");
    }

    use super::{
        Card, SHOW_TRACK, ShowPlace, file_name, fresh_track_guids, lay_out_song, name_song,
        relative_to, rooted_under,
    };

    #[test]
    fn a_songs_section_cards_become_its_sections_and_marks() {
        use session::sync::model::{MarkerState, RegionState};
        // She's American as the show left it: its SONG region, the show's
        // one section over everything, the show's SONGSTART.
        let mut song = session::sync::SessionModel::default();
        song.regions.insert(
            "song".into(),
            RegionState {
                start: 0.0,
                end: 263.0,
                name: "She's American".into(),
                color: None,
                lane: Some(0),
            },
        );
        song.regions.insert(
            "show".into(),
            RegionState {
                start: 0.0,
                end: 263.0,
                name: "Cinematic Intro".into(),
                color: None,
                lane: Some(1),
            },
        );
        song.markers.insert(
            "start".into(),
            MarkerState {
                at: 0.0,
                name: "SONGSTART".into(),
                ..MarkerState::default()
            },
        );
        let card = |name: &str, start, end, color| Card {
            name: name.into(),
            start,
            end,
            color,
        };
        let cards = [
            card("Count In", 0.0, 4.14, Some(0x2F52A2)),
            card("Intro", 4.14, 20.7, Some(0xFF3636)),
            card("Verse 1", 20.7, 300.0, None),
        ];
        lay_out_song(&mut song, &cards, 263.0);

        let mut regions: Vec<_> = song
            .regions
            .values()
            .map(|r| (r.lane, r.name.as_str(), r.start, r.end, r.color))
            .collect();
        regions.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.total_cmp(&b.2)));
        assert_eq!(
            regions,
            [
                (Some(0), "She's American", 0.0, 263.0, None),
                (Some(1), "Intro", 4.14, 20.7, Some(0xFF3636)),
                (Some(1), "Verse 1", 20.7, 263.0, None),
            ],
            "the cards on the Sections lane, the count-in not one of them, none past the end"
        );
        let mut marks: Vec<_> = song
            .markers
            .values()
            .map(|m| (m.name.as_str(), m.at, m.lane))
            .collect();
        marks.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(b.0)));
        assert_eq!(
            marks,
            [
                ("COUNT-IN", 0.0, Some(2)),
                ("SONGSTART", 4.14, Some(2)),
                ("=END", 263.0, Some(2)),
                ("SONGEND", 263.0, Some(2)),
            ],
            "bar 1 where the count-in ends; the show's SONGSTART gone"
        );
    }

    #[test]
    fn media_from_the_machine_a_set_was_made_on_is_found_under_the_show() {
        let show = tempfile::tempdir().expect("a show folder");
        std::fs::create_dir_all(show.path().join("Samples/Imported")).expect("dirs");
        std::fs::write(show.path().join("Samples/Imported/Drums.wav"), b"").expect("a file");
        let elsewhere =
            std::path::Path::new("/Users/jt/Music/Ableton/Show Project/Samples/Imported/Drums.wav");
        assert_eq!(
            rooted_under(elsewhere, show.path()).as_deref(),
            Some("Samples/Imported/Drums.wav")
        );
        assert_eq!(
            rooted_under(
                std::path::Path::new("Samples/Imported/Drums.wav"),
                show.path()
            ),
            None,
            "already where it says"
        );
        assert_eq!(
            rooted_under(std::path::Path::new("/nowhere/Gone.wav"), show.path()),
            None
        );
    }

    #[test]
    fn a_song_cut_from_a_show_is_named_for_itself_with_no_other_song_as_a_section() {
        use session::sync::{RegionState, SessionModel};
        let region = |name: &str, lane: u32| RegionState {
            start: 0.0,
            end: 196.5,
            name: name.to_owned(),
            color: None,
            lane: Some(lane),
        };
        let mut song = SessionModel::default();
        song.regions
            .insert("song".into(), region("Sr. Recital Showcase - Session", 0));
        song.regions
            .insert("ran-on".into(), region("Cinematic Intro", 1));
        song.regions.insert("verse".into(), region("Verse 1", 1));
        let titles = ["Cinematic Intro".to_owned(), "Drive To LA".to_owned()];
        name_song(&mut song, "Drive To LA", &titles);
        let mut names: Vec<_> = song
            .regions
            .values()
            .map(|r| (r.lane, r.name.as_str()))
            .collect();
        names.sort_unstable();
        assert_eq!(names, vec![(Some(0), "Drive To LA"), (Some(1), "Verse 1")]);
    }

    #[test]
    fn songs_cut_from_one_show_have_tracks_of_their_own() {
        use session::sync::{ItemState, SendState, SessionModel, TrackState};
        let track = |guid: &str, parent: Option<&str>| TrackState {
            guid: guid.to_owned(),
            parent: parent.map(str::to_owned),
            ..TrackState::default()
        };
        let mut show = SessionModel::default();
        show.tracks = vec![track("{F}", None), track("{K}", Some("{F}"))];
        show.tracks[1].sends.push(SendState {
            dest: "{F}".to_owned(),
            ..SendState::default()
        });
        show.items.insert(
            "item".to_owned(),
            ItemState {
                track: "{K}".to_owned(),
                ..ItemState::default()
            },
        );
        let (mut a, mut b) = (show.clone(), show.clone());
        fresh_track_guids(&mut a);
        fresh_track_guids(&mut b);
        assert_ne!(
            a.tracks[0].guid, b.tracks[0].guid,
            "two songs, two sets of tracks"
        );
        assert_ne!(a.tracks[0].guid, "{F}");
        assert_eq!(
            a.tracks[1].parent.as_deref(),
            Some(a.tracks[0].guid.as_str())
        );
        assert_eq!(a.tracks[1].sends[0].dest, a.tracks[0].guid);
        assert_eq!(a.items["item"].track, a.tracks[1].guid);
        assert_eq!(
            a.tracks[1].ext[SHOW_TRACK], "{K}",
            "the show's track remembered"
        );
    }

    #[test]
    fn a_song_remembers_where_it_sits_in_its_show() {
        let show = std::path::Path::new("/m/session/jt-senior-recital/Show.session");
        let dir = std::path::Path::new("/m/session/drive-to-la");
        let place = ShowPlace {
            show: relative_to(show, dir),
            start: 90.0,
            end: 286.5,
        };
        assert_eq!(place.show, "../jt-senior-recital/Show.session");
        let mut song = session::sync::SessionModel::default();
        assert_eq!(ShowPlace::of(&song), None);
        place.write(&mut song);
        assert_eq!(ShowPlace::of(&song), Some(place));
    }

    #[test]
    fn a_song_name_makes_a_safe_file_name() {
        assert_eq!(file_name("Heart isn't Ready"), "Heart isn't Ready");
        assert_eq!(file_name("AC/DC: Live?"), "AC-DC- Live-");
    }
}

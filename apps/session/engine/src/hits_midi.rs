//! The kit's hits as MIDI, both ways.
//!
//! The hits — every hit marker, where the song plays it now — are the
//! drum part. Its MIDI shows them twice over:
//!
//! - **Drum Hits**, a track just above the kit's folder: one item, every piece
//!   on its note (General MIDI's kick and snare, the toms high to low);
//! - each piece's **Sample** track: one item of that piece's notes only —
//!   what its `FTS Kit Mic` plays.
//!
//! After every edit step [`sync`] makes them agree. A note added, deleted,
//! moved or given another velocity in any of them is carried over to the
//! hits ([`crate::hits::apply_midi`]) and every view is written from the
//! hits again; an edit of the recording (a comp, a quantize, a slip)
//! moves the hits, and the views follow. Which of the two happened is told
//! by what each view was last written as: one still as written is behind
//! the hits, one changed since was edited.

use std::collections::BTreeMap;
use std::hash::{Hash as _, Hasher as _};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::TempoMap as _;
use daw::service::{ItemRef, Items as _, ProjectContext, TrackRef, Tracks};
use daw_proto::{Midi as _, MidiNoteCreate, MidiTakeLocation, TakeRef};
use facet::Facet;

use crate::actions::{meta, song};
use crate::hits::{Applied, HitChange, HitKey, NewHit, Played};

/// The track that shows the whole kit's hits.
pub const KIT_TRACK: &str = "Drum Hits";

/// The kit's note for a piece of the session's kit, by its name: General
/// MIDI's kick and snare, the toms high to low onto the kit's rack then
/// floor toms (`Tom 1` the smallest).
#[must_use]
pub fn note_for(piece: &str) -> Option<u8> {
    let name = piece.trim().to_ascii_lowercase();
    if name.contains("kick") {
        return Some(36);
    }
    if name.contains("snare") {
        return Some(38);
    }
    let tom = name.strip_prefix("tom")?.trim().parse::<u8>().ok()?;
    [45, 47, 41, 43]
        .get(usize::from(tom.checked_sub(1)?))
        .copied()
}

/// A hit's velocity from its level against its piece's loudest (dB, 0
/// the loudest): the loudest at 127, half the velocity every 12 dB down.
#[must_use]
pub fn velocity(relative_db: f64) -> u8 {
    let v = (127.0 * 10_f64.powf(relative_db.min(0.0) / 40.0)).round();
    num_traits::ToPrimitive::to_u8(&v.clamp(1.0, 127.0)).unwrap_or(1)
}

/// One hit as a note.
#[derive(Clone, Debug, PartialEq)]
pub struct HitNote {
    pub key: HitKey,
    pub piece: String,
    pub pitch: u8,
    /// Project seconds, where it plays.
    pub time: f64,
    pub velocity: u8,
}

/// A note as a view holds it: `(time, pitch, velocity)`, project seconds.
pub type Note = (f64, u8, u8);

/// Every hit that plays as a note, earliest first — each where `moved`
/// puts it, at the velocity set by hand or else the one its level gives
/// against its piece's loudest. A bleed nobody checked plays no note; a
/// ghost plays its own quiet one.
#[must_use]
pub fn hit_notes(played: &[Played], moved: &dyn Fn(f64) -> f64) -> Vec<HitNote> {
    let kept: Vec<(&Played, u8)> = played
        .iter()
        .filter(|p| !(p.mark.bleed && p.mark.origin == daw_proto::MarkOrigin::Auto))
        .filter_map(|p| Some((p, note_for(&p.piece)?)))
        .collect();
    // The scale is the measured hits': one drawn by hand has no peak, and
    // one given a velocity keeps it.
    let mut loudest: BTreeMap<&str, f64> = BTreeMap::new();
    for (p, _) in kept.iter().filter(|(p, _)| measured(p)) {
        let l = loudest.entry(p.piece.as_str()).or_insert(f64::NEG_INFINITY);
        *l = l.max(p.peak_db);
    }
    let mut notes: Vec<HitNote> = kept
        .into_iter()
        .map(|(p, pitch)| {
            let top = loudest.get(p.piece.as_str()).copied().unwrap_or(p.peak_db);
            HitNote {
                key: (crate::hits::pass_key(&p.source), p.id.clone()),
                piece: p.piece.clone(),
                pitch,
                time: moved(p.time),
                velocity: p.mark.velocity.unwrap_or_else(|| velocity(p.peak_db - top)),
            }
        })
        .collect();
    notes.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.pitch.cmp(&b.pitch)));
    notes
}

/// Whether a hit's level was measured — a detector's hit, or one placed
/// by hand where a detector read it — and it plays by it.
fn measured(p: &Played) -> bool {
    p.mark.velocity.is_none() && p.peak_db < 0.0
}

/// What a view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    /// Every piece.
    Kit,
    /// One piece: its note.
    Piece(u8),
}

impl Shows {
    #[must_use]
    pub const fn holds(self, pitch: u8) -> bool {
        match self {
            Self::Kit => true,
            Self::Piece(p) => p == pitch,
        }
    }
}

/// A track that shows hits as MIDI.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub track: String,
    pub name: String,
    pub shows: Shows,
}

/// The views among `tracks`: the [`KIT_TRACK`], and the Sample track
/// beside each of `pieces`' triggers (each a piece's name and trigger).
#[must_use]
pub fn views(tracks: &[daw_proto::Track], pieces: &[(String, String)]) -> Vec<View> {
    let kit = tracks
        .iter()
        .find(|t| !t.is_folder && t.name.trim().eq_ignore_ascii_case(KIT_TRACK))
        .map(|t| View {
            track: t.guid.clone(),
            name: t.name.clone(),
            shows: Shows::Kit,
        });
    let each = pieces.iter().filter_map(|(piece, trigger)| {
        let sample = sample_track(tracks, trigger)?;
        Some(View {
            track: sample.guid.clone(),
            name: sample.name.clone(),
            shows: Shows::Piece(note_for(piece)?),
        })
    });
    kit.into_iter().chain(each).collect()
}

/// The Sample track beside a piece's trigger: a sibling named `Sample`
/// (`Sample`, `T1 Sample`).
#[must_use]
pub fn sample_track<'a>(
    tracks: &'a [daw_proto::Track],
    trigger: &str,
) -> Option<&'a daw_proto::Track> {
    let parent = tracks
        .iter()
        .find(|t| t.guid == trigger)?
        .parent_guid
        .clone();
    tracks.iter().find(|t| {
        t.parent_guid == parent
            && !t.is_folder
            && t.name.trim().to_ascii_lowercase().ends_with("sample")
    })
}

/// How near a note must be to its hit to be that hit's, seconds: half a
/// MIDI tick at any tempo a kit is played at, with room to spare.
const ON_THE_HIT: f64 = 0.001;

/// How far a note can be dragged and still be its hit moved, seconds —
/// farther, it is a hit taken out and another put in.
const MOVE_REACH: f64 = 0.25;

/// `notes` matched to `hits` (the same pitch, within [`ON_THE_HIT`], each
/// once): for each note the hit it plays, if any.
fn matched(hits: &[Note], notes: &[Note]) -> Vec<Option<usize>> {
    let mut used = vec![false; hits.len()];
    notes
        .iter()
        .map(|n| {
            let at = hits
                .iter()
                .enumerate()
                .filter(|(i, h)| {
                    h.1 == n.1
                        && (h.0 - n.0).abs() <= ON_THE_HIT
                        && !used.get(*i).copied().unwrap_or(true)
                })
                .min_by(|a, b| (a.1.0 - n.0).abs().total_cmp(&(b.1.0 - n.0).abs()))
                .map(|(i, _)| i)?;
            if let Some(u) = used.get_mut(at) {
                *u = true;
            }
            Some(at)
        })
        .collect()
}

/// Whether `notes` are `hits`: each played once, at its velocity.
#[must_use]
pub fn plays(hits: &[Note], notes: &[Note]) -> bool {
    hits.len() == notes.len()
        && matched(hits, notes)
            .iter()
            .zip(notes)
            .all(|(m, n)| m.and_then(|i| hits.get(i)).is_some_and(|h| h.2 == n.2))
}

/// The hits with no note on them and the notes with no hit under them,
/// by time, earliest first.
#[must_use]
pub fn unmatched(hits: &[Note], notes: &[Note]) -> (Vec<f64>, Vec<f64>) {
    let m = matched(hits, notes);
    let mut gone: Vec<f64> = hits
        .iter()
        .enumerate()
        .filter(|(i, _)| !m.contains(&Some(*i)))
        .map(|(_, h)| h.0)
        .collect();
    let mut new: Vec<f64> = notes
        .iter()
        .zip(&m)
        .filter(|(_, m)| m.is_none())
        .map(|(n, _)| n.0)
        .collect();
    gone.sort_by(f64::total_cmp);
    new.sort_by(f64::total_cmp);
    (gone, new)
}

/// What an edit of a view did to the hits it shows.
#[derive(Debug, Default, PartialEq)]
pub struct MidiEdit {
    pub changes: Vec<(HitKey, HitChange)>,
    pub added: Vec<NewHit>,
}

impl MidiEdit {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.added.is_empty()
    }

    /// Another view's edit taken in too: a hit changed or drawn in both
    /// counts once.
    fn join(&mut self, other: Self) {
        for (key, change) in other.changes {
            if !self.changes.iter().any(|(k, _)| *k == key) {
                self.changes.push((key, change));
            }
        }
        for hit in other.added {
            let twice = self
                .added
                .iter()
                .any(|h| h.piece == hit.piece && (h.now - hit.now).abs() <= ON_THE_HIT);
            if !twice {
                self.added.push(hit);
            }
        }
    }
}

/// The edit that made `notes` of the hits `shown`: a note gone is its
/// hit removed (or, with one of its pitch new within [`MOVE_REACH`],
/// moved there); a note new is a hit drawn (on the piece `piece_of` its
/// pitch); a note at another velocity is its hit's velocity set.
#[must_use]
pub fn edit_of(
    shown: &[HitNote],
    notes: &[Note],
    piece_of: &dyn Fn(u8) -> Option<String>,
) -> MidiEdit {
    let hits: Vec<Note> = shown
        .iter()
        .map(|h| (h.time, h.pitch, h.velocity))
        .collect();
    let m = matched(&hits, notes);
    let mut edit = MidiEdit::default();
    for (n, hit) in notes.iter().zip(&m) {
        if let Some(h) = hit.and_then(|i| shown.get(i))
            && h.velocity != n.2
        {
            edit.changes.push((h.key.clone(), HitChange::Velocity(n.2)));
        }
    }
    let mut gone: Vec<&HitNote> = shown
        .iter()
        .enumerate()
        .filter(|(i, _)| !m.contains(&Some(*i)))
        .map(|(_, h)| h)
        .collect();
    for (n, _) in notes.iter().zip(&m).filter(|(_, m)| m.is_none()) {
        let from = gone
            .iter()
            .enumerate()
            .filter(|(_, h)| h.pitch == n.1 && (h.time - n.0).abs() <= MOVE_REACH)
            .min_by(|a, b| (a.1.time - n.0).abs().total_cmp(&(b.1.time - n.0).abs()))
            .map(|(i, _)| i);
        match from {
            Some(i) => {
                let h = gone.remove(i);
                edit.changes.push((h.key.clone(), HitChange::MoveTo(n.0)));
            }
            None => {
                if let Some(piece) = piece_of(n.1) {
                    edit.added.push(NewHit {
                        piece,
                        now: n.0,
                        velocity: n.2,
                    });
                }
            }
        }
    }
    edit.changes
        .extend(gone.into_iter().map(|h| (h.key.clone(), HitChange::Remove)));
    edit
}

/// What a view holds, to tell later whether someone edited it: its notes,
/// to the tenth of a millisecond.
#[must_use]
fn fingerprint(notes: &[Note]) -> u64 {
    let mut keyed: Vec<(i64, u8, u8)> = notes
        .iter()
        .map(|&(t, p, v)| {
            let at = num_traits::ToPrimitive::to_i64(&(t * 10_000.0).round()).unwrap_or(0);
            (at, p, v)
        })
        .collect();
    keyed.sort_unstable();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    keyed.hash(&mut h);
    h.finish()
}

/// What each view was last written as (or found agreeing): its track
/// and [`fingerprint`]. Only [`sync`] keeps it.
static WRITTEN: std::sync::Mutex<BTreeMap<String, u64>> = std::sync::Mutex::new(BTreeMap::new());

/// Held while the views and the hits are brought into step ([`sync`]).
static SYNCING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn remember(track: &str, notes: &[Note]) {
    if let Ok(mut w) = WRITTEN.lock() {
        w.insert(track.to_owned(), fingerprint(notes));
    }
}

/// Whether `notes` are other than the view was last written as: someone
/// edited it. A view never written this run was not.
fn edited(track: &str, notes: &[Note]) -> bool {
    WRITTEN
        .lock()
        .ok()
        .and_then(|w| w.get(track).copied())
        .is_some_and(|f| f != fingerprint(notes))
}

/// What a sync goes by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// An edit step: a view edited since it was written is the hits'
    /// edit, carried over to them first.
    Reconcile,
    /// An undo or a redo — every view and the hits put back together —
    /// or an edit about to be made: the views written from the hits,
    /// whatever they hold.
    Follow,
}

/// What a sync did.
#[derive(Debug, Default)]
pub struct Synced {
    /// Views written again.
    pub written: usize,
    /// What a view's edit did to the hits.
    pub applied: Option<Applied>,
}

/// Make the kit's MIDI and its hits agree (see the module docs): each
/// hit where `moved` puts it. Views not set up (holding no item) are
/// left.
///
/// # Errors
///
/// The engine refusing a write.
pub fn sync(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    mode: Mode,
    moved: &dyn Fn(f64) -> f64,
) -> Result<Synced, String> {
    // One sync at a time: the step thread's and a caller's own, run
    // together, both read one note edit as the hits' edit — and the
    // second's rewrite of the views took the first's back.
    let mut stages = crate::stages::Stages::start();
    let _one = SYNCING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    stages.mark("sync.wait_ms");
    let tracks = Tracks::all(daw, project.clone());
    let pieces = crate::drum_reference::pieces_of(&tracks);
    let held: Vec<(View, Vec<Note>)> = views(&tracks, &pieces)
        .into_iter()
        .filter(|v| {
            !daw.get_items(project.clone(), TrackRef::Guid(v.track.clone()))
                .is_empty()
        })
        .map(|v| {
            let notes = read(daw, project, &v);
            (v, notes)
        })
        .collect();
    stages.mark("sync.views_ms");
    if held.is_empty() {
        return Ok(Synced::default());
    }
    let mut hits = hit_notes(&crate::hits::playback(daw, project), moved);
    stages.mark("sync.hits_ms");
    let mut synced = Synced::default();
    if mode == Mode::Reconcile {
        let piece_of = |pitch: u8| {
            pieces
                .iter()
                .find(|(p, _)| note_for(p) == Some(pitch))
                .map(|(p, _)| p.clone())
        };
        let mut edit = MidiEdit::default();
        for (view, notes) in &held {
            let shown: Vec<HitNote> = hits
                .iter()
                .filter(|h| view.shows.holds(h.pitch))
                .cloned()
                .collect();
            if edited(&view.track, notes) {
                edit.join(edit_of(&shown, notes, &piece_of));
            }
        }
        if !edit.is_empty() {
            let applied = crate::hits::apply_midi(daw, project, &edit.changes, &edit.added);
            hits = hit_notes(&crate::hits::playback(daw, project), moved);
            synced.applied = Some(applied);
        }
    }
    for (view, notes) in &held {
        let shown: Vec<Note> = hits
            .iter()
            .filter(|h| view.shows.holds(h.pitch))
            .map(|h| (h.time, h.pitch, h.velocity))
            .collect();
        if plays(&shown, notes) {
            remember(&view.track, notes);
            continue;
        }
        write(daw, project, &view.track, &shown)?;
        remember(&view.track, &read(daw, project, view));
        synced.written = synced.written.saturating_add(1);
    }
    stages.mark("sync.write_ms");
    Ok(synced)
}

/// A view's notes, earliest first: those it shows.
fn read(daw: &daw::standalone::Standalone, project: &ProjectContext, view: &View) -> Vec<Note> {
    let mut notes: Vec<Note> = crate::drum_reference::notes(daw, project, &view.track)
        .into_iter()
        .filter(|n| view.shows.holds(n.pitch))
        .map(|n| (n.time, n.pitch, n.velocity))
        .collect();
    notes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    notes
}

/// How long each written note is, quarter notes: a drum's sample plays
/// out whatever the length.
const NOTE_QN: f64 = 0.25;

/// A project file's MIDI resolution, ticks a quarter note: a note is
/// written on one, so the saved song plays it where the live one did (off
/// a tick, a hit moved by up to half of one, ~0.3 ms at 100 bpm, on save).
const TICKS_PER_QN: f64 = 960.0;

/// `notes` as the one MIDI item on `track`: what was there before is
/// replaced (a view holds the notes this writes, nothing else).
///
/// # Errors
///
/// The engine refusing a write.
pub fn write(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    notes: &[Note],
) -> Result<(), String> {
    for item in daw.get_items(project.clone(), TrackRef::Guid(track.to_owned())) {
        daw.delete_item(project.clone(), ItemRef::Guid(item.guid))
            .map_err(|e| e.to_string())?;
    }
    // In time order, whatever order they came in: the item spans them,
    // and each note ends before the next of its pitch.
    let mut notes = notes.to_vec();
    notes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let (Some(first), Some(last)) = (notes.first(), notes.last()) else {
        return Ok(());
    };
    let start = (first.0 - 1.0).max(0.0);
    let location: MidiTakeLocation = daw
        .create_midi_item(
            project.clone(),
            TrackRef::Guid(track.to_owned()),
            start,
            last.0 + 5.0,
        )
        .ok_or("the MIDI item would not make")?;
    let tempos: Vec<(f64, f64)> = daw
        .get_tempo_points(project.clone())
        .into_iter()
        .filter_map(|p| Some((p.position.time?.as_seconds(), p.bpm)))
        .collect();
    let bpm = daw.get_default_tempo(project.clone());
    let created = placed(&notes, |time| {
        crate::drum_reference::qn_between(start, time, &tempos, bpm)
    });
    let _ = daw.add_notes_ppq(
        MidiTakeLocation::new(location.project, location.item, TakeRef::Active),
        created,
    );
    Ok(())
}

/// `notes` as MIDI notes, at `qn(time)` quarter notes into the item: each
/// on a tick ([`TICKS_PER_QN`]) and ending a tick before the next of its
/// pitch — overlapping notes of one pitch are not two notes once written
/// as ons and offs (they read back as one cut short and one of no
/// length), and a drum's sample plays out whatever the note's length.
fn placed(notes: &[Note], qn: impl Fn(f64) -> f64) -> Vec<MidiNoteCreate> {
    // Whole ticks, divided once — as a file's reader does — so a note
    // reads back as the very value it was written with.
    let ticks: Vec<Note> = notes
        .iter()
        .map(|&(time, pitch, vel)| ((qn(time) * TICKS_PER_QN).round(), pitch, vel))
        .collect();
    let longest = NOTE_QN * TICKS_PER_QN;
    ticks
        .iter()
        .enumerate()
        .map(|(i, &(start, pitch, vel))| {
            let next = ticks
                .iter()
                .skip(i.saturating_add(1))
                .find(|n| n.1 == pitch && n.0 > start)
                .map_or(f64::INFINITY, |n| n.0);
            let length = longest.min(next - start - 1.0).max(1.0);
            MidiNoteCreate::new(pitch, vel, start / TICKS_PER_QN, length / TICKS_PER_QN)
        })
        .collect()
}

/// One view against the hits it shows.
#[derive(Debug)]
pub struct Standing {
    pub view: View,
    /// The hits it shows playing in the span asked about.
    pub hits: usize,
    /// Its notes in it.
    pub notes: usize,
    /// Where a hit plays with no note on it, seconds.
    pub unplayed: Vec<f64>,
    /// Where a note plays with no hit under it, seconds.
    pub stray: Vec<f64>,
    /// Notes on their hits at another velocity.
    pub velocities: usize,
}

/// Each view set up, against the hits it shows over `span` (seconds).
#[must_use]
pub fn check(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    span: (f64, f64),
) -> Vec<Standing> {
    let tracks = Tracks::all(daw, project.clone());
    let pieces = crate::drum_reference::pieces_of(&tracks);
    let hits = hit_notes(&crate::hits::playback(daw, project), &|t| t);
    let inside = |n: &Note| n.0 >= span.0 && n.0 < span.1;
    views(&tracks, &pieces)
        .into_iter()
        .filter_map(|view| {
            let notes: Vec<Note> = read(daw, project, &view)
                .into_iter()
                .filter(inside)
                .collect();
            let shown: Vec<Note> = hits
                .iter()
                .filter(|h| view.shows.holds(h.pitch))
                .map(|h| (h.time, h.pitch, h.velocity))
                .filter(inside)
                .collect();
            if notes.is_empty()
                && daw
                    .get_items(project.clone(), TrackRef::Guid(view.track.clone()))
                    .is_empty()
            {
                return None;
            }
            let (unplayed, stray) = unmatched(&shown, &notes);
            let velocities = matched(&shown, &notes)
                .iter()
                .zip(&notes)
                .filter(|(m, n)| m.and_then(|i| shown.get(i)).is_some_and(|h| h.2 != n.2))
                .count();
            Some(Standing {
                hits: shown.len(),
                notes: notes.len(),
                unplayed,
                stray,
                velocities,
                view,
            })
        })
        .collect()
}

/// The [`KIT_TRACK`]: the one there, or a new one just above the kit's
/// folder. Its guid.
///
/// # Errors
///
/// No kit, or the engine refusing the track.
pub fn kit_track(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Result<String, String> {
    let tracks = Tracks::all(daw, project.clone());
    if let Some(t) = tracks
        .iter()
        .find(|t| !t.is_folder && t.name.trim().eq_ignore_ascii_case(KIT_TRACK))
    {
        return Ok(t.guid.clone());
    }
    let folder = crate::track_regions::kit_folder(daw, project).ok_or("no kit folder")?;
    let at = tracks
        .iter()
        .find(|t| t.guid == folder)
        .ok_or("no kit folder")?
        .index;
    Tracks::add(daw, project.clone(), KIT_TRACK, Some(at)).map_err(|e| e.to_string())
}

static SET_UP: ActionMeta = meta(
    "DRUMS_HITS_MIDI",
    "drums_hits_midi",
    "Drum hits as MIDI",
    "The kit's hits as MIDI you can edit: a `Drum Hits` track just above the kit's folder with every piece on its note (kick 36, snare 38, toms 45/47/41/43), and each piece's Sample track with its own notes only. Each is written where the hits play now, and kept so after every edit: a comp, a quantize or a slip moves them; a note added, deleted, moved or given another velocity in any of them is that hit added, removed, moved or re-velocitied (hand-placed), and the rest follow. Says each view's notes.",
    "Drums",
    "",
);

static CHECK: ActionMeta = meta(
    "DRUMS_HITS_CHECK",
    "drums_hits_check",
    "Drum hits MIDI in step",
    "Whether each view of the kit's hits as MIDI (the `Drum Hits` track, each Sample track) plays the hits where they play now: per view, the hits it shows and its notes (`from`/`to`, seconds, narrow it), `in_step`, the first few hits with no note (`unplayed`), notes with no hit (`stray`), and how many notes are at another velocity than their hit's.",
    "Drums",
    "",
);

/// Nothing to say.
#[derive(Facet)]
struct NoArg {}

/// Over when.
#[derive(Facet)]
struct CheckArg {
    from: Option<f64>,
    to: Option<f64>,
}

/// One view against its hits.
#[derive(Facet)]
struct StandingOut {
    track: String,
    shows: String,
    hits: u32,
    notes: u32,
    in_step: bool,
    unplayed: Vec<f64>,
    stray: Vec<f64>,
    velocities: u32,
}

/// One view, as set up.
#[derive(Facet)]
struct ViewOut {
    track: String,
    shows: String,
    notes: u32,
}

/// How many of the hits or notes out of step a check names.
const SHOWN: usize = 20;

fn shows_name(shows: Shows) -> String {
    match shows {
        Shows::Kit => "kit".to_owned(),
        Shows::Piece(p) => format!("note {p}"),
    }
}

/// Register the hits-as-MIDI actions.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    list.register(&SET_UP, Effect::Mutating, move |_: NoArg| {
        let (daw, project) = song()?;
        let label = "Drum hits as MIDI";
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        let made = kit_track(daw, &project).and_then(|track| {
            // An empty view is not set up: give it its first item.
            if daw
                .get_items(project.clone(), TrackRef::Guid(track.clone()))
                .is_empty()
            {
                daw.create_midi_item(project.clone(), TrackRef::Guid(track), 0.0, 1.0)
                    .ok_or("the MIDI item would not make")?;
            }
            sync(daw, &project, Mode::Follow, &|t| t)
        });
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        made?;
        crate::song_model::request_resync();
        Ok(check(daw, &project, (f64::NEG_INFINITY, f64::INFINITY))
            .into_iter()
            .map(|s| ViewOut {
                track: s.view.name,
                shows: shows_name(s.view.shows),
                notes: count(s.notes),
            })
            .collect::<Vec<_>>())
    })?;
    list.register(&CHECK, Effect::ReadOnly, move |a: CheckArg| {
        let (daw, project) = song()?;
        let span = (
            a.from.unwrap_or(f64::NEG_INFINITY),
            a.to.unwrap_or(f64::INFINITY),
        );
        Ok(check(daw, &project, span)
            .into_iter()
            .map(|s| StandingOut {
                in_step: s.unplayed.is_empty() && s.stray.is_empty() && s.velocities == 0,
                track: s.view.name,
                shows: shows_name(s.view.shows),
                hits: count(s.hits),
                notes: count(s.notes),
                unplayed: s.unplayed.into_iter().take(SHOWN).collect(),
                stray: s.stray.into_iter().take(SHOWN).collect(),
                velocities: count(s.velocities),
            })
            .collect::<Vec<_>>())
    })?;
    Ok(())
}

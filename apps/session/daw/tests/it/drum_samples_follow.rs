//! The drum samples stay on the kit's hits through every edit: a Sample
//! track's notes are where its piece's hits play now — after a quantize,
//! a hit placed by hand, an undo, a revert — never where they were when
//! the samples were written.
//!
//! On the golden session, the kick and snare marked where the drummer
//! played them, their Sample tracks given notes from those hits; then
//! each edit run as a model runs it (the window's part done here as the
//! window does it), and every piece's notes held up to its hits.
//!
//! Then the other way: the kit's MIDI edited — a note deleted, one drawn,
//! one dragged, one given another velocity — is the hits edited, and
//! every view of them follows.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use std::path::Path;
use std::time::{Duration, Instant};

use daw::service::{ProjectContext, TrackRef, Tracks};
use daw_proto::Midi as _;
use dynamic_template::golden_session::{perform, shape};
use session_daw::actions::{list, take_window, window_pending};
use session_daw::arrange_edit::{Editor, Effect, SlipCut, kit_items, seconds_of};
use session_daw::hits_midi::{Mode, Note};
use session_daw::keys::Action;
use session_daw::studio::StudioSession;

/// How near its hit a sample's note must be, seconds: the note is on a
/// MIDI tick (well under a millisecond).
const ON_ITS_HIT: f64 = 0.001;

/// Carry out what the Action List queued for the window — the slip — on
/// the session as the engine has it now.
fn be_the_window(path: &Path) {
    for action in take_window() {
        let Action::SlipDrums {
            moves,
            pre_roll_us,
            crossfade_us,
            stretch_over_us,
            attacks_us,
            ticket,
        } = action
        else {
            continue;
        };
        let session =
            StudioSession::read_current(Some(path), None).expect("the session reads back");
        let mut project = (*session.project.0).clone();
        let items = kit_items(&project, &[]);
        let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1_000_000.0;
        let cut = SlipCut {
            pre_roll: seconds(pre_roll_us),
            crossfade: seconds(crossfade_us),
            stretch_over: (stretch_over_us > 0).then(|| seconds(stretch_over_us)),
            attacks: attacks_us.iter().map(|us| seconds(*us)).collect(),
        };
        let mut effects = Vec::new();
        Editor::slip_items(
            &mut project,
            &items,
            &seconds_of(&moves),
            ("Slip edit drums", cut),
            &mut effects,
        );
        for effect in effects {
            if let Effect::Send(edit) = effect {
                assert!(session_daw::engine::apply_now(&edit), "{edit:?} applies");
            }
        }
        assert!(session_daw::engine::apply_now(
            &session_daw::engine::Edit::Done(ticket)
        ));
    }
}

/// Run `id` with `args` as a model does, being the window while it runs.
fn run_as_a_model(path: &Path, id: &'static str, args: String) -> String {
    let run = std::thread::spawn(move || list().run_json(id, args.as_bytes()));
    let began = Instant::now();
    while !run.is_finished() {
        assert!(began.elapsed() < Duration::from_secs(60), "{id} finishes");
        if window_pending() {
            be_the_window(path);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let said = run.join().expect("it ran").expect("it was not refused");
    String::from_utf8_lossy(&said).into_owned()
}

/// The Sample track beside a piece's trigger (`Sample`, `T1 Sample`).
fn sample_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    trigger: &str,
) -> Option<String> {
    let tracks = Tracks::all(daw, project.clone());
    let parent = tracks
        .iter()
        .find(|t| t.guid == trigger)
        .and_then(|t| t.parent_guid.clone());
    tracks
        .iter()
        .find(|t| t.parent_guid == parent && t.name.trim().to_ascii_lowercase().ends_with("sample"))
        .map(|t| t.guid.clone())
}

/// Every piece's sample notes held up to its hits where they play now:
/// one note a hit, each on it.
fn samples_on_the_hits(daw: &daw::standalone::Standalone, project: &ProjectContext, after: &str) {
    let played = session_daw::hits::playback(daw, project);
    for (piece, trigger) in session_daw::drum_reference::pieces(daw, project) {
        let mut hits: Vec<f64> = played
            .iter()
            .filter(|p| {
                p.piece == piece && !(p.mark.bleed && p.mark.origin == daw_proto::MarkOrigin::Auto)
            })
            .map(|p| p.time)
            .collect();
        if hits.is_empty() {
            continue;
        }
        hits.sort_by(f64::total_cmp);
        let sample = sample_of(daw, project, &trigger).expect("a Sample beside a played piece");
        let notes: Vec<f64> = session_daw::drum_reference::notes(daw, project, &sample)
            .into_iter()
            .map(|n| n.time)
            .collect();
        assert_eq!(
            notes.len(),
            hits.len(),
            "after {after}: {piece}'s samples, one a hit"
        );
        for (note, hit) in notes.iter().zip(&hits) {
            assert!(
                (note - hit).abs() < ON_ITS_HIT,
                "after {after}: {piece}'s sample at {note:.4}, its hit at {hit:.4}"
            );
        }
    }
}

#[test]
fn the_samples_stay_on_the_hits_through_every_edit() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../features/dynamic-template/fixtures/golden/template.rpp");
    dynamic_template::golden_session::write_media(path.parent().expect("the fixtures"))
        .expect("the golden media writes");
    session_daw::open::open_silent(&path).expect("the golden session opens");

    let song = shape::maximal();
    let bar = 240.0 / song.bpm;
    let section = song
        .sections
        .iter()
        .find(|s| s.start >= 8 && s.end > s.start.saturating_add(3))
        .expect("a section past the intro");
    let (from, to) = (f64::from(section.start) * bar, f64::from(section.end) * bar);
    for (on, piece, _) in perform::groove(&song) {
        let name = match piece {
            perform::Piece::Kick => "Kick",
            perform::Piece::Snare => "Snare",
            _ => continue,
        };
        if on < from - 2.0 * bar || on > to + 2.0 * bar {
            continue;
        }
        let played = perform::kit_played(on, piece, song.bpm);
        let args = format!(r#"{{"piece":"{name}","time":{played},"snap":false}}"#);
        list()
            .run_json("HITS_ADD", args.as_bytes())
            .expect("a hit placed");
    }

    // The samples written where the hits play.
    let (daw, project) = session_daw::actions::song().expect("the song is open");
    for (_, trigger) in session_daw::drum_reference::pieces(daw, &project) {
        let Some(sample) = sample_of(daw, &project, &trigger) else {
            continue;
        };
        daw.create_midi_item(project.clone(), TrackRef::Guid(sample), 0.0, 1.0)
            .expect("a MIDI item on the Sample track");
    }
    session_daw::hits_midi::sync(daw, &project, Mode::Follow, &|t| t).expect("the samples written");
    samples_on_the_hits(daw, &project, "writing them");

    let said = run_as_a_model(
        &path,
        "QUANTIZE",
        format!(r#"{{"from":{from},"to":{to},"grid":16,"target":"Grid","brief":true}}"#),
    );
    assert!(said.contains(r#""applied":true"#), "{said}");
    samples_on_the_hits(daw, &project, "a quantize");

    let mid = session_daw::hits::playback(daw, &project)
        .into_iter()
        .filter(|p| p.time > from && p.time < to)
        .nth(4)
        .expect("a hit in the section");
    run_as_a_model(
        &path,
        "HITS_PLACE",
        format!(r#"{{"id":"{}","by_ms":20,"snap":false}}"#, mid.id),
    );
    samples_on_the_hits(daw, &project, "a hit placed by hand");

    list().run_json("SESSION_UNDO", b"{}").expect("an undo");
    samples_on_the_hits(daw, &project, "an undo");

    run_as_a_model(
        &path,
        "DRUMS_REVERT",
        format!(r#"{{"from":{from},"to":{to}}}"#),
    );
    samples_on_the_hits(daw, &project, "a revert");

    the_midi_edits_the_hits(daw, &project, (from, to));
}

/// Every view of the hits (the `Drum Hits` track, each Sample track) in step.
fn views_in_step(after: &str) {
    let said = list()
        .run_json("DRUMS_HITS_CHECK", b"{}")
        .expect("the check runs");
    let said = String::from_utf8_lossy(&said);
    assert!(
        said.contains("\"track\":\"Drum Hits\""),
        "the kit's view is there: {said}"
    );
    assert!(
        !said.contains("\"in_step\":false"),
        "after {after}: every view plays the hits: {said}"
    );
}

/// The notes of `piece` playing now, `(time, velocity)`.
fn hits_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    piece: &str,
) -> Vec<(f64, u8)> {
    session_daw::hits_midi::hit_notes(&session_daw::hits::playback(daw, project), &|t| t)
        .into_iter()
        .filter(|h| h.piece == piece)
        .map(|h| (h.time, h.velocity))
        .collect()
}

/// A piece's Sample track's notes, edited by `edit` as a person would in
/// the editor, then the step's sync.
fn edit_notes(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    piece: &str,
    edit: impl FnOnce(&mut Vec<Note>),
) -> String {
    let (_, trigger) = session_daw::drum_reference::pieces(daw, project)
        .into_iter()
        .find(|(p, _)| p == piece)
        .expect("the piece");
    let sample = sample_of(daw, project, &trigger).expect("its Sample track");
    let mut notes: Vec<Note> = session_daw::drum_reference::notes(daw, project, &sample)
        .into_iter()
        .map(|n| (n.time, n.pitch, n.velocity))
        .collect();
    let before = notes.clone();
    edit(&mut notes);
    assert_ne!(before, notes, "{piece}: the edit changed a note");
    session_daw::hits_midi::write(daw, project, &sample, &notes).expect("the notes written");
    let synced = session_daw::hits_midi::sync(daw, project, Mode::Reconcile, &|t| t)
        .expect("the step's sync");
    assert!(
        synced.applied.is_some(),
        "the edit was read as the hits' edit"
    );
    format!("{:?}, {} views written", synced.applied, synced.written)
}

fn the_midi_edits_the_hits(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
) {
    list()
        .run_json("DRUMS_HITS_MIDI", b"{}")
        .expect("the hits as MIDI");
    views_in_step("setting up the hits as MIDI");
    let inside = |t: f64| t > from && t < to;

    // A snare note deleted: its hit is gone.
    let snares = hits_of(daw, project, "Snare");
    let gone = snares
        .iter()
        .map(|h| h.0)
        .find(|t| inside(*t))
        .expect("a snare");
    let _ = edit_notes(daw, project, "Snare", |notes| {
        notes.retain(|n| (n.0 - gone).abs() > ON_ITS_HIT);
    });
    let now = hits_of(daw, project, "Snare");
    assert_eq!(now.len(), snares.len() - 1, "one snare fewer");
    assert!(now.iter().all(|h| (h.0 - gone).abs() > ON_ITS_HIT));
    views_in_step("a snare note deleted");

    // A kick note drawn between two kicks: a kick hit is there.
    let kicks = hits_of(daw, project, "Kick");
    let pair = kicks
        .windows(2)
        .find(|w| inside(w[0].0) && w[1].0 - w[0].0 > 0.3)
        .expect("two kicks apart");
    let drawn = (pair[0].0 + pair[1].0) / 2.0;
    let did = edit_notes(daw, project, "Kick", |notes| notes.push((drawn, 36, 77)));
    let now = hits_of(daw, project, "Kick");
    assert_eq!(now.len(), kicks.len() + 1, "one kick more: {did}");
    let new = now
        .iter()
        .find(|h| (h.0 - drawn).abs() < ON_ITS_HIT)
        .expect("the kick drawn plays");
    assert_eq!(new.1, 77, "at the velocity drawn");
    views_in_step("a kick note drawn");

    // A kick's velocity changed: the hit keeps it.
    let soft = now
        .iter()
        .map(|h| h.0)
        .find(|t| inside(*t) && (t - drawn).abs() > 0.1)
        .expect("a kick");
    let _ = edit_notes(daw, project, "Kick", |notes| {
        if let Some(n) = notes.iter_mut().find(|n| (n.0 - soft).abs() < ON_ITS_HIT) {
            n.2 = 21;
        }
    });
    assert!(
        hits_of(daw, project, "Kick")
            .iter()
            .any(|h| (h.0 - soft).abs() < ON_ITS_HIT && h.1 == 21),
        "the kick plays at 21"
    );
    views_in_step("a kick's velocity changed");

    // A snare note dragged 30 ms later: its hit moved, no other.
    let snares = hits_of(daw, project, "Snare");
    let dragged = snares
        .iter()
        .map(|h| h.0)
        .find(|t| inside(*t))
        .expect("a snare");
    let _ = edit_notes(daw, project, "Snare", |notes| {
        if let Some(n) = notes
            .iter_mut()
            .find(|n| (n.0 - dragged).abs() < ON_ITS_HIT)
        {
            n.0 += 0.030;
        }
    });
    let now = hits_of(daw, project, "Snare");
    assert_eq!(now.len(), snares.len(), "as many snares");
    assert!(
        now.iter().any(|h| (h.0 - (dragged + 0.030)).abs() < 0.002),
        "the snare plays 30 ms later"
    );
    views_in_step("a snare note dragged");
}

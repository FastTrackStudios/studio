//! Takes ranked up and down by hand, to sort the good from the bad: REAPER's
//! take ranking (7.17) — a rank marker on the take (`:)` … `:)))` up,
//! `:(` down) rating it from where it stands until the next.
//!
//! A rank is written on the recording, so every slice of it carries it,
//! and — on a kit — on every mic's recording of the pass, so the kit's
//! take is ranked as one. A take ranked as a whole is marked at its
//! start; one ranked while it records is marked where the press fell.
//! The comp ranking reads it: an up-ranked take first among its kind, a
//! down-ranked one last and skipped by T.

use std::sync::Mutex;

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ItemRef, Items as _, ProjectContext, TakeRef, Takes as _, Tracks};
use daw_proto::transport::service::Transport as TransportService;
use daw_proto::{TakeMarker, TakeRating};
use facet::Facet;

/// The most a take is up-ranked (REAPER's default).
pub const MAX_UP: i8 = 3;
/// Two rank markers this near are one, seconds.
const SAME_PLACE: f64 = 0.001;

/// A rank marker's level: 1..=[`MAX_UP`] up, -1 down; `None` for any
/// other marker.
#[must_use]
pub fn level_of(marker: &TakeMarker) -> Option<i8> {
    if marker.hit.is_some() {
        return None;
    }
    match TakeRating::from_marker_name(&marker.name)? {
        TakeRating::UpRank(n) => Some(i8::try_from(n).unwrap_or(MAX_UP).min(MAX_UP)),
        TakeRating::DownRank => Some(-1),
    }
}

/// Where a rank or a note holds, seconds into the recording: from a
/// point on (`to` none, REAPER's take marker), or a take region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub from: f64,
    pub to: Option<f64>,
}

impl Place {
    /// Whether `m` is at this place: the same start, and the same end or
    /// none alike.
    fn holds(&self, m: &TakeMarker) -> bool {
        (m.source_position_seconds - self.from).abs() <= SAME_PLACE
            && match (self.to, m.end_seconds) {
                (None, None) => true,
                (Some(a), Some(b)) => (a - b).abs() <= SAME_PLACE,
                _ => false,
            }
    }
}

/// The rank at `source` seconds into the recording: a take region's that
/// holds it (the latest to start), else the last point rank at or before
/// it; 0 with none.
#[must_use]
pub fn level_at(markers: &[TakeMarker], source: f64) -> i8 {
    let latest = |regions: bool| {
        markers
            .iter()
            .filter(|m| m.end_seconds.is_some() == regions)
            .filter(|m| m.source_position_seconds <= source + SAME_PLACE)
            .filter(|m| m.end_seconds.is_none_or(|end| source < end))
            .filter_map(|m| level_of(m).map(|l| (m.source_position_seconds, l)))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, l)| l)
    };
    latest(true).or_else(|| latest(false)).unwrap_or(0)
}

fn rank_name(level: i8) -> Option<String> {
    match level {
        l if l > 0 => Some(TakeRating::UpRank(u8::try_from(l).unwrap_or(1)).to_marker_name()),
        l if l < 0 => Some(TakeRating::DownRank.to_marker_name()),
        _ => None,
    }
}

/// `markers` with the rank at `place` moved `by` levels (up positive),
/// and the level it is now there: the rank marker at the place replaced
/// (its note kept), or one added — none where the level comes out as
/// what rates it already. A region at 0 is taken away: what is under it
/// rates it again.
#[must_use]
pub fn ranked(mut markers: Vec<TakeMarker>, place: Place, by: i8) -> (Vec<TakeMarker>, i8) {
    let probe = place
        .to
        .map_or(place.from, |to| f64::midpoint(place.from, to));
    let was = markers
        .iter()
        .find(|m| place.holds(m) && level_of(m).is_some())
        .cloned();
    let level = (was
        .as_ref()
        .and_then(level_of)
        .unwrap_or_else(|| level_at(&markers, probe))
        + by)
        .clamp(-1, MAX_UP);
    markers.retain(|m| !(place.holds(m) && level_of(m).is_some()));
    let under = level_at(&markers, probe);
    // A point that says what rates it already says nothing; a region
    // does (it holds against what may come after its start).
    let says = place.to.is_some() || level != under;
    let Some(name) = rank_name(level).filter(|_| says) else {
        return (markers, under);
    };
    markers.push(TakeMarker {
        name,
        source_position_seconds: place.from,
        end_seconds: place.to,
        note: was.map(|m| m.note).unwrap_or_default(),
        ..TakeMarker::default()
    });
    markers.sort_by(|a, b| {
        a.source_position_seconds
            .total_cmp(&b.source_position_seconds)
    });
    (markers, level)
}

/// `markers` with `note` said at `place`: on the marker there (a rank's
/// or a note's), else on a new one; an empty note takes a note-only
/// marker away.
#[must_use]
pub fn noted(mut markers: Vec<TakeMarker>, place: Place, note: &str) -> Vec<TakeMarker> {
    let here = markers
        .iter()
        .position(|m| m.hit.is_none() && place.holds(m));
    match here {
        Some(i) if note.is_empty() && markers.get(i).and_then(level_of).is_none() => {
            markers.remove(i);
        }
        Some(i) => {
            if let Some(m) = markers.get_mut(i) {
                m.note = note.to_owned();
                if level_of(m).is_none() {
                    // REAPER shows a marker's name: a note-only marker's is
                    // its note.
                    m.name = note.to_owned();
                }
            }
        }
        None if note.is_empty() => {}
        None => {
            markers.push(TakeMarker {
                name: note.to_owned(),
                note: note.to_owned(),
                source_position_seconds: place.from,
                end_seconds: place.to,
                ..TakeMarker::default()
            });
            markers.sort_by(|a, b| {
                a.source_position_seconds
                    .total_cmp(&b.source_position_seconds)
            });
        }
    }
    markers
}

/// A rank or note on a take, where the song plays it.
pub fn shown(markers: &[TakeMarker], origin: f64) -> Vec<crate::hit_overlay::TakeMark> {
    markers
        .iter()
        .filter(|m| m.hit.is_none())
        .filter_map(|m| {
            let level = level_of(m).unwrap_or(0);
            (level != 0 || !m.note.is_empty()).then(|| crate::hit_overlay::TakeMark {
                from: origin + m.source_position_seconds,
                to: m.end_seconds.map(|e| origin + e),
                level,
                note: m.note.clone(),
            })
        })
        .collect()
}

/// Every take's ranks and notes where the song plays them: by item (each
/// slice of a recording carrying any) and by the kit's take (its pass).
#[must_use]
pub fn take_marks(p: &daw::standalone::sync::ProjectState) -> crate::hit_overlay::TakeMarks {
    let marked: std::collections::HashMap<&str, &Vec<TakeMarker>> = p
        .take_markers
        .iter()
        .filter(|(_, ms)| {
            ms.iter()
                .any(|m| m.hit.is_none() && (level_of(m).is_some() || !m.note.is_empty()))
        })
        .map(|(path, ms)| (path.as_str(), ms))
        .collect();
    let mut out = crate::hit_overlay::TakeMarks::default();
    if marked.is_empty() {
        return out;
    }
    for (guid, entry) in &p.items {
        let Some(take) = p
            .takes
            .get(guid)
            .and_then(|l| l.takes.get(usize::try_from(l.active_idx).ok()?))
        else {
            continue;
        };
        let Some((path, markers)) = take
            .source_file_path
            .as_deref()
            .and_then(|path| marked.get(path).map(|m| (path, *m)))
        else {
            continue;
        };
        let origin = entry.item.position.as_seconds() - take.start_offset.as_seconds();
        let marks = shown(markers, origin);
        out.by_pass
            .entry(crate::hits::pass_key(path))
            .or_insert_with(|| marks.clone());
        out.by_item.insert(guid.clone(), marks);
    }
    out
}

/// What to rank: a take by an item playing it, at a moment of the song
/// (`None`: the take as a whole, from its start) — or over a stretch of
/// it, a take region.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub item: String,
    pub at: Option<f64>,
    /// With `at`, where a take region ends (song seconds).
    pub to: Option<f64>,
}

/// What a rank did.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Ranked {
    /// The take's recording pass.
    pub pass: String,
    /// Its level now there: 1..=3 up, -1 down, 0 neither.
    pub level: i32,
    /// Seconds into its recording the rank is from.
    pub from: f64,
    /// Where it ends, a take region's.
    pub to: Option<f64>,
    /// How many recordings carry it (a kit's mics).
    pub recordings: u32,
}

/// Each recording of the take `item` plays — on a kit, every mic's of
/// the pass — as an item playing it.
fn recordings_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    item: &daw_proto::Item,
    path: &str,
) -> Vec<(String, String)> {
    let pass = crate::hits::pass_key(path);
    let tracks = Tracks::all(daw, project.clone());
    let kit: Vec<&daw_proto::Track> = crate::track_regions::kit_folder(daw, project)
        .map(|folder| daw_ui::studio::folded::under(&tracks, &folder))
        .unwrap_or_default();
    if !kit.iter().any(|t| t.guid == item.track_guid) {
        return vec![(item.guid.clone(), path.to_owned())];
    }
    kit.iter()
        .filter_map(|t| {
            daw.get_items(
                project.clone(),
                daw::service::TrackRef::Guid(t.guid.clone()),
            )
            .into_iter()
            .find_map(|i| {
                let file = daw
                    .get_active_take(project.clone(), ItemRef::Guid(i.guid.clone()))?
                    .source_file_path?;
                (crate::hits::pass_key(&file) == pass).then_some((i.guid, file))
            })
        })
        .collect()
}

/// Rank `target`'s take `by` levels, on every recording of it.
///
/// # Errors
///
/// No such item, or one playing no recording.
pub fn rank(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    target: &Target,
    by: i8,
) -> Result<Ranked, String> {
    on_recordings(daw, project, target, &|m, place| ranked(m, place, by))
}

/// Say `note` on `target`'s take, on every recording of it.
///
/// # Errors
///
/// No such item, or one playing no recording.
pub fn note(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    target: &Target,
    note: &str,
) -> Result<Ranked, String> {
    on_recordings(daw, project, target, &|m, place| {
        let m = noted(m, place, note);
        let level = level_at(&m, place.from);
        (m, level)
    })
}

/// `edit` made on the markers of every recording of `target`'s take, at
/// its place.
fn on_recordings(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    target: &Target,
    edit: &dyn Fn(Vec<TakeMarker>, Place) -> (Vec<TakeMarker>, i8),
) -> Result<Ranked, String> {
    let at = ItemRef::Guid(target.item.clone());
    let item = daw
        .get_item(project.clone(), at.clone())
        .ok_or_else(|| format!("no item {}", target.item))?;
    let path = daw
        .get_active_take(project.clone(), at)
        .and_then(|t| t.source_file_path)
        .ok_or("the item plays no recording")?;
    let source_at =
        |t: f64| (item.start_offset.as_seconds() + (t - item.position.as_seconds())).max(0.0);
    let place = Place {
        from: target.at.map_or(0.0, source_at),
        to: target.at.and(target.to).map(source_at),
    };
    let recordings = recordings_of(daw, project, &item, &path);
    let mut level = 0;
    for (guid, _) in &recordings {
        let at = ItemRef::Guid(guid.clone());
        let markers = daw.get_take_markers(project.clone(), at.clone(), TakeRef::Active);
        let (markers, now) = edit(markers, place);
        level = now;
        daw.set_take_markers(project.clone(), at, TakeRef::Active, markers)
            .map_err(|e| e.to_string())?;
    }
    Ok(Ranked {
        pass: crate::hits::pass_key(&path),
        level: i32::from(level),
        from: place.from,
        to: place.to,
        recordings: u32::try_from(recordings.len()).unwrap_or(u32::MAX),
    })
}

/// The rank of a take — its recording at `path`, which starts `origin`
/// song seconds in — at `t`, song seconds.
#[must_use]
pub fn level_of_take(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (track, path): (&str, &str),
    t: f64,
) -> i8 {
    daw.get_items(
        project.clone(),
        daw::service::TrackRef::Guid(track.to_owned()),
    )
    .into_iter()
    .find_map(|i| {
        let take = daw.get_active_take(project.clone(), ItemRef::Guid(i.guid.clone()))?;
        (take.source_file_path.as_deref() == Some(path)).then(|| {
            let origin = i.position.as_seconds() - i.start_offset.as_seconds();
            let markers =
                daw.get_take_markers(project.clone(), ItemRef::Guid(i.guid), TakeRef::Active);
            level_at(&markers, t - origin)
        })
    })
    .unwrap_or(0)
}

/// [`settle`] on the song this window plays, if it plays one.
pub fn settle_open() {
    if let (Some(daw), Some(song)) = (crate::open::local_engine(), crate::open::current_song()) {
        settle(daw, &ProjectContext::Project(song));
    }
}

/// Ranks pressed while a pass records, `(song seconds, by)`: written on
/// its takes once it ends ([`settle`]).
static PENDING: Mutex<Vec<(f64, i8)>> = Mutex::new(Vec::new());
/// The first item of the last pass seen ended: a new one is selected.
static LAST_PASS: Mutex<Option<String>> = Mutex::new(None);

/// A rank pressed while recording, at `t`.
pub fn press_while_recording(t: f64, by: i8) {
    if let Ok(mut pending) = PENDING.lock() {
        pending.push((t, by));
    }
}

/// Once a recording pass has ended: its takes selected (what the arrows
/// rank next), and the ranks pressed while it recorded written on them.
/// Cheap when nothing changed; called from the window each frame.
pub fn settle(daw: &daw::standalone::Standalone, project: &ProjectContext) {
    let ProjectContext::Project(guid) = project else {
        return;
    };
    if TransportService::is_recording(daw, project.clone()) {
        return;
    }
    let pass = daw.last_recording(guid);
    let Some(first) = pass.first() else {
        return;
    };
    let Ok(mut last) = LAST_PASS.lock() else {
        return;
    };
    if last.as_deref() == Some(first.item_guid.as_str()) {
        return;
    }
    *last = Some(first.item_guid.clone());
    drop(last);
    let pending: Vec<(f64, i8)> = PENDING
        .lock()
        .map(|mut p| p.drain(..).collect())
        .unwrap_or_default();
    let _ = daw::service::Items::select_all_items(daw, project.clone(), false);
    for take in &pass {
        let _ = daw::service::Items::set_selected(
            daw,
            project.clone(),
            ItemRef::Guid(take.item_guid.clone()),
            true,
        );
    }
    let label = "Rank the take while it recorded";
    if !pending.is_empty() {
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        for (t, by) in pending {
            let target = Target {
                item: first.item_guid.clone(),
                at: Some(t),
                to: None,
            };
            if let Err(e) = rank(daw, project, &target, by) {
                tracing::warn!(rank.error = %e, "a rank pressed while recording could not be written");
            }
        }
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    }
    crate::comp_rank::forget();
    crate::studio::request_resync_items();
}

static NOTE: ActionMeta = crate::actions::meta(
    "TAKE_NOTE",
    "take_note",
    "Note on a take",
    "Say something on a take: a note on its take marker at `at` (song seconds; none: its start) — or over `at`..`to`, its take region — on its rank marker there if it has one, else a marker of its own; on a kit, on every mic's recording of the pass. An empty `note` takes a note-only marker away. Takes as TAKES_RANK (`items`, or `take` near `near`). One undo step.",
    "Comp",
    "",
);

static RANK: ActionMeta = crate::actions::meta(
    "TAKES_RANK",
    "takes_rank",
    "Rank takes up or down",
    "Up-rank (`by` 1) or down-rank (-1) takes, REAPER's take ranking: a rank marker (`:)` … `:)))` — one to three stars — or `:(`, a red X) on each take's recording — on a kit, every mic's recording of the pass — rating it from `at` (song seconds) on, else as a whole; with `to` too, only `at`..`to`: a take region. Takes by `items` (GUIDs, from TRACK_ITEMS), else by `take` (pass, colour or lane name, as COMP_CANDIDATES names them) on the kit, found where it plays `near` (song seconds) if given. While the transport records, the press is kept and written on the pass's takes when it ends. One undo step. COMP_CANDIDATES then ranks an up-ranked take first among its kind and a down-ranked one last; T skips it. Says each take's level.",
    "Comp",
    "",
);

#[derive(Facet)]
struct RankArg {
    #[facet(default)]
    items: Vec<String>,
    take: Option<String>,
    /// Where `take` plays, to find it by (song seconds); else `at`.
    near: Option<f64>,
    at: Option<f64>,
    /// With `at`: rank only up to here (song seconds), a take region.
    to: Option<f64>,
    by: i32,
}

#[derive(Facet)]
struct NoteArg {
    #[facet(default)]
    items: Vec<String>,
    take: Option<String>,
    near: Option<f64>,
    at: Option<f64>,
    to: Option<f64>,
    note: String,
}

pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&RANK, Effect::Mutating, |a: RankArg| {
        let (daw, project) = crate::actions::song()?;
        let up = i32::from(MAX_UP);
        let by = i8::try_from(a.by.clamp(-up, up)).unwrap_or(0);
        if TransportService::is_recording(daw, project.clone()) {
            press_while_recording(
                a.at.unwrap_or_else(|| TransportService::get_position(daw, project.clone())),
                by,
            );
            return Ok(Vec::new());
        }
        let items = targets_of(daw, &project, &a.items, a.take.as_deref(), a.near.or(a.at))?;
        let label = if by > 0 {
            "Up-rank the take"
        } else {
            "Down-rank the take"
        };
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        let done = items
            .iter()
            .map(|item| {
                rank(
                    daw,
                    &project,
                    &Target {
                        item: item.clone(),
                        at: a.at,
                        to: a.to,
                    },
                    by,
                )
            })
            .collect::<Result<Vec<Ranked>, String>>();
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        changed();
        done
    })?;
    list.register(&NOTE, Effect::Mutating, |a: NoteArg| {
        let (daw, project) = crate::actions::song()?;
        let items = targets_of(daw, &project, &a.items, a.take.as_deref(), a.near.or(a.at))?;
        let label = "Note on the take";
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        let done = items
            .iter()
            .map(|item| {
                note(
                    daw,
                    &project,
                    &Target {
                        item: item.clone(),
                        at: a.at,
                        to: a.to,
                    },
                    &a.note,
                )
            })
            .collect::<Result<Vec<Ranked>, String>>();
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        changed();
        done
    })?;
    Ok(())
}

/// The ranks changed: the ranking forgotten, the window told.
fn changed() {
    crate::comp_rank::forget();
    crate::studio::request_resync_items();
    crate::hit_overlay::refresh();
}

/// The items to rank or note by: `items`, else an item playing the kit's
/// take `take` (near `near`).
fn targets_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    items: &[String],
    take: Option<&str>,
    near: Option<f64>,
) -> Result<Vec<String>, String> {
    if !items.is_empty() {
        return Ok(one_per_take(daw, project, items));
    }
    let name = take.ok_or("say which takes: `items` or `take`")?;
    Ok(vec![item_of_take(daw, project, name, near)?])
}

/// `items`, one for each take they play: a take selected on every mic
/// is ranked once, not once a mic. An item playing no recording is kept,
/// to be refused for it.
fn one_per_take(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    items: &[String],
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .filter(|guid| {
            daw.get_active_take(project.clone(), ItemRef::Guid((*guid).clone()))
                .and_then(|t| t.source_file_path)
                .is_none_or(|path| seen.insert(crate::hits::pass_key(&path)))
        })
        .cloned()
        .collect()
}

/// An item playing the kit's take `name` (around `at`, else anywhere).
fn item_of_take(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    name: &str,
    at: Option<f64>,
) -> Result<String, String> {
    let span = at.map_or((f64::NEG_INFINITY, f64::INFINITY), |t| (t - 0.01, t + 0.01));
    let versions = crate::comp_rank::versions_over(daw, project, span);
    let pass = crate::comp_rank::pass_named(project, &versions, name)
        .ok_or_else(|| format!("no take called {name}"))?;
    let version = versions
        .iter()
        .find(|v| v.pass == pass)
        .ok_or("the take went")?;
    let (track, path) = version.files.first().ok_or("the take has no recordings")?;
    daw.get_items(project.clone(), daw::service::TrackRef::Guid(track.clone()))
        .into_iter()
        .find(|i| {
            daw.get_active_take(project.clone(), ItemRef::Guid(i.guid.clone()))
                .and_then(|t| t.source_file_path)
                .is_some_and(|p| p == *path)
        })
        .map(|i| i.guid)
        .ok_or_else(|| format!("no item plays {name}"))
}

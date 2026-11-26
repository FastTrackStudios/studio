//! The kit and its hits as the song plays them: what more than one domain
//! reads (the edit domain's hits, the listen domain's keyed renders, the
//! mix's CLAP topics). Moved down from engine-edit's `hits` and
//! `hits_midi`, which re-export it from their old paths.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use daw::service::{Items as _, Takes as _, Tracks};
use daw_proto::{HitMark, ItemRef, ProjectContext, TakeMarker, TakeRef, TrackRef};

/// A piece of the kit and its tracks.
#[derive(Clone, Debug)]
pub struct KitPiece {
    pub name: String,
    pub trigger: String,
    /// Its close mics, the one detection reads first (`In`, `Top`).
    pub mics: Vec<String>,
}

impl KitPiece {
    /// Every track whose recordings carry the piece's hits.
    pub fn tracks(&self) -> impl Iterator<Item = &String> {
        self.mics.iter().chain(std::iter::once(&self.trigger))
    }
}

/// The kit: each piece with a trigger, and the close mics that belong to
/// no piece (hi-hat, ride) — bleed to refuse, nothing to detect.
#[must_use]
pub fn kit(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> (Vec<KitPiece>, Vec<String>) {
    let tracks = Tracks::all(daw, project.clone());
    let pieces = crate::drum_reference::pieces(daw, project);
    // Which tracks hold items: one look, not every track's items copied.
    let filled: Option<std::collections::HashSet<String>> = match project {
        ProjectContext::Project(guid) => daw.read_project(guid, |p| {
            p.items_by_track
                .iter()
                .filter(|(_, items)| !items.is_empty())
                .map(|(track, _)| track.clone())
                .collect()
        }),
        ProjectContext::Current => None,
    };
    let has_items = |guid: &str| {
        filled.as_ref().map_or_else(
            || {
                !daw.get_items(project.clone(), TrackRef::Guid(guid.to_owned()))
                    .is_empty()
            },
            |filled| filled.contains(guid),
        )
    };
    kit_from(&tracks, pieces, &has_items)
}

/// The kit of `tracks`: each of `pieces` (name, trigger track) with its
/// close mics that hold items (`has_items`), and the hats and rides in the
/// Drums folder no piece claims.
#[must_use]
pub fn kit_from(
    tracks: &[daw_proto::Track],
    pieces: Vec<(String, String)>,
    has_items: &dyn Fn(&str) -> bool,
) -> (Vec<KitPiece>, Vec<String>) {
    let pieces: Vec<KitPiece> = pieces
        .into_iter()
        .filter_map(|(name, trigger)| {
            let parent = tracks
                .iter()
                .find(|t| t.guid == trigger)?
                .parent_guid
                .clone();
            let mut mics: Vec<&daw_proto::Track> = tracks
                .iter()
                .filter(|t| {
                    let lower = t.name.trim().to_ascii_lowercase();
                    t.parent_guid == parent
                        && t.guid != trigger
                        && !t.is_folder
                        && !dynamic_template::apply::is_trigger(&t.name)
                        && !lower.ends_with("sample")
                        && !dynamic_template::apply::is_one_note(&t.name)
                        && has_items(&t.guid)
                })
                .collect();
            // The mic nearest the head first: what detection reads.
            mics.sort_by_key(|t| {
                let lower = t.name.trim().to_ascii_lowercase();
                !(lower.starts_with("in")
                    || lower.starts_with("top")
                    || lower == name.to_ascii_lowercase())
            });
            (!mics.is_empty()).then(|| KitPiece {
                name,
                trigger,
                mics: mics.into_iter().map(|t| t.guid.clone()).collect(),
            })
        })
        .collect();
    let taken: BTreeSet<&String> = pieces.iter().flat_map(KitPiece::tracks).collect();
    let by_guid: HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let extras = tracks
        .iter()
        .filter(|t| {
            let lower = t.name.to_ascii_lowercase();
            !t.is_folder
                && !taken.contains(&t.guid)
                && (lower.contains("hat") || lower.contains("ride"))
                && crate::gain_stage::path_of(t, &by_guid)
                    .first()
                    .is_some_and(|p| p.trim().eq_ignore_ascii_case("drums"))
                && has_items(&t.guid)
        })
        .map(|t| t.guid.clone())
        .collect();
    (pieces, extras)
}

/// The pass a hit's recording is of — what, with its id, tells it from
/// the same id on another take recorded from the same place.
#[must_use]
pub fn pass_key(path: &str) -> String {
    crate::take_colors::stamp(path).unwrap_or_else(|| path.to_owned())
}

/// One hit, told from every other: its recording's pass ([`pass_key`])
/// and its marker id.
pub type HitKey = (String, String);

/// One hit where the song plays it now: on its piece's first mic, on the
/// lane an edit works on (`EDIT`, else the playing lane).
#[derive(Clone, Debug)]
pub struct Played {
    pub id: String,
    /// The recording it is a hit of: a marker's id is unique within its
    /// recording only — takes recorded from the same place share them.
    pub source: String,
    pub piece: String,
    /// Project seconds, as it plays now.
    pub time: f64,
    pub peak_db: f64,
    pub mark: HitMark,
}

impl Played {
    /// What tells it from every other hit: its recording and its id.
    #[must_use]
    pub fn key(&self) -> (&str, &str) {
        (&self.source, &self.id)
    }
}

/// Where in its take `source` plays, through `markers` (by position): `None`
/// with fewer than two, or outside them.
pub fn stretched(markers: &[daw_proto::StretchMarker], source: f64) -> Option<f64> {
    markers.windows(2).find_map(|pair| {
        let [a, b] = pair else { return None };
        let span = b.source_position - a.source_position;
        (source >= a.source_position && source <= b.source_position && span > 0.0).then(|| {
            ((source - a.source_position) / span).mul_add(b.position - a.position, a.position)
        })
    })
}

/// Every hit of the kit where it plays now — a hit cut into two slices
/// that both play it is here twice, one cut away is not here at all.
#[must_use]
pub fn playback(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Played> {
    // The song's index, kept up to date from the journal — when the song is
    // this process's engine's (`daw` is the one engine a process runs).
    if let Some(song) = crate::hit_index::song_of(project)
        && std::ptr::eq(song.daw(), daw)
    {
        return crate::hit_index::playback(&song).as_ref().clone();
    }
    playback_scan(daw, project)
}

/// [`playback`] read item by item from the engine: every slice of the kit
/// looked at again — what the index is checked against.
#[must_use]
pub fn playback_scan(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Played> {
    let (pieces, _) = kit(daw, project);
    let mut out = Vec::new();
    for piece in &pieces {
        let Some(mic) = piece.mics.first() else {
            continue;
        };
        let Some(track) = Tracks::get(daw, project.clone(), TrackRef::Guid(mic.clone())) else {
            continue;
        };
        let mut by_source: HashMap<String, Vec<TakeMarker>> = HashMap::new();
        for item in daw.get_items(project.clone(), TrackRef::Guid(mic.clone())) {
            if item.muted || !crate::drum_edit::is_edited(&track, &item) {
                continue;
            }
            let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))
            else {
                continue;
            };
            let Some(path) = take.source_file_path.clone() else {
                continue;
            };
            let rate = if take.play_rate > 0.0 {
                take.play_rate
            } else {
                1.0
            };
            let from = take.start_offset.as_seconds();
            let to = item.length.as_seconds().mul_add(rate, from);
            // A slice drawn out over a gap plays less of its source than
            // its length says: where its stretch markers say.
            let stretch = daw_proto::StretchMarkers::get_stretch_markers(
                daw,
                project.clone(),
                ItemRef::Guid(item.guid.clone()),
                TakeRef::Active,
            );
            let (from, to) = match (stretch.first(), stretch.last()) {
                (Some(first), Some(last)) if stretch.len() > 1 => {
                    (first.source_position, last.source_position)
                }
                _ => (from, to),
            };
            let at = |source: f64| {
                stretched(&stretch, source).map_or_else(
                    || item.position.as_seconds() + (source - from) / rate,
                    |t| item.position.as_seconds() + t,
                )
            };
            let markers = by_source.entry(path.clone()).or_insert_with(|| {
                daw.get_take_markers(
                    project.clone(),
                    ItemRef::Guid(item.guid.clone()),
                    TakeRef::Active,
                )
            });
            for m in markers.iter() {
                let Some(h) = &m.hit else { continue };
                if h.piece != piece.name
                    || m.source_position_seconds < from
                    || m.source_position_seconds >= to
                {
                    continue;
                }
                out.push(Played {
                    id: m.id.clone(),
                    source: path.clone(),
                    piece: h.piece.clone(),
                    time: at(m.source_position_seconds),
                    peak_db: h.peak_db,
                    mark: h.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

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
                key: (crate::kit::pass_key(&p.source), p.id.clone()),
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

//! Where every hit of the kit plays now, kept per song and brought up to
//! date from the engine's journal.
//!
//! A hit is a marker on a recording; it plays wherever an EDIT slice plays
//! that moment of the recording. Each slice's hits are worked out once and
//! kept by the slice; after an edit, only the slices the journal names are
//! worked out again — a drag re-reads its few slices, not the song's 27 000.
//! New hit markers (a detection) or a different kit read the whole kit again.
//!
//! ```ignore
//! let hits = hit_index::playback(&song);        // every hit, by time: shared
//! let near = hit_index::between(&song, 84.0, 86.0);
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use daw::service::{ProjectContext, Tracks as _};
use daw::standalone::sync::ProjectState;

use crate::hits::Played;
use crate::song::Song;

/// Every song's index, by song.
static INDEXES: Mutex<Option<HashMap<String, Index>>> = Mutex::new(None);

/// One song's hits.
#[derive(Default)]
struct Index {
    /// The journal read up to.
    cursor: u64,
    /// Each close mic that carries hits, and its piece: what the kit was.
    kit: Vec<(String, String)>,
    /// Each slice's hits, by slice.
    by_item: HashMap<String, Vec<Played>>,
    /// Every hit, by time, and the slice that plays it.
    all: Arc<Vec<Played>>,
    items_of: Vec<String>,
}

/// Every hit of `song`'s kit where it plays now, by time.
#[must_use]
pub fn playback(song: &Song) -> Arc<Vec<Played>> {
    with_index(song, |index| Arc::clone(&index.all)).unwrap_or_default()
}

/// The hits of `song`'s kit that play between `from` and `to`, by time.
#[must_use]
pub fn between(song: &Song, from: f64, to: f64) -> Vec<Played> {
    with_index(song, |index| {
        let lo = index.all.partition_point(|h| h.time < from);
        let hi = index.all.partition_point(|h| h.time < to);
        index
            .all
            .get(lo..hi)
            .map(<[Played]>::to_vec)
            .unwrap_or_default()
    })
    .unwrap_or_default()
}

/// `look` at `song`'s index, brought up to date first.
fn with_index<R>(song: &Song, look: impl FnOnce(&Index) -> R) -> Option<R> {
    // The kit as the tracks say it is: read before the state's lock (the
    // tracks have their own call); compared with what the index was built
    // for under it.
    let tracks = song.daw().all(song.context());
    let pieces = crate::drum_reference::pieces(song.daw(), &song.context());
    let mut indexes = INDEXES.lock().ok()?;
    let index = indexes
        .get_or_insert_with(HashMap::new)
        .entry(song.id().to_string())
        .or_default();
    song.read(|p| index.update(p, &tracks, pieces))?;
    Some(look(index))
}

impl Index {
    /// Up to date with `p`: the slices the journal names since the last
    /// look worked out again, or — with a new kit, new hit markers, or a
    /// journal that cannot say — every slice of the kit.
    fn update(
        &mut self,
        p: &ProjectState,
        tracks: &[daw_proto::Track],
        pieces: Vec<(String, String)>,
    ) {
        let has_items = |guid: &str| p.items_by_track.get(guid).is_some_and(|v| !v.is_empty());
        let (kit, _) = crate::hits::kit_from(tracks, pieces, &has_items);
        // Each piece's hits are read off its first close mic.
        let first_mics: Vec<(String, String)> = kit
            .iter()
            .filter_map(|piece| Some((piece.mics.first()?.clone(), piece.name.clone())))
            .collect();
        let changes = p.item_changes_since(self.cursor);
        let markers_moved = p
            .take_markers
            .written_since(self.cursor)
            .is_none_or(|mut keys| keys.next().is_some());
        let whole = self.cursor == 0 || first_mics != self.kit || markers_moved;
        match changes.items {
            Some(items) if !whole => {
                if items.is_empty() {
                    self.cursor = changes.cursor;
                    return;
                }
                for item in &items {
                    self.by_item.remove(item);
                    if let Some(hits) = item_hits(p, tracks, &first_mics, item) {
                        self.by_item.insert(item.clone(), hits);
                    }
                }
                self.splice(&items);
            }
            _ => {
                self.by_item = first_mics
                    .iter()
                    .flat_map(|(mic, _)| p.items_by_track.get(mic).into_iter().flatten())
                    .filter_map(|item| {
                        Some((item.clone(), item_hits(p, tracks, &first_mics, item)?))
                    })
                    .collect();
                self.rebuild();
            }
        }
        self.kit = first_mics;
        self.cursor = changes.cursor;
    }

    /// Every hit again, from every slice's.
    fn rebuild(&mut self) {
        let mut all: Vec<(String, Played)> = self
            .by_item
            .iter()
            .flat_map(|(item, hits)| hits.iter().map(|h| (item.clone(), h.clone())))
            .collect();
        all.sort_by(|a, b| a.1.time.total_cmp(&b.1.time));
        let (items_of, all) = all.into_iter().unzip();
        self.items_of = items_of;
        self.all = Arc::new(all);
    }

    /// The hits of `changed` slices taken out of the list and their new
    /// ones put in, in order: the rest of the list is not touched.
    fn splice(&mut self, changed: &std::collections::HashSet<String>) {
        let keep: Vec<bool> = self
            .items_of
            .iter()
            .map(|item| !changed.contains(item))
            .collect();
        let all = Arc::make_mut(&mut self.all);
        let mut flags = keep.iter();
        all.retain(|_| *flags.next().unwrap_or(&true));
        let mut flags = keep.iter();
        self.items_of.retain(|_| *flags.next().unwrap_or(&true));
        for item in changed {
            for hit in self.by_item.get(item).into_iter().flatten() {
                let at = all.partition_point(|h| h.time <= hit.time);
                all.insert(at, hit.clone());
                self.items_of.insert(at, item.clone());
            }
        }
    }
}

/// The hits slice `item` plays, where it plays them — `None` when it is
/// not a first close mic's slice on the lane edits are made on.
fn item_hits(
    p: &ProjectState,
    tracks: &[daw_proto::Track],
    first_mics: &[(String, String)],
    item: &str,
) -> Option<Vec<Played>> {
    let item = &p.items.get(item)?.item;
    let (_, piece) = first_mics.iter().find(|(mic, _)| *mic == item.track_guid)?;
    let track = tracks.iter().find(|t| t.guid == item.track_guid)?;
    if item.muted || !crate::drum_edit::is_edited(track, item) {
        return Some(Vec::new());
    }
    let list = p.takes.get(&item.guid)?;
    let take = list.takes.get(usize::try_from(list.active_idx).ok()?)?;
    let key = take
        .source_file_path
        .clone()
        .unwrap_or_else(|| take.guid.clone());
    let rate = if take.play_rate > 0.0 {
        take.play_rate
    } else {
        1.0
    };
    let stretch = p
        .stretch_markers
        .get(&take.guid)
        .map_or(&[][..], Vec::as_slice);
    let from = take.start_offset.as_seconds();
    let (from, to) = match (stretch.first(), stretch.last()) {
        // A slice drawn out over a gap plays less of its source than its
        // length says: where its stretch markers say.
        (Some(first), Some(last)) if stretch.len() > 1 => {
            (first.source_position, last.source_position)
        }
        _ => (from, item.length.as_seconds().mul_add(rate, from)),
    };
    let start = item.position.as_seconds();
    let at = |source: f64| {
        crate::hits::stretched(stretch, source)
            .map_or_else(|| start + (source - from) / rate, |t| start + t)
    };
    Some(
        p.take_markers
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let hit = m.hit.as_ref()?;
                let inside = m.source_position_seconds >= from && m.source_position_seconds < to;
                (hit.piece == *piece && inside).then(|| Played {
                    id: m.id.clone(),
                    piece: hit.piece.clone(),
                    time: at(m.source_position_seconds),
                    peak_db: hit.peak_db,
                    mark: hit.clone(),
                })
            })
            .collect(),
    )
}

/// The song `project` names, when it is open in this process's engine.
#[must_use]
pub fn song_of(project: &ProjectContext) -> Option<Song> {
    match project {
        ProjectContext::Project(guid) => {
            Song::open(daw_proto::id::ProjectId::from(guid.as_str())).ok()
        }
        ProjectContext::Current => Song::current().ok(),
    }
}

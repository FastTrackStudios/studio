# Rewrites — real call sites, before and after

Eight call sites from the repo as of 2026-10-15 (`editing-engine`
branch), each shortened with `…`, then rewritten against the target API in
`SKILL.md`. The "after" code is the shape to build toward: names such as
`ItemId`, `ProjectHandle::item`, `edit_item`, `read`, `SongView` and
`Gesture` do not all exist yet. Each rewrite says what it costs today and
what it would cost after.

`perfect-functions` already rewrites `comp.rs::swipe`, `comp.rs::clear`,
`studio.rs::patch_items` (for honesty) and `item.rs::duplicate_item` (for
levels of abstraction). The ones here overlap only where the API or the
data layout is the point.

---

## 1. Placing a comp piece — five setters, ten clones, ~14 locks

`apps/session/daw/src/comp.rs:311-360` (`place` and `slip`)

```rust
fn place(daw: &Standalone, project: &ProjectContext, copy: &str,
         source: &daw_proto::Item, (a, b): (f64, f64)) -> Result<(), DawError> {
    let at = ItemRef::Guid(copy.to_owned());
    let (start, _) = span(source);
    daw.set_position(project.clone(), at.clone(), PositionInSeconds::from_seconds(a))?;
    slip(daw, project, &at, source.start_offset.as_seconds() + (a - start))?;
    daw.set_length(project.clone(), at.clone(), Duration::from_seconds(b - a))?;
    daw.set_fade_in(project.clone(), at.clone(), Duration::from_seconds(JOIN), source.fade_in_shape)?;
    daw.set_fade_out(project.clone(), at, Duration::from_seconds(JOIN), source.fade_out_shape)
}

fn slip(daw: &Standalone, project: &ProjectContext, item: &ItemRef, offset: f64) -> Result<(), DawError> {
    if daw.get_active_take(project.clone(), item.clone()).is_none() {   // clones a whole Take to test existence
        return Ok(());
    }
    daw.set_start_offset(project.clone(), item.clone(), TakeRef::Active, Duration::from_seconds(offset))
}
```

Cost per piece: six engine calls, each `resolve_project` (lock) +
`with_project_mut` (lock) + a journal note + an `ItemEvent` on the bus,
plus a cloned `Take`. On a kit comp that copies a piece per mic per hit,
that is thousands of lock round-trips and events for one gesture.

```rust
/// The copy `piece` of `source`, cut to `span`: playing what `source`
/// plays there, meeting its neighbours with the short crossfade.
fn place(song: &ProjectHandle, piece: &ItemId, source: &Item, span: Span) -> DawResult<()> {
    let into_source = source.start_offset + (span.from - source.position);
    song.edit_item(piece, |item| {
        item.position = span.from;
        item.length = span.len();
        item.fade_in = Fade::new(JOIN, source.fade_in_shape);
        item.fade_out = Fade::new(JOIN, source.fade_out_shape);
        item.active_take_mut().map(|take| take.start_offset = into_source);
    })
}
```

One lock, one journal note per touched map, one `ItemEvent::Changed`. No
id is cloned at the call site; `Span` carries the "finite, `from < to`"
proof (perfect-functions example 3). `Fade` bundles length and shape,
which today travel as two parameters of `set_fade_in`.

---

## 2. Which take the comp plays — an N+1 query in a nested loop

`apps/session/daw/src/comp_rank.rs:148-182` (`current`)

```rust
fn current(daw: &Standalone, project: &ProjectContext, versions: &[TakeVersion],
           (from, to): (f64, f64)) -> Option<String> {
    let mid = (from + to) / 2.0;
    let tracks = Tracks::all(daw, project.clone());
    versions.iter().find_map(|v| {
        let hit = v.files.iter().any(|(track, path)| {
            let Some(t) = tracks.iter().find(|t| t.guid == *track) else { return false };
            let comp = crate::drum_lanes::lane_named(t, crate::drum_lanes::COMP).unwrap_or(0);
            daw.get_items(project.clone(), daw::service::TrackRef::Guid(track.clone()))  // whole track, cloned
                .into_iter()
                .filter(|i| i.fixed_lane.unwrap_or(0) == comp)
                .filter(|i| { let at = i.position.as_seconds(); mid >= at && mid < at + i.length.as_seconds() })
                .any(|i| {
                    daw::service::Takes::get_active_take(daw, project.clone(),
                        daw::service::ItemRef::Guid(i.guid))                           // a lock per item
                    .and_then(|take| take.source_file_path)
                    .is_some_and(|p| p == *path)
                })
        });
        hit.then(|| v.pass.clone())
    })
}
```

For every (version × mic file) it clones every item on the mic's track
(thousands on a sliced kit) and fetches a take per candidate.

```rust
/// The pass the comp plays at the middle of `span`.
fn current<'v>(song: &SongView, versions: &'v [TakeVersion], span: Span) -> Option<&'v PassId> {
    let mid = span.mid();
    let plays = |track: &TrackId, file: &Path| {
        let comp = song.track(track).and_then(|t| lane_named(t, COMP)).unwrap_or(0);
        song.items_at(track, mid)                                   // binary search, borrowed
            .filter(|item| item.lane() == comp)
            .any(|item| song.active_take(&item.id).is_some_and(|take| take.source() == Some(file)))
    };
    versions
        .iter()
        .find(|v| v.files.iter().any(|(track, file)| plays(track, file)))
        .map(|v| &v.pass)
}
// caller: let now = daw.read(&song_id, |song| current(song, &versions, span).cloned());
```

One lock for the whole question (the caller's `read`), no item or take
cloned, `items_at` is O(log n) over a lane kept sorted. The function is
honest and testable against a `SongView` built from fixtures.

---

## 3. Two fields from two fetches

`apps/session/daw/src/hits.rs:1215-1229` (`recording_origin`)

```rust
fn recording_origin(daw: &Standalone, project: &ProjectContext, item: &str) -> Option<f64> {
    let i = daw.get_item(project.clone(), ItemRef::Guid(item.to_owned()))?;
    let take = daw.get_active_take(project.clone(), ItemRef::Guid(item.to_owned()))?;
    let rate = if take.play_rate > 0.0 { take.play_rate } else { 1.0 };
    Some(i.position.as_seconds() - take.start_offset.as_seconds() / rate)
}
```

```rust
/// Where `item`'s recording starts on the timeline.
fn recording_origin(song: &SongView, item: &ItemId) -> Option<Seconds> {
    let take = song.active_take(item)?;
    Some(song.item(item)?.position - take.start_offset / take.rate())
}
```

`Take::rate()` owns the "zero means 1.0" rule once instead of at each of
its readers. `hits.rs` has 15 `ItemRef::Guid(…clone())` sites of this
shape; most collapse the same way.

---

## 4. The window's read-back — O(song) work per edit

`apps/session/daw/src/studio.rs:491-555` (`patch_items`)

```rust
for (track, items) in &mut raw.items {                         // every lane in the song
    let before = items.len();
    items.retain(|i| !changed.contains(&i.guid));
    if items.len() != before { lanes.insert(track.clone()); }
}
for guid in &changed {
    raw.names.remove(guid);
    raw.midi.remove(guid);
    let Some(item) = daw.get_item(project.clone(), ItemRef::Guid(guid.clone())) else { continue };
    if let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(guid.clone())) { … }
    raw.items.entry(item.track_guid.clone()).or_default().push(item);
    …
}
…
raw.item_count = raw.items.values().map(Vec::len).sum();       // every lane again
let reach = raw.items.values().flatten()                        // every item again
    .map(|i| i.position.as_seconds() + i.length.as_seconds()).fold(0.0_f64, f64::max);
```

Moving one hit walks all 27 000 items three times (`retain`, `sum`,
`fold`) and takes two locks per changed item.

```rust
/// The UI's copy of the song, patched by what the engine says changed.
pub struct SongMirror {
    lanes: HashMap<TrackId, Arc<Lane>>,      // a lane is sorted items + their titles; shared with the last frame
    where_is: HashMap<ItemId, TrackId>,      // which lane holds each item
    reach: Reach,                            // max end, kept with a count per end so a removal is O(1)
    cursor: Cursor,
}

impl SongMirror {
    pub fn patch(&mut self, daw: &Standalone, song: &ProjectId) -> Patched {
        let Some(changes) = daw.changes_since(song, &self.cursor) else { return Patched::Whole };
        self.cursor = changes.cursor;
        daw.read(song, |s| {
            for id in &changes.items {
                self.remove(id);
                if let Some(now) = s.item(id) { self.insert(now, s.active_take(id)); }
            }
        });
        Patched::Items(changes.items)
    }
}
```

`remove`/`insert` touch only the lanes `where_is` names, and copy just
those lanes (`Arc::make_mut`), so the previous frame's `Arc<Lane>`s for
untouched tracks stay pointer-equal and the painter can skip them.
`item_count` is `where_is.len()`. The cursor is a field, not the `SEEN`
static, and the read is one lock.

---

## 5. The engine's own setter plumbing — two locks and a guid clone per call

`crates/daw/standalone/src/item.rs:31-60` (`mutate_item_evt`, `resolve_project`)

```rust
fn mutate_item_evt<F>(daw: &Standalone, project: &ProjectContext, item: &ItemRef, f: F) -> DawResult<()>
where F: FnOnce(&str, &str, &mut Item) -> ItemEvent {
    let guid = resolve_project(daw, project).ok_or_else(no_project)?;     // lock #1, String clone
    let event = daw.with_project_mut(&guid, |p| {                        // lock #2
        let item_guid = item_guid_from_ref(p, item)                       // String clone
            .ok_or_else(|| DawError::not_found("Item", &format!("{item:?}")))?;
        let entry = p.items.get_mut(&item_guid).ok_or_else(|| DawError::not_found("Item", &item_guid))?;
        let event = f(&guid, &item_guid, &mut entry.item);
        Ok::<ItemEvent, DawError>(event)
    })??;
    publish_item_event(daw, event);
    Ok(())
}

fn item_guid_from_ref(p: &ProjectState, item: &ItemRef) -> Option<String> {
    match item {
        ItemRef::Guid(g) => Some(g.clone()),
        ItemRef::ProjectIndex(idx) => p.items.keys().nth(*idx as usize).cloned(),   // HashMap order: arbitrary
        ItemRef::Index(_) => None,
    }
}
```

```rust
impl Standalone {
    /// Change one item in place, as one write: one lock, one event.
    pub fn edit_item(&self, project: &ProjectContext, item: &ItemRef,
                     edit: impl FnOnce(&mut ItemEdit<'_>)) -> DawResult<()> {
        let event = self.with_song_mut(project, |song| {
            let id = song.resolve(item)?;                 // &ItemId borrowed from the map, no clone
            let mut item = song.items.edit(id)?;          // journals `id` once
            edit(&mut item);
            Ok(ItemEvent::changed(song.id(), id))
        })?;
        self.publish(event);
        Ok(())
    }
}
```

`with_song_mut` resolves `ProjectContext::Current` inside the lock it
already holds. Each `set_*` in `impl Items for Standalone` becomes a one
line `self.edit_item(&project, &item, |i| i.muted = muted)`, and the RPC
`patch_item` is the same call with an `ItemPatch`. `ItemRef::ProjectIndex`
either gets an ordered index or goes; "the Nth key of a HashMap" is not
an identity.

---

## 6. Undo snapshots and the take→item scan

`crates/daw/standalone/src/sync/daw.rs:524-543` (`EditSnapshot::capture`) and
`:1478-1486` (in `item_changes_since`)

```rust
pub fn capture(p: &ProjectState) -> Self {
    Self {
        tracks: p.tracks.clone(),
        items: (*p.items).clone(),                 // 27 000 Items, each with 2–3 Strings
        items_by_track: p.items_by_track.clone(),
        takes: (*p.takes).clone(),                 // every Take, its Strings
        midi_notes: (*p.midi_notes).clone(),
        envelopes: (*p.envelopes).clone(),
        …
    }
}
…
// A take's write is its item's: found through the takes
if !takes.is_empty() {
    for (item, list) in p.takes.iter() {           // every item's take list, per read-back
        if list.takes.iter().any(|t| takes.contains(&t.guid)) { items.insert(item.clone()); }
    }
}
```

And `restore` puts every map back with `.into()`, which tells every reader
"everything changed" — so an undo of one moved hit re-reads the whole song.

```rust
/// The project's editable state; cloning it is O(1) (structurally shared).
#[derive(Clone)]
pub struct Edits {
    tracks: imbl::Vector<Track>,
    items: imbl::OrdMap<ItemId, ItemEntry>,
    takes: imbl::OrdMap<ItemId, TakeList>,
    item_of_take: imbl::OrdMap<TakeId, ItemId>,    // the reverse index the scan was standing in for
    midi_notes: imbl::OrdMap<TakeId, Arc<[MidiNote]>>,
    …
}

pub fn capture(p: &ProjectState) -> EditSnapshot { EditSnapshot(p.edits.clone()) }

pub fn restore(snapshot: EditSnapshot, p: &mut ProjectState) {
    p.journal.note_diff(&p.edits, &snapshot.0);       // only the keys that differ
    p.edits = snapshot.0;
}
```

`imbl::OrdMap::diff` is documented to skip nodes the two maps share, so
`note_diff` costs what changed, not what exists, and undo/redo stays
incremental for the window. (`imbl::HashMap` only offers
`symmetric_difference`, O(n log n) with no such promise — use `OrdMap`
where a diff is wanted; it also gives a defined iteration order.) `Journaled` keeps its API; only its backing
map changes. Measure first: `imbl::HashMap` lookups are slower than std
by a constant factor (see `research.md`), so keep the per-block audio
snapshot on its own flattened form.

---

## 7. `Edit` — sixty positional variants and a `guid()` that guesses

`apps/session/daw/src/engine.rs:30-316` (enum) and `:340-400` (`guid`)

```rust
pub enum Edit {
    SetVolume(String, f64),
    SetFxParam(String, String, u32, f64),
    MoveItem(String, f64),
    SplitItem(String, f64, String),
    AddMarker(String, f64, u32),
    …
}

/// The track this is about.
pub fn guid(&self) -> &str {
    match self {
        Self::SetVolume(g, _) | Self::SetFxParam(g, ..) | Self::MoveItem(g, _)   // an item guid, not a track's
        | Self::SplitItem(g, ..) | Self::AddMarker(g, ..) | …                   // …and this one a project's
        => g,
    }
}
```

`SplitItem(String, f64, String)`: which string is the item and which the
new right half? The reader must open the doc comment.

```rust
pub enum Edit {
    Track(TrackId, TrackEdit),
    Item(ItemId, ItemEdit),
    Project(ProjectEdit),
    Done(Ticket),
}

pub enum TrackEdit {
    Volume(Gain),
    FxParam { fx: FxId, param: u32, value: f64 },
    …
}

pub enum ItemEdit {
    Move { to: Seconds },
    Split { at: Seconds, right: ItemId },
    Trim(Span),
    …
}

// call site, arrange_edit.rs:2869
splits.push(Edit::Item(item.id.clone(), ItemEdit::Split { at, right: right.id.clone() }));
```

"What is this about" is now the variant's first field and its type says
which kind of thing. The 60-arm `guid()` match goes away; a queue that
coalesces edits by target keys on `Target::Item(&ItemId)`.

---

## 8. The window's gestures — a dozen `Option`s and a priority ladder

`apps/session/daw/src/widget.rs:607-648` (fields) and `:4423-4520` (`took`)

```rust
pub struct ArrangementWidget {
    …
    hit_drag: Option<HitDrag>,
    hit_marquee: Option<((f64, f64), (f64, f64))>,
    swipe: Option<crate::swipe::Swipe>,
    turning: Option<Turn>,
    item_turn: Option<ItemTurn>,
    lane_drag: Option<crate::automation_edit::LaneDrag>,
    take_drag: Option<TakeDrag>,
    menu: Option<crate::popup::Popup<crate::automation_menu::Pick>>,
    …
}

UiEvent::PointerMove(e) => {
    …
    if let Some(menu) = self.menu.as_mut() { return menu.hover_at(x, y); }
    if self.take_drag.is_some() { self.take_moved(x, y); return true; }
    if self.lane_drag.is_some() { … let Some(drag) = self.lane_drag.as_mut() else { return false }; … }
    if self.holding { … return self.held_drag(x, y, mods(e.mods)); }
    if let Some(turn) = self.item_turn.as_mut() { …; return true; }
    if self.turning.is_some() { self.turn(y, …); return true; }
    …
}
```

Nothing stops two of these being `Some` at once; the order of the `if`s
silently decides which wins, and `took` is 325 lines.

```rust
enum Gesture {
    Idle,
    Menu(Popup<Pick>),
    TakeDrag(TakeDrag),
    LaneDrag(LaneDrag),
    Held(Held),
    ItemTurn(ItemTurn),
    Knob(Turn),
    HitDrag(HitDrag),
    HitMarquee(Marquee),
    Swipe(Swipe),
}

fn pointer_moved(&mut self, at: Point, mods: Mods) -> bool {
    self.share_pointer(at);                                 // the ghosts block, now a method
    match &mut self.gesture {
        Gesture::Menu(menu) => menu.hover_at(at),
        Gesture::TakeDrag(drag) => self.take_moved(drag, at),
        Gesture::LaneDrag(drag) => self.lane_dragged(drag, at),
        Gesture::Held(held) => self.held_drag(held, at, mods),
        Gesture::ItemTurn(turn) => turn.turn(at.y, mods.fine()),
        Gesture::Knob(turn) => self.turn(turn, at.y, mods.fine()),
        Gesture::Idle | … => self.hover(at),
    }
}
```

(Borrowing `self.gesture` and `self` together needs the usual
`std::mem::replace(&mut self.gesture, Gesture::Idle)` take-and-put-back, or
methods on the gesture types that take the few fields they need — which is
also the honest-function shape.) `Editor` in `arrange_edit.rs:130-176`
has the same pattern (`item_press`, `fade_drag`, `razor_drag`,
`ruler_press`, `marquee`, `time_drag`, `snapback`).

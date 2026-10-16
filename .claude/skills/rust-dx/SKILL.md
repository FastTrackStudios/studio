---
name: rust-dx
description: How Rust APIs in this repo should look and perform — typed ids instead of String guids, handles/scopes instead of `ProjectContext` + `ItemRef::Guid(x.clone())` threading, one-lock batched edits instead of chains of setters, borrowed read views instead of cloned Vec<Item>, change journals and persistent (structurally shared) snapshots for the 27 000-item song, gesture/state enums instead of piles of Options, typed errors instead of String. Load before designing or reviewing a public type, trait, service method or module boundary, before touching the daw proto/standalone item/take/track APIs, the Session window's read-back (studio.rs, widget.rs, midi.rs, daw-ui project/folded), or when a call site needs more than one clone or more than one engine call to say one thing. Pairs with `perfect-functions` (function shape) — this skill is about types, data and API surfaces.
---

# Rust DX — APIs you could publish as a snippet

The bar: **every code block reads on its own, like a snippet on a docs
site**, and the code that reads well is also the fast code. In this repo
those two goals point the same way almost every time — the ceremony
(`project.clone()`, `ItemRef::Guid(item.guid.clone())`, one RPC per field)
is also the cost (a lock, a resolve, an allocation and an event per call,
times 27 000 items on a sliced kit, inside a 5 ms redraw budget).

- `perfect-functions` covers the *shape of a function* (honest signatures,
  one level of abstraction, invariants in parameter types). Load it too.
- This skill covers *types, data and API surfaces*: what an id is, how a
  caller reaches an item, how an edit is batched, how a reader learns what
  changed, how state is snapshotted.
- Long material lives next to this file:
  - `rewrites.md` — eight before/after rewrites of real call sites.
  - `research.md` — the sources and what each contributes, plus the
    comparison with SoundCraft (`storytold/soundcraft`).

## When to load

- Adding or changing a `#[architect::rpc]` trait, a proto type, an `Edit`
  variant, a handle/scope, or anything in `crates/daw/{proto,standalone}`.
- Writing code that calls `Items`/`Takes`/`Tracks` more than once for one
  logical thing, or that loops calling the engine per item.
- Touching the window's read-back path (`studio.rs::patch_items`,
  `daw_ui::studio::project`, `folded.rs`, `midi.rs::Previews`) or any
  per-frame code.
- Reviewing a diff with `.clone()` in an argument position, a `String` that
  is really an id, a `Result<_, String>`, or a new `static`.

## The principles

1. **Say it once.** Context (which project, which item) is bound once, in
   a handle or scope, never re-passed per call. If a call site repeats an
   argument on three consecutive lines, the API is missing a handle.
2. **Ids are types.** A track guid, an item guid, a take guid and a project
   guid are four types. A `String` can be any of them, which is how
   `Edit::guid()` ended up returning "the track this is about" for an
   *item* move.
3. **Cheap to hold, cheap to pass.** Ids are `Copy` or `Arc`-backed (clone =
   refcount bump). Context types are passed by reference. No API should
   force `x.clone()` at the call site to satisfy a by-value parameter it
   only reads (C-CALLER-CONTROL).
4. **One edit, one lock, one event.** Changing four fields of one item is
   one call that takes the lock once, writes the journal once and publishes
   one `ItemEvent`. Chains of `set_position`/`set_length`/`set_fade_in` are
   N locks, N resolves, N events and N journal entries.
5. **Read by borrowing.** Readers get `&Item`/`&Take` inside a closure or a
   view — not a cloned `Vec<Item>` of the whole track to find one item.
6. **Readers ask what changed.** Every big collection the window mirrors is
   journaled (`journal::Journaled`) or structurally shared (`imbl`,
   `Arc<[T]>` per track) so a reader pays O(changed), not O(song).
7. **Make illegal states unrepresentable.** Mutually exclusive state is an
   enum, not N `Option` fields; a span is a type with `from < to`; a folded
   row's item is an enum variant, not a `"folded:{guid}:{i}"` string.
8. **Errors are values with a type.** `thiserror` enums in libraries and
   app modules; `String` only at the outermost user-facing edge.

## Target idioms

The names below are the target API. Some exist (the `handle` module,
`journal::Journaled`, architect `scopes(...)`); the rest are what to build
toward — see `rewrites.md` for each against real code. When you add one,
add it in the owning crate (`daw_proto` for wire types and ext traits,
`daw_standalone` for engine internals) so every caller gets it.

### Typed ids

```rust
/// An item's identity: its guid, shared, never re-allocated on clone.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Facet)]
#[facet(transparent)]
pub struct ItemId(Guid);            // Guid = Arc<str> (or String until Facet takes Arc<str>)

impl From<&ItemId> for ItemRef { fn from(id: &ItemId) -> Self { ItemRef::Guid(id.clone()) } }
impl Borrow<str> for ItemId { … } // look up a HashMap<ItemId, _> with &str during migration
```

- One newtype per kind: `ProjectId`, `TrackId`, `ItemId`, `TakeId`,
  `FxId`. Precedent in-repo: `crates/frame/src/document/proto/model.rs`
  (`NodeId`, `AssetId`, `#[facet(transparent)]`).
- Fields become typed: `Item { id: ItemId, track: TrackId, … }`.
- Synthetic ids are enums, not encoded strings:
  `enum RowItem { Real(ItemId), Folded { folder: TrackId, span: u32 } }`.
- Inside the engine, prefer dense keys for the hot maps (slotmap
  `new_key_type!` or a `u32` index with a `SecondaryMap`), mapped to/from
  the guid once at the boundary. Guids stay the wire and file identity.

### Handles and scopes, not threaded context

```rust
let song = daw.project(&song_id);          // ProjectHandle: binds backend + project
let item = song.item(&item_id);            // ItemHandle: binds the item too
item.set_length(Duration::from_seconds(2.0))?;
let take = item.active_take()?;            // TakeHandle, same idea
```

- `#[architect::rpc(scopes(project: ProjectContext, item: ItemRef))]` on
  `Items` and `Takes` generates the scope views for free (Tracks already
  has `scopes(project, track)` → `TracksProjectScope`/`TracksTrackScope`).
  Wrap them in `daw_proto::handle` the way `ProjectHandle`/`TrackHandle`
  already wrap Tracks.
- Scope views clone their bound args on every call — another reason ids
  must be `Arc`-cheap.
- A function that works on one item takes an `ItemHandle` (or `&ItemId`
  plus a `&ProjectHandle`), never `(daw, project, guid)` as three params.

### Batched edits: one closure, one lock

```rust
song.edit_item(&piece, |item| {
    item.position = PositionInSeconds::from_seconds(to);
    item.length   = Duration::from_seconds(end - to);
    item.fade_in  = Fade::NONE.shaped(source.fade_in_shape);
})?;                                       // one lock, one journal note, one ItemEvent::Changed
```

- In-process (`Standalone`): `edit_item(id, FnOnce(&mut ItemEdit))` and a
  wider `edit(|song: &mut SongMut| …)` that hands out journaled `&mut`
  access. This is the SoundCraft/GPUI shape: data first, mutate fields,
  the container records what changed.
- Over RPC (Facet, ≤ 4 params): an `ItemPatch` struct with `Option`
  fields (`patch_item(project, item, patch)`), or the existing `batch`
  ops program (`crates/daw/proto/src/batch`, `daw_control::batch`). Never
  grow a fifth parameter; never add a setter per field for a new caller.
- Undo is a scope, not a pair: `song.undo_step("Comp from lane 2", |song| …)`
  (or an RAII `UndoBlock` guard) so `?` cannot skip `end_undo_block`.

### Borrowed read views

```rust
let origin = song.read(|s| {
    let item = s.item(&id)?;               // &Item
    let take = s.active_take(&id)?;        // &Take
    Some(item.position.as_seconds() - take.start_offset.as_seconds() / take.rate())
});
```

- One lock for the whole question, zero clones; return only the answer.
- `SongView` exposes iterators, not `Vec`s: `items_on(&TrackId)`,
  `items_in(&TrackId, Span)`, `active_take(&ItemId)`, `item_of_take(&TakeId)`
  (backed by a reverse index, not a scan of every take list).
- Keep `get_items`/`get_item` RPCs for remote clients; in-process code with
  a `&Standalone` uses the view.

### Change journals and structural sharing

- `journal::Journaled<K, V>` is the pattern: no `DerefMut`, every write
  names its key, readers hold a cursor (`journal::now()`) and ask
  `written_since(cursor)`. Bevy's change ticks and salsa's revisions are
  the same idea. Extend it, don't bypass it (`write_project` is a test hook).
- Pair it with a cursor *type*, not a bare `u64` in a `static`:
  `struct Cursor(u64)` owned by the reader (the window's mirror), returned
  updated by `changes_since(&Cursor) -> Changes`.
- `Changes` should name typed ids (`HashSet<ItemId>`), and say *removed*
  vs *written*, so the reader does not have to re-query to find out.
- For snapshots (undo, the UI's `Arc<Project>`): store big collections in
  `imbl::OrdMap`/`imbl::Vector` or per-track `Arc<[Item]>`. Clone is O(1),
  an edit copies one path or one track, and two snapshots diff cheaply
  (`OrdMap::diff` skips shared nodes; `Arc::ptr_eq` per track) — so undo
  stays exact instead of "everything changed" (`EditSnapshot::restore`
  today calls `note_all`). `imbl::HashMap` has no sharing-aware diff.
- The UI mirror keeps an index (`HashMap<ItemId, (TrackId, usize)>`) so a
  patch touches only the changed items' lanes; derived totals
  (`item_count`, `length_secs`) are maintained incrementally, not re-summed
  over 27 000 items.

### State machines as enums

```rust
enum Gesture {                // at most one at a time, by construction
    Idle,
    TakeDrag(TakeDrag),
    LaneDrag(LaneDrag),
    ItemTurn(ItemTurn),
    Knob(Turn),
    HitDrag(HitDrag),
    Marquee { from: Point, to: Point },
}
```

- `ArrangementWidget` holds a dozen gesture `Option`s
  (`take_drag`, `lane_drag`, `item_turn`, `turning`, `hit_drag`,
  `hit_marquee`, `swipe`, `finger`, …) that `took()` checks in priority
  order. One `gesture: Gesture` field makes "two drags at once"
  unrepresentable and turns a 300-line `if let` ladder into a `match`
  that dispatches to one method per variant.
- Same for `Editor` (`item_press`, `fade_drag`, `razor_drag`,
  `ruler_press`, `marquee`, `time_drag`, `snapback`).

### Commands and edit messages

- Variants carry named, typed fields:
  `Edit::SetFxParam { track: TrackId, fx: FxId, param: u32, value: f64 }`,
  not `SetFxParam(String, String, u32, f64)`.
- Group by target: `Edit::Track(TrackId, TrackEdit)`,
  `Edit::Item(ItemId, ItemEdit)`, `Edit::Project(ProjectEdit)`. Then
  "what is this about" is a field, not a 60-arm match returning `&str`.

### Errors

- Libraries and engine modules: a `thiserror` enum per module boundary
  (`CompError::NothingToComp { from, to }`, `CompError::NoLane { lane }`,
  `#[from] DawError`). `Display` is the user sentence; the variant is what
  code matches.
- `Result<_, String>` (170 of them in `apps/session/daw/src`) only where the
  value goes straight to a person (an action's status line).
- Never `Option` to swallow an error (`.ok()?` on a lock or a not-found).
  `DawResult` already exists — use it.

### Time and spans

- A half-open span is a type: `Span { from: Seconds, to: Seconds }` with a
  constructor that refuses NaN, backwards and sub-`GRAIN` spans. Not
  `(from, to): (f64, f64)`. SoundCraft's `Range` and `Samples` newtypes are
  the model (see `research.md`).
- `PositionInSeconds`/`Duration` exist — accept them in APIs, convert to
  `f64` at the arithmetic, not at every call.
- What a time conversion needs travels as one small `Copy` struct
  (`TimeCtx { tempo: &TempoMap, … }`), not as `(daw, project)` so the
  callee can re-fetch the tempo map.
- A track's items are kept sorted by position (state the invariant on the
  field); "items at t" and "items in span" are binary searches.

### Globals

- `crate::open::local_engine()`, `current_song()`, `RESYNC`/`ALL`/`SEEN`,
  `midi::STALE`, `bar_step::STEP`, `cursor::CURRENT`, … (≈160 `static`s in
  `apps/session/daw/src`) are ambient state. New code takes what it needs as
  parameters; the window owns a `Session { daw, song, cursor, previews }`
  context and passes `&Session` down. See `perfect-functions` principle 1.

## Performance rules that shape the API

- **No per-item engine call in a loop over a track or the song.** That is
  N locks and N clones; ask the view once (`items_on`, `active_takes`).
- **No O(song) work per edit on the redraw path**: `retain` over every
  lane, `fold` over every item, `values().map(len).sum()`. Maintain it.
- **No deep clone for a snapshot.** `EditSnapshot::capture` clones every
  item, take and MIDI map per undo step; at 27 000 items that is tens of
  thousands of allocations. Persistent maps make it O(1).
- **Return `Arc<T>`/`&T`, not `T`, from caches read per frame**
  (`Previews::get` clones a `Vec<Note>` per call; `wave` correctly
  returns `Arc<Wave>`).
- **Lock once per request.** `resolve_project` locks `state`, then
  `with_project_mut` locks it again; resolve inside the one lock.
- **Deterministic lookups.** `ItemRef::ProjectIndex(n)` is
  `p.items.keys().nth(n)` on a `HashMap` — O(n) and in no defined order.
  Index-based refs need an ordered index or should be removed.
- Measure with the span fields (`architect_telemetry::timing::Stages`,
  `PERF_RECENT`), never with prints — see `logging-best-practices`.

## What SoundCraft changes (details in `research.md` §10)

`storytold/soundcraft` (MIT OR Apache-2.0, so its ideas are free to use
here) is a Rust DAW whose model is plain data edited through `&mut`:

- **Adopt:** `Copy` counter-minted typed ids as the *internal* key
  (`TrackId(u64)`; guids stay our wire identity); edits as honest
  functions over `&mut` document data that just assign fields (our form:
  `edit_item`/`edit` closures over journaled access); a normalised
  half-open `Range` + `TimeCtx { tempo, sr }` (our `Span` + `TimeCtx`);
  `Arc::ptr_eq` as the free "nothing changed" test; per-track item lists
  kept sorted with the invariant written down; actions whose `enabled`
  returns `Err(reason)`.
- **Do not adopt:** whole-document `Arc::make_mut` (the first write after
  an undo point deep-copies the song — the cost we are removing); one
  global `revision` for change notification (our journal is finer); linear
  `find_clip` by id; string flag keys, `view: String`, `bool` mode
  parameters and `String` payloads in error variants.

## Review checklist

Run on every public item and every call site you touch:

- [ ] No `ItemRef::Guid(x.clone())`, `TrackRef::Guid(x.clone())` or
      `project.clone()` at the call site — a handle or `From<&Id>` does it.
- [ ] Ids are typed (`ItemId`, not `String`); no id encoded in a string
      format that is parsed back.
- [ ] Parameters that are only read are borrowed (`&ItemId`, `&str`,
      `&[T]`, `impl IntoIterator`); owned only when kept.
- [ ] An edit of several fields of one thing is one call / one lock.
- [ ] No engine call inside a loop over items or tracks.
- [ ] Per-frame code does no O(song) walk and no deep clone.
- [ ] Mutually exclusive state is one enum field.
- [ ] Begin/end pairs are a closure scope or an RAII guard.
- [ ] Errors are a typed enum (or `DawError`); no `Option` hiding failure.
- [ ] Pure getters, builders and "changes since" results are `#[must_use]`.
- [ ] No new `static`; context comes in through parameters.
- [ ] Enum variants with more than one field use named fields.
- [ ] Spans/time ranges use a span type, not `(f64, f64)`.
- [ ] RPC methods stay ≤ 4 params by taking a struct named for the call
      (`ItemSpan`, `ItemPatch`), never by splitting into more methods.
- [ ] The doc comment says what the thing *is*; the snippet using it reads
      without the doc.

## Anti-patterns seen here (and what replaces them)

| Seen | Where (example) | Instead |
|---|---|---|
| `daw.set_x(project.clone(), at.clone(), …)?;` ×5 | `apps/session/daw/src/comp.rs:249-360` (`clear`, `place`, `slip`) | `song.edit_item(&id, \|i\| { … })` |
| per-item `get_active_take` inside `get_items` loop | `comp_rank.rs:148-182` (`current`) | one `song.read(\|s\| …)` with `items_in` + `active_take` |
| `get_item` + `get_active_take` for two fields | `hits.rs:1215-1229` | one borrowed read returning `f64` |
| `retain` over every lane + `fold` over every item per edit | `studio.rs:491-555` (`patch_items`) | mirror with `ItemId → (TrackId, slot)` index, incremental totals |
| whole-state deep clone per undo step | `standalone/src/sync/daw.rs:524-543` | `imbl` maps / per-track `Arc` |
| `"folded:{guid}:{i}"` ids parsed back | `daw-ui/src/studio/folded.rs:211-226` | `RowItem` enum |
| `Edit::guid() -> &str` over 60 positional variants | `apps/session/daw/src/engine.rs:30-400` | `Edit::Item(ItemId, ItemEdit)` etc. |
| a dozen gesture `Option`s | `apps/session/daw/src/widget.rs:607-648` | `gesture: Gesture` |
| `begin_undo_block` … `end_undo_block` by hand | `comp.rs:107-124` | `undo_step(label, \|\| …)` |
| `Result<_, String>` in engine-side modules | `comp.rs`, `comp_rank.rs`, … | `thiserror` enum |
| `static SEEN/RESYNC/STALE` cursors and queues | `studio.rs:412-422`, `midi.rs:65` | a reader-owned `Cursor`, a `Session` context |
| double lock (`resolve_project` then `with_project_mut`) | `standalone/src/item.rs:31-60` | resolve inside one lock |

## Migrating without a flag day

1. Add the id newtypes with `From<String>`, `Borrow<str>`, `Display`, and
   `From<&ItemId> for ItemRef`. Existing `String` fields keep working.
2. Add `scopes(...)` to `Items`/`Takes` and `ItemHandle`/`TakeHandle` in
   `daw_proto::handle`. New code uses handles; old code is untouched.
3. Add `Standalone::read`/`edit_item`/`undo_step` (in-process) and
   `ItemPatch` (RPC). Port the hottest call sites first: `comp.rs`,
   `hits.rs`, `comp_rank.rs`, `studio.rs::patch_items`.
4. Switch `Item`/`Take`/`Track` fields to typed ids crate by crate.
5. Swap `ProjectState`'s hot maps to persistent ones behind `Journaled`
   (its API does not change), then delete `EditSnapshot`'s deep copy.

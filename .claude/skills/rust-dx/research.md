# Research notes — sources and what each contributes

Gathered 2026-10-15. Paraphrased; follow the links for the originals.
Each section ends with what it means *here*.

## 1. Rust API Guidelines (rust-lang.github.io/api-guidelines)

- Checklist: <https://rust-lang.github.io/api-guidelines/checklist.html>
- Type safety: <https://rust-lang.github.io/api-guidelines/type-safety.html>
- Flexibility: <https://rust-lang.github.io/api-guidelines/flexibility.html>

What matters most for this repo:

- **C-NEWTYPE** — wrap a primitive so values of different meaning cannot be
  mixed; zero runtime cost, explicit conversions. → `ItemId`/`TrackId`
  instead of `String`.
- **C-CUSTOM-TYPE** — arguments carry meaning through types, not `bool` or
  `Option`; call sites read on their own and gain options without breaking.
  → `Fade`, `Span`, `Overlap`, no `shuffle: bool`.
- **C-BUILDER** — complex values get a builder; prefer non-consuming
  (`&mut self`) builders so configuration can be staged. → `SendBuilder` in
  `daw_proto::handle` already does this; `ItemPatch` can be a builder.
- **C-CALLER-CONTROL** — take ownership only when you keep the value;
  borrow when you only read. → the `project: ProjectContext` /
  `item: ItemRef` by-value parameters force a clone per call on callers
  that only lend. In-process wrappers should take `&` and the RPC layer
  can own.
- **C-GENERIC** — accept `impl IntoIterator` rather than a concrete
  collection when you only iterate.
- **C-INTERMEDIATE** — expose intermediate results so the caller does not
  redo work. → `changes_since` returning *which* items and *whether
  removed*, so the reader does not re-query.
- **C-COMMON-TRAITS / C-CONV-TRAITS** — implement `Clone, Debug, Eq,
  Hash, Ord, Display, From, AsRef` eagerly on id types.
- **C-GOOD-ERR** — errors are meaningful, implement `std::error::Error`,
  `Display`. → `thiserror` enums, not `String`.
- **C-DEREF** — only smart pointers implement `Deref`. The repo's
  handle-with-`Deref`-to-scope pattern (`ProjectHandle → TracksProjectScope`,
  `TrackManager → D`) bends this rule on purpose; keep it to handles whose
  target *is* the thing they wrap, and do not add inherent methods that
  shadow the target's.
- **C-METHOD** — functions with a clear receiver are methods. → `song.item(id)`
  rather than `get_item(daw, project, id)`.

## 2. Effective Rust (David Drysdale) — builders vs `..Default::default()`

<https://effective-rust.com/builders.html>

Struct literal with `..Default::default()` fits when every field has a
sensible default and the struct may grow; a builder fits when some fields
have no sensible default (required in `new`) or when construction logic
should live in one place. A consuming builder cannot be staged
conditionally; a `&mut self` builder can. → `Item::default()` followed by
six field assignments in `standalone/src/item.rs:167-173` is fine as a
struct literal (`Item { id, track, index, position, length, ..Item::default() }`);
`ItemPatch` (all-optional) is naturally `ItemPatch::default().length(d).fade_in(f)`.

## 3. "Parse, don't validate" and illegal states

Secondary sources (the original is Alexis King, 2019; her follow-up
"Names are not type safety" warns that a newtype is a weaker guarantee
than a type whose *structure* forbids the bad value):

- <https://braindetox.kr/en/posts/rust_parse_dont_validate_pattern_2026.html>
- <https://docs.rs/bare-types> (parse once at the boundary, carry the proof)
- <https://eikopf.bearblog.dev/newtyped-indices-are-proofs/> (typed indices
  as proofs of membership; cites matklad's 2018 post)

→ `Span` with a checked constructor (validated once at the gesture),
`Gesture` as an enum (structure, not a name), `RowItem::Folded { … }`
instead of a string that has to be parsed back and might not parse.

## 4. Bevy ECS — change detection by ticks

- <https://taintedcoders.com/bevy/how-to/component-changed/>
- Bevy source (`bevy_ecs/src/change_detection`), issue
  <https://github.com/bevyengine/bevy/issues/1443>

Bevy records, per component, the tick at which it was last mutably
dereferenced; a system remembers the tick it last ran at and asks
"changed since my last run?". It never compares values. Mutable access
goes through a smart pointer (`Mut<T>`) that bumps the tick on
`DerefMut`. Old ticks are periodically clamped so very stale readers are
told "changed" rather than wrong answers.

→ `journal::Journaled` is the same design (global sequence, per-write
key log, `floor` for readers that fell behind, no `DerefMut` so every
write is noted). The Bevy lesson to add: a typed per-entry write guard
(`ItemEdit<'_>`, à la `Mut<T>`) so a batched edit notes its key once, on
first write, instead of every setter noting separately.

## 5. Salsa / rust-analyzer — revisions, durability, typed arenas

- Salsa algorithm: <https://git.joshthomas.dev/mirrors/salsa/raw/branch/gh-pages/reference/algorithm.html>
- rustc dev guide on Salsa: <https://rustc-dev-guide.rust-lang.org/salsa.html>
- docs: <https://docs.rs/salsa/>
- `la-arena` (rust-analyzer's typed-index arena): <https://docs.rs/la-arena>

Salsa keeps one revision counter bumped on every input change; a derived
query stores its value plus the revisions its inputs last changed at, and
re-validates by walking inputs. **Durability** groups inputs by how often
they change, so a query depending only on rarely-changing inputs is
re-validated in O(1). rust-analyzer stores syntax/HIR in arenas addressed
by `Idx<T>`, a typed `u32`.

→ The window's derived data (folds, lane layout, waveform cuts, chart)
should key on the *inputs it read*, not on a global revision. Durability
maps directly: tracks/tempo/regions change rarely ("high"), items change
every gesture ("low") — `Resync::All` vs `Resync::Items` in `studio.rs` is
a hand-rolled two-level durability; make it a type. Typed `u32` arena
indices are the right engine-internal key for items (see slotmap).

## 6. Zed / GPUI — entity handles and `update` closures

- <https://zed.dev/blog/gpui-ownership>
- Zed `.rules` (agent guidance on entities):
  <https://git.secluded.site/zed/blob/v0.201.7/.rules>

All state is owned by the `App`. An `Entity<T>` is a refcounted typed id
that cannot reach the state without the `App`; you read with
`entity.read(cx)` and mutate with `entity.update(cx, |state, cx| …)`, the
closure receiving `&mut T` plus a context for that entity. `cx.notify()`
marks it changed; observers/subscribers react. Effects are queued and
flushed after the update, which avoids re-entrancy; updating an entity
already being updated panics. `WeakEntity` returns `Result` because the
target may be gone.

→ This is the target shape for `ProjectHandle`/`ItemHandle`: the handle
is an id plus the backend, `read(|view| …)` for questions,
`edit_item(id, |item| …)` for changes, the event published after the
closure returns (as `mutate_item_evt` already does). Do not hand out
`&mut ProjectState` beyond the engine crate; hand out the closure.

## 7. Linebender — Xilem, Masonry, Vello

- <https://github.com/linebender/xilem> (README; the design post is
  "Xilem: an architecture for UI in Rust")

Xilem builds a cheap, short-lived view tree from app state each cycle,
diffs it against the previous one and applies minimal updates to a
retained widget tree (Masonry); Vello renders retained scenes. The point
for us is the split: cheap immutable description → diff → mutate retained
state only where the diff says.

→ The arrangement painter should keep a retained per-lane scene
(`vello::Scene` fragments) keyed by `Arc<Lane>` pointer identity: a lane
whose `Arc` did not change is re-appended, not re-encoded. That only
works if the mirror shares untouched lanes between frames (rewrite 4).

## 8. Data-oriented collections

- slotmap: <https://docs.rs/slotmap/latest/slotmap/>
  O(1) insert/get/remove with generational keys (stale keys stay invalid
  after reuse — no ABA), `new_key_type!` for distinct key types per map,
  `SecondaryMap` for Vec-backed per-key side data, `DenseSlotMap` for
  Vec-speed iteration. Already in `Cargo.lock` (1.1.1).
- imbl: <https://docs.rs/imbl/latest/imbl/>,
  `OrdMap` <https://docs.rs/imbl/latest/imbl/ordmap/struct.GenericOrdMap.html>,
  `HashMap` <https://docs.rs/imbl/latest/imbl/hashmap/struct.GenericHashMap.html>
  Persistent `Vector` (RRB), `HashMap` (HAMT), `OrdMap` (B+tree); clone is
  a refcount bump, writes copy one path (in place when uniquely owned).
  `OrdMap::diff` is documented to skip nodes shared by both maps;
  `HashMap` only has `symmetric_difference` (O(n log n), no sharing
  promise); both have O(1) `ptr_eq`. Costs: `OrdMap` is ~2–3× slower than
  `BTreeMap` on basic ops; `Vec` beats `Vector` below a few hundred
  elements. `im` 15.1 is already in the lockfile transitively; `imbl` is
  the maintained fork.
- `Arc<str>` vs `String` vs interning:
  <https://users.rust-lang.org/t/arc-str-is-better-than-string-was-i-using-the-wrong-string-type/99789>,
  <https://users.rust-lang.org/t/arc-string-vs-arc-str/30571>,
  RFC 1845 <https://rust-lang.github.io/rfcs/1845-shared-from-slice.html>.
  `Arc<str>` clones are an atomic increment, no allocation; it is a fat
  pointer with no extra hop. Small-string types (`smol_str`,
  `compact_str`, both already in the lockfile) avoid even the atomic for
  ≤ 23-byte strings — but a REAPER guid is 38 bytes, so `Arc<str>` (or a
  128-bit `Uuid` where the backend guarantees one) is the fit. Interning
  pays when the same strings repeat a lot; item guids do not repeat.

→ Engine-internal: slotmap keys for items/takes with a
`HashMap<ItemId, ItemKey>` at the boundary. Snapshots: `imbl::OrdMap` or
per-track `Arc<[Item]>`. Wire ids: `Arc<str>`-backed newtypes.

## 9. In-repo precedent

- `crates/frame/src/document/proto/model.rs:21-55` — `NodeId(Uuid)` and
  `AssetId(String)` with `#[facet(transparent)]`: typed ids already cross
  the Facet wire in this repo.
- `crates/daw/proto/src/handle.rs` — `DawHandle`/`ProjectHandle`/
  `TrackHandle`/`SendBuilder`: the handle layer exists for tracks; items
  and takes need the same.
- `crates/daw/proto/src/track/service.rs:39` —
  `#[architect::rpc(…, scopes(project: ProjectContext, track: TrackRef))]`
  generates leveled scope views; `Items` (`item/service.rs:29`) and
  `Takes` (`take/service.rs:13`) do not declare `scopes` yet. The macro
  (architect `macros/architect-rpc-derive/src/lib.rs`, `emit_scope_views`)
  clones the bound scope values on every call.
- `crates/daw/standalone/src/journal.rs` — `Journaled<K, V>`.
- `crates/daw/control/src/batch.rs` and `crates/daw/proto/src/batch` —
  reified ops (`ops(ProjectContext as ProjectArg, …)`) for multi-call
  programs over RPC; the remote analogue of `edit_item`.
- `.claude/skills/perfect-functions` — function-level rules this skill
  builds on.

## 10. SoundCraft (`storytold/soundcraft`) compared

Read-only shallow clone at commit HEAD of 2026-10-15. **Licence: MIT OR
Apache-2.0** (`Cargo.toml` `[workspace.package] license`, `NOTICE`), so
ideas — and code with its notice kept — are GPL-compatible to take; the
ArtCraft names/logos in `docs/brand/` are trademarks, not open. It is a
clean-room Pro Tools re-implementation: egui UI, a command engine, a
plain-data session model, ~24 k lines across `crates/{model,engine,time}`.

### How it models things

| Concern | SoundCraft | studio today |
|---|---|---|
| Ids | `TrackId(u64)`, `ClipId`, `SourceId`, … from one session-wide counter; `Copy`, `Ord`, `serde(transparent)` (`crates/model/src/ids.rs:3-22`) | `String` guids everywhere; `ItemRef::Guid(String)` |
| Document | `Session` is plain data: `tracks: Vec<Track>`, each `playlists: Vec<Playlist>` of `clips: Vec<Clip>` kept sorted (`model/src/session.rs:300-333`, `track.rs:177-181`) | `ProjectState` of guid-keyed `Journaled` maps + `items_by_track` order vecs |
| Access | `s.track(id)`, `s.track_mut(id)`, `s.find_clip_mut(id)` then assign fields (`engine/src/cmd/clip.rs:23-32`) | one RPC per field (`set_length`, `set_fade_in`, …) |
| Edits | honest free functions over `&mut Session` + typed ids: `clear_range(s, track, range, shuffle)`, `move_clips(s, ids, delta, dest)` (`engine/src/edit.rs:82-519`) | logic spread across window, engine and `Edit` messages |
| Time | `Samples` newtype, half-open `Range { start, end }` normalised on construction, `TimeCtx { tempo, sr }` bundling what MIDI splits need (`time/src/lib.rs:86-100`, `engine/src/edit.rs:20-33`) | `f64` seconds, `(f64, f64)` tuples, `PositionInSeconds`/`Duration` converted at every call |
| Undo | `Arc<Session>` copy-on-write: `session_mut()` = `Arc::make_mut`; history is a stack of `Arc<Session>`; "did anything change" is `Arc::ptr_eq(&before, &doc)` (`engine/src/lib.rs:111-182, 281, 329`) | `EditSnapshot` deep-clones the edit maps |
| Change notification | one `revision: u64` bumped on every document change; the UI re-reads when it moved (`ui-egui/src/lib.rs:1150`) | per-key journal with cursors (`journal.rs`) |
| Commands | every action is a `CommandSpec { id: &'static str, run: fn(&mut Engine, &Value) -> Result<Value>, enabled, menu, shortcut, undoable }`; JSON params; menus, CLI, MCP and the control socket all dispatch by id (`engine/src/cmd/mod.rs:46-66`) | architect actions + `Edit` enum + RPC traits |
| Errors | `EngineError` (`thiserror`), variants carry `String` context (`engine/src/lib.rs:36-52`); `#![deny(unwrap/expect/panic)]` at every crate root; `execute` catches panics and restores the document | `DawError`; `Result<_, String>` across the app |

### Adopt

1. **Copy, counter-minted ids for engine-internal identity**
   (`ids.rs`). Cheap, ordered, hashable, impossible to mix. Here the guid
   must remain the wire/file identity (REAPER), so adopt it as the
   *internal* key (or slotmap key) behind typed guid newtypes — this
   strengthens SKILL.md's "dense keys inside, guids at the boundary".
2. **Data-first edits: functions over `&mut Document` that assign
   fields.** `clear_range(s, track, range, …)` reads like the operation it
   is. Our equivalent is `edit_item(id, |item| …)` / `edit(|song| …)` —
   same DX, but through journaled access so readers still learn what
   changed.
3. **`Range`/`Samples`/`TimeCtx`.** A normalised half-open range type and
   a small `Copy` context struct for "what you need to convert time" is
   exactly what `(from, to): (f64, f64)` plus `daw, project` keeps
   re-deriving. Adopt as `Span` (seconds) and a `TimeCtx { tempo: &TempoMap }`.
4. **`Arc::ptr_eq(&before, &after)` as "nothing changed"** — a free,
   exact no-op check after a command. Adopt at the granularity of our
   persistent maps / per-track `Arc`s.
5. **Sorted per-track vectors with a documented invariant** ("sorted by
   start; non-overlapping except at crossfades", `track.rs:177-181`).
   `items_by_track` should state and keep an order (by position) so
   `items_at`/`items_in` are binary searches.
6. **One registry for every user-visible action** with id, label, menu,
   shortcut, enabled-reason and params doc, serialisable for agents. The
   repo already has architect actions; the lesson is the *enabled reason*
   (`Err(reason)` = disabled, shown to the user) and that agents/CLI and
   menus hit the same entry.

### Do not adopt

1. **Whole-document `Arc::make_mut`.** `Session.tracks` is a plain
   `Vec<Track>` of `Vec<Clip>`, so the first write after an undo point
   deep-copies every track and clip — the same O(song) cost as our
   `EditSnapshot`, just moved. At 27 000 items it would blow the 5 ms
   budget. Take the idea at a finer grain: per-track `Arc` or `imbl`.
2. **A single `revision` counter for change notification.** Coarse: the
   UI cannot tell one moved clip from a new song. Our journal is strictly
   better for the window; keep it.
3. **Linear lookups by id** (`find_clip`/`find_clip_mut` scan every
   track's playlist, `session.rs:417-422`; `clip.rating` loops call it per
   selected clip — O(k·N)). Keep an id → location index.
4. **Stringly-typed internals**: edit flags as `&str` keys
   (`SNAP_TO_GRID = "edit.snap_to_grid"`, `edit.rs:13-17`), `Track.view:
   String` (`track.rs:221`), `bool` mode parameters (`clear_range(…,
   shuffle: bool)`), and JSON `Value` params inside the engine. JSON at
   the agent/CLI boundary is right; inside Rust, typed enums and structs.
5. **`String` payloads in error variants** (`NotFound(String)`,
   `BadParams(String, String)`). Prefer structured fields
   (`NotFound { kind: Kind, id: ItemId }`) so callers can match.
6. **`catch_unwind` around every command** to restore the document. It
   fits their "never crash" rule but hides bugs; here, deny
   `unwrap`/`expect` in engine crates (as they do) and keep panics loud.

### What it changes in the recommendations

- Reinforces typed ids and a `Span` type; adds `TimeCtx` and the sorted-
  lane invariant to the target idioms.
- Confirms data-first `&mut` editing as the clean DX — but only paired
  with the journal and fine-grained sharing, never whole-document CoW.
- Suggests the enabled-reason shape for actions (`Result<(), Reason>`
  instead of `bool`), which also belongs on architect actions.

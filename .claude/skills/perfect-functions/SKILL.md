---
name: perfect-functions
description: How to shape a Rust function in this repo — honest signatures (everything it reads or writes comes through its parameters), dishonesty (globals, engine handles, clocks, I/O, tracing) pushed to the top of the call tree, empathetic parameter types, invariants in types, and a body that stays at one level of abstraction. Load before writing a new non-trivial function, before splitting or refactoring a long one (anything past ~60 lines, or with section comments, nested closures or a raw loop doing a sub-task), and when reviewing a diff for function design. Based on Logan Smith's "How to write the perfect function"; ends with a checklist to run on every function before committing.
---

# Perfect functions

Distilled from Logan Smith's talk **"How to write the perfect function"**
(<https://www.youtube.com/watch?v=2OMRWPOSw9s>), translated to Rust and to
this repo. The principles are his; the Rust mapping, the checklist and the
examples are ours. Fuller notes, with timestamps: `talk-notes.md` next to
this file.

## When to load

- Writing any function longer than a handful of lines, or any `pub` fn.
- A function has grown past ~60 lines, has `// step` style section
  comments, a closure longer than its caller, or a loop whose body is a
  sub-task.
- A function reaches for `crate::open::*`, a `static`, an atomic, the
  clock, `Uuid::new_v4`, the filesystem or the engine handle on its own.
- Reviewing a diff and asking "is this shaped right?".

Do not use it as a licence for drive-by refactors: apply it to the
function you are touching.

## Why functions exist (the talk's framing)

They add no power; they manage complexity. You get four things from a
good one: **local reasoning** (its body can be understood alone, and
nothing outside reaches in), **abstraction** (a name tag you can trust
without zooming in), **testability**, and reuse. Reuse is the least of
them — consider extracting code the *first* time you write it, not the
third.

## Principle 1 — Honest functions, dishonesty at the top

An **honest** function touches the world only through its signature: it
reads and writes nothing its caller did not hand it. In-place mutation is
fine (`sort`, `Vec::clear`) — it only touches what it was lent. A
**dishonest** function's signature is not the whole story: it reads a
global, the clock, a singleton, the disk. The talk's key point (from
~9:39): this, not pure/impure, is the axis that decides whether you can
reason about and test a function — and **dishonesty is infectious**: one
dishonest callee makes every caller dishonest. So build the logic from
honest functions and inject the dishonest parts at the highest level you
can: honest core, thin dishonest skin.

In Rust here:

- `&mut T` in a signature *is* honesty about mutation — prefer it to
  interior mutability, statics or a captured handle. `&self` methods that
  mutate through `Mutex`/`RefCell`/atomics are dishonest about it.
- Dishonest in this repo: `crate::open::local_engine()` /
  `current_song()`, `static` atomics and `OnceLock`s, `Uuid::new_v4()`,
  `Instant::now()`/`SystemTime`, `std::env`, file and network I/O, and the
  `daw` engine handle when what you need is the project data it holds.
  Take the value (or a `&mut dyn RngCore`, a `now: f64`, an id-minting
  closure) as a parameter instead; the caller at the top fetches it.
- Instead of acting on each result as it is found, **return a value that
  represents the whole result** (a `Vec<Edit>`, a plan) and let the
  dishonest caller apply it. If that is too big, take a callback
  (`impl FnMut(Item)`) or return an `impl Iterator` — the talk names the
  custom iterator as Rust's natural form of this.
- Framework hooks (Dioxus components, `#[architect::rpc]` handlers,
  action callbacks, `main`) are "backwards" functions: keep their bodies
  tiny, read the context they must, and jump straight into honest code.
- **Tracing is skin too.** Per `logging-best-practices`, enrich the span
  with `architect_telemetry::wide::set` in the handler; honest helpers
  return what happened (`Result`, a count, an outcome enum) and the top
  records it. Never `println!`/`eprintln!`/`dbg!` anywhere.
- Realtime code: `process()` is already honest by necessity (no
  allocation, locks or I/O — see the processing-core rules in CLAUDE.md).
  Keep it that way when you split it.

## Principle 2 — Empathetic signatures

The signature is read first by the person calling it. Tell them what
goes in and what comes out, and ask for exactly what you need.

- **No bare `bool`s or anonymous tuples at the call site.** `f(a, b,
  true)` and `(from, to): (f64, f64)` make the caller guess. Use an enum
  (`Fade::Keep`), a named struct, or a newtype. Clippy's
  `too_many_arguments` and architect's 4-param RPC limit point the same
  way: group parameters into a struct built for this call.
- **Do not ask for the whole wallet.** Taking `&Project` or `&Standalone`
  when you need two maps out of it hides what the function really
  touches and makes it hard to test. Take the fields
  (`&HashMap<String, Track>`, `&[Item]`).
- **Do not over-constrain.** `&[T]` not `&Vec<T>`, `&str` not `&String`,
  `impl IntoIterator<Item = &T>` when you only iterate, `impl Into<String>`
  / `AsRef<Path>` for owned-or-borrowed inputs. Take ownership only when
  you keep the value; borrow otherwise.
- **But demand what you truly need.** If B must follow A, make A return a
  token or guard that B takes (a `MutexGuard`, an `UndoBlock` RAII guard,
  a `Validated<T>`), instead of a doc comment saying "call A first". A
  begin/end pair is the same shape: one function taking a closure, or a
  guard whose `Drop` ends it, so an early `?` cannot skip the end.
- **Returns say what can happen.** `Option<T>` for "may be absent",
  `Result<T, E>` for "may fail and here is why" — never `Option` to
  swallow an error (`.ok().flatten()` turns a lock failure into "not
  found"). Prefer a typed error over `String` in library code. Mark pure
  getters and builders `#[must_use]`. Return a rich enum rather than a
  `bool` when the caller needs to know *which* no.
- **Write the function you wish existed.** Mid-thought, call
  `bounce(velocity, normal)` before it exists, finish the thought, then
  write it. A call abstracts detail in time as well as in space.

## Principle 3 — Encode invariants in types

When a parameter must satisfy a rule (finite, non-empty, `from < to`,
normalised, sorted, a lane that is not the comp lane), checking inside
creates an error case on the happy path. Make a type that can only be
built valid — a newtype with a fallible constructor (`TryFrom`, `fn
new(..) -> Option<Self>`) and cheap access to the inner value (`Deref`,
`From`, `as_*`). The caller, who knows more, validates once and the type
carries the proof everywhere after. Stop when the types start encoding
every pre/post-condition combination; the talk admits the line is a
judgement call.

## Principle 4 — One level of abstraction (the golden rule)

Every line of a body sits at the same level. Writing a function is
stacking bricks; zooming into a brick to fiddle with it is a function
call, not inline code. This covers single responsibility, "separate
producing data from acting on it", and "no raw loops".

Tells that a body mixes levels:

- section comments (`// Each where it was, taken out; then …`) — each
  section is a function waiting to be named;
- a `for` loop whose body is a sub-task (search, dedupe, copy, group) —
  use an iterator adapter (`find`, `filter_map`, `any`, `fold`,
  `partition`, `dedup_by_key`) or a named helper;
- a closure several screens long (`with_project_mut(|p| { … })`) — give
  the closure's body a name and pass `|p| copy_takes(p, …)`;
- the same "ad hoc data structure" (lower-case then binary-search;
  parallel maps kept in step) maintained by hand in several functions —
  encapsulate it in a type with methods.

Rust tools for staying flat:

- **Early returns**: `let Some(x) = … else { return … };`, `?` on
  `Option`/`Result`, guard clauses first. Two levels of nesting is a lot.
- **Small private helpers** next to their caller (free `fn`s, not methods,
  when they need no `self`) — they are cheap and are where tests go.
- Lines need not each be one call. Calling `binary_search_by` and
  checking its result is one level; writing the binary search inline is
  not.

## Naming and docs (repo conventions)

The talk only touches naming (maps as `value_by_key`, e.g.
`assets_by_name`; getters must not hide filtering). In this repo:

- `snake_case` verbs for actions (`duplicate_item`), nouns for honest
  queries (`span`, `comp_lane_of`); no `get_` prefix; `is_`/`has_` for
  `bool`s; `try_` for a fallible twin; `into_`/`as_`/`to_` per the Rust
  API guidelines.
- A name must not promise more honesty than the body has. If a "getter"
  depends on what else has happened, rename it or make it honest.
- Doc comments say what the function gives the caller and why, in plain
  sentences (see `comp.rs`); `# Errors` on every `pub` fn returning
  `Result`, `# Panics` if it can. Inline comments explain *why*, never
  restate the code — and if a comment labels a section, extract it.

## Checklist — run on every function before committing

1. **Honest?** Does it read or write anything not reachable through its
   parameters (statics, `open::*`, clock, RNG, env, disk, engine)? If so,
   can that move to the caller? Is this the top of the call tree?
2. **Testable?** Could a unit test call it with plain values and check
   the result? If not, what dishonest piece is in the way?
3. **One level?** Read the body aloud: does every line sit at the same
   zoom? Any section comments, raw loops doing a sub-task, or a closure
   bigger than a screen?
4. **Signature kind?** Borrowed where possible, slices/`&str`/`impl
   Iterator` over concrete containers, no bare `bool`/tuple parameters,
   no whole-wallet structs, at most ~4 params.
5. **Invariants?** Is any precondition re-checked inside that a type
   could carry instead? Does a begin/end or call-A-first rule rely on a
   comment rather than a guard or closure?
6. **Return?** `Option` vs `Result` vs enum chosen deliberately; no
   errors swallowed into `None`; `#[must_use]` on pure results.
7. **Flat?** Guard clauses and `let … else` up front; nesting ≤ 2.
8. **Named and documented?** The name matches what it does (including
   hidden filtering); doc comment for `pub`; comments say why.
9. **House rules:** no `println!`/`dbg!`; tracing only at the skin via
   wide events; no allocation, locks or I/O if it runs on the audio
   thread.

## Examples from this repo

Excerpts are shortened (`…`). "After" sketches show the shape, not
compiled code.

### 1. Duplicate an item — a closure that zooms into every brick

`crates/daw/standalone/src/item.rs:327` `duplicate_item` is ~115 lines,
most of it one closure passed to `with_project_mut`. At the top it works
at "mint a guid, copy the item"; at line 360 it drops into copying each
take, then into filtering envelopes by key, re-keying them, cloning audio
sources, stretch markers and eight MIDI maps — four levels in one body,
held together by long comments. Also `.ok().flatten()` turns "the project
lock failed" into "no such item".

```rust
// after (sketch)
fn duplicate_item(&self, project: ProjectContext, item: ItemRef) -> Option<String> {
    let guid = resolve_project(self, &project)?;
    let (new_guid, new_item) = self.with_project_mut(&guid, |p| duplicate_in(p, &item)).ok()??;
    publish_item_event(self, ItemEvent::Created { … });
    Some(new_guid)
}

/// Honest: everything it touches is `p`.
fn duplicate_in(p: &mut ProjectState, item: &ItemRef) -> Option<(String, Item)> {
    let src = item_guid_from_ref(p, item)?;
    let (new_guid, new_item) = copy_item(p, &src)?;
    copy_takes(p, &src, &new_guid);           // each take: copy_take(p, …)
    Some((new_guid, new_item))
}
// copy_take → copy_take_envelopes, copy_take_media, copy_take_midi
```

The "why" comments now sit on `copy_take_envelopes` and `copy_take_midi`,
where they explain one decision each. Note `Uuid::new_v4()` inside the
loop is still dishonest; `p.mint_take_guid()` (as `mint_item_guid` already
does) would make the whole thing deterministic in tests.

### 2. Patch the UI's project — honest logic wrapped in globals

`apps/session/daw/src/studio.rs:491` `patch_items` fetches the engine and
song from `crate::open`, reads and stores the `SEEN` static (line 422),
then does honest work: drop changed items from their lanes, re-insert the
current ones, re-sort, recompute `item_count` and `length_secs`. The
honest part cannot be tested without a running engine.

```rust
// after (sketch): the skin
pub fn patch_items(raw: &mut Project) -> Option<Vec<String>> {
    let (daw, song) = (crate::open::local_engine()?, crate::open::current_song()?);
    let changed = take_changes(&daw, &song)?;          // SEEN lives here only
    let project = ProjectContext::Project(song);
    Some(apply_item_changes(raw, &changed, |guid| current_item(&daw, &project, guid)))
}

/// The core: honest, unit-testable with a closure that returns fixtures.
fn apply_item_changes(
    raw: &mut Project,
    changed: &[String],
    mut now: impl FnMut(&str) -> Option<CurrentItem>,
) -> Vec<String> {
    let mut touched = remove_items(raw, changed);
    let placed = insert_items(raw, changed, &mut now, &mut touched);
    resort_lanes(raw, &touched);
    recount(raw);
    placed
}
```

This is the talk's permutations/PRNG refactor: the core takes the
dishonest lookup as an injected function.

### 3. Comp a swipe — a raw loop, a hidden invariant and a begin/end pair

`apps/session/daw/src/comp.rs:58` `swipe` takes `(from, to): (f64, f64)`
and checks `to - from > GRAIN` and finiteness on its first line (as do its
siblings); resolves targets with a `for` loop that searches, records names
and dedupes in one body (lines 71–82); then brackets the work with
`begin_undo_block` / `end_undo_block` (lines 107/124), kept paired only by
the loop's `break` never returning early.

```rust
// after (sketch)
pub struct Span { from: f64, to: f64 }       // only built finite and wider than GRAIN
impl TryFrom<(f64, f64)> for Span { … }

pub fn swipe(daw: &Standalone, project: &ProjectContext, tracks: &[String],
             lane: &str, span: Span) -> Result<Vec<String>, String> {
    let all = Tracks::all(daw, project.clone());
    let (named, targets) = resolve_targets(&all, tracks)?;
    let plans = plan_swipe(daw, project, targets, lane, &named)?;
    in_undo_block(daw, project, &format!("Comp from lane {lane}"), || {
        comp_each(daw, project, &all, plans, span)
    })
}
```

`Span` moves the validation to the caller once (the talk's normalised
vector); `in_undo_block` makes the pairing impossible to break with a `?`
(the talk's lock-guard receipt). `daw-control`'s batch already groups edits under one undo label (`crates/daw/control/src/batch.rs:83`
`with_undo`); look there before inventing a new helper.

### 4. Clear a span from an item — two booleans standing for a shape

`apps/session/daw/src/comp.rs:249` `clear` computes `left` and `right`
flags and branches on `right`, `left`, and `!right` with an `else if` at
the end, so the four cases (inside, overlaps left, overlaps right, spans
both) are spread across two `if`s and share the duplicate-or-reuse logic
inline.

```rust
// after (sketch)
enum Overlap { Covers, Head, Tail, Middle }   // what survives of the item
fn overlap(item: (f64, f64), cut: Span) -> Overlap { … }   // honest, testable

fn clear(daw: &Standalone, project: &ProjectContext, item: &Item, cut: Span)
         -> Result<(), DawError> {
    match overlap(span(item), cut) {
        Overlap::Covers => daw.delete_item(project.clone(), item_ref(item)),
        Overlap::Head   => keep_head(daw, project, item, cut.from),
        Overlap::Tail   => keep_tail(daw, project, item_ref(item), item, cut.to),
        Overlap::Middle => {
            let tail = duplicate(daw, project, item)?;
            keep_tail(daw, project, tail, item, cut.to)?;
            keep_head(daw, project, item, cut.from)
        }
    }
}
```

The decision (`overlap`) becomes an honest function you can test with
numbers; the body reads as the four cases it handles.

# `editing` — one engine for editing a recorded part

> "Instead of `quantize_drums` we select the drums and then run quantize."

Status: 2026-10-15. Steps 1–3 are done, and step 4 in part. Unbreakable is the song this
is built against: its drums must stay editable after every step.

## 0. The shape of the answer

Session edits drums well, but the engine is spread across files in
`apps/session/daw/src/`:

- the quantize engine: about 3.3k lines of `actions.rs`;
- `drum_level.rs`, `drum_edit.rs`, `groove.rs`, `written.rs`,
  `drum_reference.rs`, `drum_settings.rs` and `hits.rs`.

The engine is drums from top to bottom. Its entry points are `QUANTIZE`
and `SESSION_QUANTIZE_DRUMS`. Kit discovery hard-codes a top-level
`Drums` folder (`arrange_edit::kit_items`, `hits::kit`), and the
selection only gives it a span: the tracks are always the whole kit.

Most of the engine is not drum-specific:

- grid lines and choosing a line;
- tolerance and strength;
- turning hits and targets into cuts (`judge`);
- phase-locked splits across a group of tracks, with pre-roll,
  crossfades and gap fill (`slip_takes`, `close_seams`);
- the check after the edit;
- the groove learner, which works over `piece: usize` in beats;
- matching to a written part (`written::align`);
- riding each event toward a target (`level::events`).

What makes it drums is a short list:

- which tracks form a phase group: a kit is one group;
- what an event is: a hit marker, with its piece, ghost, bleed and flam;
- which events ride when levelling;
- the per-piece presets;
- the rules about strokes: gestures, double kick, the lead piece;
- fills, and the drum MIDI as the written part.

So the plan is one engine with **profiles**.

- **The `editing` engine** (`features/editing`) holds:
  - `Scope`: what is selected;
  - `Unit`: a group of tracks cut and levelled as one;
  - `Event`: a timed thing with a level;
  - the timing core: `quantize`, `revert` and `place` (plan, judge, cuts);
  - the dynamics core: `level` (on `level::events`);
  - the check.
- **Profiles** turn a scope into units and events, and supply the
  defaults and extra rules. `drums` comes first. Others come later, for
  example bass DI, rhythm guitars and vocals (vocals would use the
  phrase leveler).
- **Actions act on the selection.** `QUANTIZE`, `LEVEL` and `REVERT`
  take the current selection. The profile is chosen from the selected
  tracks (`dynamic_template` classification, then the Drums folder). The
  drum-named ids (`SESSION_QUANTIZE_DRUMS`, `DRUMS_LEVEL`,
  `DRUMS_REVERT`) become aliases that set the profile to drums.
- **Plan, then apply.** The engine returns a plan: cuts, moves and
  curves. Applying it goes through a sink. Today the sink is the window's
  `Editor::slip_items`, reached through the queue-and-poll round trip
  (`Action::SlipDrums`, `wait_for_slip`). The sink is what replaces that
  round trip.

## 1. Selection becomes a `Scope`

Today the window turns its selection into `from`/`to`
(`Widget::drum_span`), and every data action takes explicit arguments.
`Scope` is the published form of that selection, made the same way
`cursor::publish` makes the time selection:

```rust
pub struct Scope {
    pub tracks: Vec<String>,          // guids, folders expanded
    pub spans: Vec<(f64, f64)>,       // seconds; razor/fold rows/time selection
    pub items: Vec<String>,           // explicit items, when items were picked
    pub sections: Vec<String>,        // by name, from MCP
}
```

The window fills it from:

- the selected folded rows (`Editor::selected_folds`);
- the razor areas, with their row ranges mapped to track guids;
- the selected tracks (`Track.selected`);
- the time selection.

The MCP form (`tracks`, `from`/`to`, `sections`) builds the same
struct, so a key press and a model call take one path.

## 2. Profiles

```rust
pub trait Profile {
    fn name(&self) -> &'static str;
    /// The groups cut as one, inside the scope. For a kit, one unit
    /// holding every mic of every piece.
    fn units(&self, daw, scope) -> Vec<Unit>;
    /// The events of a unit: for drums, hit markers.
    fn events(&self, daw, unit) -> Vec<Event>;
    fn quantize_defaults(&self) -> Settings;
    fn ride(&self, part: &str) -> level::events::Ride;
    /// Extra rules: zones, fills, gestures, the written part.
    fn rules(&self) -> Rules;
}
```

`drums` wraps what exists today. The other profiles are not written
until a second material needs them; the trait is shaped to the drum
case, with nothing speculative in it.

## 3. Migration (each step builds, keeps its tests, and keeps Unbreakable editable)

1. **Done.** `level::events`: the generic event ride, with the one gain
   law. `level::drums` is now just the kit's presets plus `ride_of`.
   `envelope::phrase_ride` uses the same law. `drum_level` runs on it.
2. **Done.** **The crate, with the pure parts.** Create `features/editing`
   and move in the code that has no daw access, together with its tests:
   - `groove`, `written`;
   - `drum_edit::{grid_scan, fills, Piece}`;
   - `drum_settings::{Settings, Patch}`, without the global store;
   - from `actions.rs`: `grid_choice`, `keeps_own_line`,
     `hold_gestures_by`, `one_stroke_a_line`, `to_lines_either_side`,
     `lock_*`, `spread_between`, `uncrossed`, `shift_at`, `judge`, and
     the `SlipResult` and `SlipMove` types.

   Session re-exports them, and nothing changes in behaviour. Landed as
   `editing::{quantize, groove, written, settings, drums}`; Session keeps
   the sticky settings store (`drum_settings`) and the lanes
   (`drum_edit::{edit_lane, is_edited}`).
3. **Done.** **Scope and the selection actions.** `editing::scope`
   holds `Scope`, `Span`, folder expansion and `profile_of`. The window
   builds its scope (selected folded rows, else razor areas, else selected
   tracks over the time selection) and publishes it every frame
   (`edit_actions::publish`). `EDIT_QUANTIZE`, `EDIT_REVERT`, `EDIT_LEVEL`
   and the read-only `EDIT_SCOPE` (`apps/session/daw/src/edit_actions.rs`)
   take the call's `tracks` / `spans` / `from`/`to` / `sections`, or the
   published selection, then pick the profile and run the drum engines
   (`actions::quantize`, `actions::revert`, `drum_level::level`). Q, Alt+Q
   and Backspace on selected hits go through them, and a selection with
   no profile gets a notice in the window. Nothing selected is refused,
   never read as the whole song. Pinned by `tests/edit_selection.rs`.
   `SESSION_QUANTIZE_DRUMS` and `SESSION_REVERT_DRUMS` are the same keys,
   now on the selection; `QUANTIZE`, `DRUMS_REVERT` and `DRUMS_LEVEL`
   stay for calls that name them.
4. **In part.** **Level on the engine.** Done: levelling works on the
   selection. The selected pieces are levelled (a folder stands for what
   is under it); over spans, only the strokes in them are levelled, and
   each take's curve outside is kept (`editing::level::{owned, splice}`).
   A span owns its strokes' curve until the lead of the first stroke
   after it, so the last stroke keeps its gain through its ring. Still
   to do: `drum_level` itself (reading hit markers, choosing COMP/EDIT
   items, writing curves) moving into `editing`, generic over the
   `daw::service` traits. That waits on the lanes and `hits::kit`, which
   are still Session's.
5. **Quantize on the engine.** `detect` and `targets` move into the
   crate, generic over the `daw::service` traits. The apply step becomes
   a `Sink` trait that the window implements. The queue-and-poll path
   (`slip_now`, `wait_for_slip`) goes, and the slip and check become one
   call.
6. **A second profile**, to prove the seams. One candidate is bass DI
   timing against the kick; another is the vocal leveler as a `level`
   profile.

## 4. Open questions

- **One unit for the kit when levelling?** Today levelling works per
  piece and quantize works on the whole kit. With a selection, levelling
  only the snare means selecting the snare folder. Does that cover it?
- **Zones and regions** (`hits::zones`, `track_regions`) are kept on the
  Drums folder today. They could stay drum-only for now, or move to the
  unit's folder.

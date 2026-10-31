# Song history: ways back

Status: 2026-10-31. Built: the song's history (`SESSION_HISTORY`,
`SESSION_HISTORY_RESTORE`) and the marks set before every drum rewrite.
The automatic forks made before marks existed are pruned with
`SESSION_CHECKPOINTS_PRUNE`.

A song is one project file (`crates/daw/session-format/src/store`) holding
a Loro doc. Loro keeps every operation, and its snapshots are never
shallow, so every version the song has ever been is still in the doc,
named by its frontier. What it lacked was a way to name those versions.
Undo is the bridge's in-memory `loro::UndoManager`
(`features/session/src/sync/bridge.rs`): 200 steps, gone when the song
reopens. The file's labelled `updates` are folded away by
`Store::compact`.

## 1. The history

`apps/session/engine/src/history.rs`, plus `project_file::{history, mark,
restore}`.

- **The `history` table.** The project file has a `history` table
  (`Store::note`, `Store::history`, `Store::moment`). Every step appended
  to the file (`project_file::append_as`) adds a row: its label, the
  action that made it, the time, and the doc's frontier after it.
  Compaction never folds this table. Two steps with the same label are two
  rows with two frontiers, so they stay separate. Loro's own change
  merging doesn't matter, because the rows don't depend on it.
- **The action.** `history::ActionLayer`, a tracing layer in the app's
  subscriber (`apps/session/desktop/src/lib.rs`), reads `action.id` from
  architect's `action` span while the action runs. The step listener
  reads it on the same thread. A step made outside any action (a remote
  change, a gesture with no action) has none.
- **`SESSION_HISTORY` `{offset?, limit?}`** lists the steps and marks,
  newest first. Each has its number, kind, label, action, local time, and
  `restorable` (whether its frontier is still in this doc's history).
- **`SESSION_HISTORY_RESTORE` `{step | mark}`** forks the doc at the
  moment's frontier, reads the song from that fork, and writes it over the
  song as it is now, as one new local step (`Bridge::restore`). That step
  is "Back to …"; SESSION_UNDO takes it back. Nothing of the history is
  rewritten, and every later step can still be gone back to. It works
  after a reopen, because the rows and the whole oplog are in the file.
  Tests: `session-engine` `history::tests::*` (identical labels stay
  separate, restore after reopen, restore is undoable, a mark holds the
  song as it was); `session-format`
  `store::a_noted_step_outlives_compaction_and_opens_as_it_was`.
- **Cost.** Each step adds one small INSERT. A restore costs a `fork_at`
  of the whole doc plus one step, which is fine for a deliberate action.

## 2. Marks before drum rewrites

`apps/session/engine/src/safety_net.rs`. Before an action rewrites the
drums, it sets a **mark**: a row of kind `mark` named `Before
DRUMS_REQUANTIZE 2026-10-31 14:05` (local time; ` 2`, ` 3`, … when the
name is taken), holding the doc's frontier. That is a few bytes and no
branch. The action's result says the mark in `checkpoint`; for
`QUANTIZE`, `DRUMS_REVERT` and `DRUMS_LEVEL`, which return lists, it is on
the first entry. `SESSION_HISTORY` lists marks, and
`SESSION_HISTORY_RESTORE {mark}` goes back to one.

- **No save before a mark.** None is needed: every step is appended to
  the file as it is recorded. The mark does record, as one step named for
  the mark, whatever the engine holds that no step has recorded yet: a
  write that bypassed an undo block, or a step still in the recorder's
  queue. So the mark holds the song as it is, but costs one engine read.
- **Never on a key.** That read is a step's cost, so nothing a key or a
  drag runs over a selection sets a mark (`safety_net::on_a_key`); undo
  covers those.

| Action | When it marks |
|---|---|
| `DRUMS_REQUANTIZE`, `HITS_DETECT`, `DRUMS_REMOVE`, `DRUMS_REVERT` | always |
| `QUANTIZE`, `DRUMS_EDIT_LANES`, `HITS_RENEW_IDS`, `DRUMS_LEVEL`, `DRUMS_SAMPLES`, `DRUMS_SAMPLES_ALIGN`, `COMP_MAIN`, `COMP_SEAMS` | not a dry run |
| `DRUMS_TRIGGERS_FROM_MICS` | not a dry run, and something to make |
| `COMP_BEST` | the whole song (no `from`/`to`, `spans`, `sections`), not a dry run |
| `EDIT_QUANTIZE` (Q), `EDIT_REVERT` (Alt+Q, Backspace), `EDIT_LEVEL`, `COMP_BEST` on a stretch (Ctrl+T), `COMP_CHOOSE` (T), `COMP_KEEP`, `HITS_STRETCH` (a hit dragged) | never |

- **One mark per action the user ran.** An action that another one runs
  (`DRUMS_REQUANTIZE` runs `QUANTIZE`) uses its caller's mark, through a
  thread-local.
- **Refused, never unprotected.** If the song is not open from its project
  file, or the file refuses the write, the action is not run, and the
  error says why.

**Checkpoints are what a user makes on purpose.** `SESSION_CHECKPOINT`
forks a real branch, an alternative to edit separately. Until marks
existed, the safety net forked one automatically before each rewrite.
`SESSION_CHECKPOINTS_PRUNE` `{keep?, older_than_days?, dry_run}` cleans
those up. It only touches branches named `Before <ACTION_ID> YYYY-MM-DD
HH:MM[ n]`, never the version open or one another was forked from, and it
is not undoable.

## 3. Where the doc's history is not enough

Going back restores the **doc**, and what the doc never held cannot come
back:

- **Edits that bypass the doc.** The doc holds what `SessionModel` reads
  from the engine. A step records it when the engine finishes an undo
  block, so a write outside one (`write_project`, `set_take_markers`)
  reaches the doc only with the next recorded step. A mark records it
  first, but a step restore goes back to what the steps recorded. Engine
  state the model does not read is not in the history at all: decoded
  audio, peaks, the sticky settings kept in prefs (`HITS_SETTINGS`,
  `DRUMS_EDIT_SETTINGS`, render-ahead), the window's view. Restoring a
  step leaves those as they are.
- **A log discarded on load.** The old `.session` folder format
  (`session-format` `oplog.rs`) throws its oplog away when the manifest
  was hand-edited, and starts fresh history from the text. The first save
  moves that folder aside as `Song.session.legacy`, and the project file
  starts its history there. Nothing older can be gone back to; the history
  table starts empty.
- **A shallow snapshot.** Nothing here makes one: `Store::compact`,
  `take_snapshot` and the live-set seed (`relay::seed`) all export full
  history. If one ever did, frontiers before it would no longer be in the
  doc. `SESSION_HISTORY` would show them as `restorable: false`, and
  `SESSION_HISTORY_RESTORE` would refuse them instead of guessing.
- **Joining a live set.** `project_file::adopt` → `keep_aside` moves the
  song onto the set's doc, a new lineage. Its own version is kept as a
  branch (`Before joining …`), and its history rows move with it
  (`Store::move_history`), because their frontiers are of that lineage.
  The song's history starts again on the new doc. To go back past the
  join, open the kept branch and use its history there.
- **Media files.** Audio is files beside the project, not in the doc.
  Going back restores which file a take plays, not the file's bytes. So a
  file an edit writes must never be overwritten in place.
  `DRUMS_TRIGGERS_FROM_MICS` writes each click file under a fresh name
  (`trigger_from_mics::fresh`: `<name> (from mics).wav`, then ` 2`,
  ` 3`, …), so an older version still plays the click file it was made
  with. No other engine action writes a file the song plays:
  `DRUMS_SAMPLES` writes notes into the doc, and the other writes are
  settings, caches and streamed copies of recordings. A future writer must
  pick a fresh name the same way.
- **Other branches.** A branch lists its own steps. An alternative forked
  from the song does not list the song's steps from before the fork.
  Going back there means the song's own history.
- **Steps from before the history was kept.** Steps recorded before
  2026-10-31 are not in the table. Their versions are still in the doc,
  but have no names to list.

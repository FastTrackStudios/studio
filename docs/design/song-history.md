# Song history: ways back

Status: 2026-10-31. The checkpoints before drum rewrites and their pruning (part 1) are built. A
history of named steps (part 2) is investigated here and not built.

A song is one project file (`crates/daw/session-format/src/store`) holding
a Loro doc. Undo is the bridge's in-memory `loro::UndoManager`
(`features/session/src/sync/bridge.rs`): 200 steps, gone when the song
reopens. This note covers what lasts longer than undo.

## 1. A checkpoint before every drum rewrite

`apps/session/engine/src/safety_net.rs`. Every action that rewrites the
drums forks a branch of the song as it is before it runs, whatever its
size (`BIG_STRETCH` is 0): `Before DRUMS_REQUANTIZE 2026-10-31 14:05`
(local time). If that name is taken, ` 2`, ` 3`, … is added, because a
fork onto an existing name replaces that branch. The action's result
names the branch in its `checkpoint` field. For `QUANTIZE`,
`DRUMS_REVERT` and `DRUMS_LEVEL`, which return lists, the field is on the
first entry. The branch is listed with the song's versions
(`SESSION_VERSIONS`, `SESSION_ALTERNATIVES`) and opens like any
alternative.

| Action | When it forks |
|---|---|
| `DRUMS_REQUANTIZE`, `HITS_DETECT`, `DRUMS_REMOVE`, `DRUMS_REVERT` | always |
| `QUANTIZE`, `DRUMS_EDIT_LANES`, `HITS_RENEW_IDS`, `DRUMS_LEVEL`, `DRUMS_SAMPLES`, `DRUMS_SAMPLES_ALIGN`, `COMP_MAIN`, `COMP_SEAMS` | not a dry run |
| `DRUMS_TRIGGERS_FROM_MICS` | not a dry run, and something to make |
| `COMP_BEST` | the whole song (no `from`/`to`, `spans`, `sections`), not a dry run |
| `EDIT_QUANTIZE` (Q), `EDIT_REVERT` (Alt+Q, Backspace), `EDIT_LEVEL`, `COMP_BEST` on a stretch (Ctrl+T), `COMP_CHOOSE` (T), `COMP_KEEP`, `HITS_STRETCH` (a hit dragged) | never |

The last row is what a key or a drag runs over a selection. A checkpoint
saves the song first, and a keypress must never wait on a save; undo
covers those. `safety_net::on_a_key` marks such a run, so an action it
runs (Q's `EDIT_QUANTIZE` runs `QUANTIZE`) forks nothing either.

**Pruning.** Checkpoints pile up, so `SESSION_CHECKPOINTS_PRUNE` `{keep?,
older_than_days?, dry_run}` lets go of the automatic ones past the
newest `keep`, or older than `older_than_days`. Only names that match
`Before <ACTION_ID> YYYY-MM-DD HH:MM[ n]` count as automatic. A version a
user named, a live set's `Before joining …`, the version open and any
version another was forked from are never let go. Pruning deletes the
branch row and its own updates and snapshot, so it frees what the
checkpoint held of its own (after a live-set join, a whole snapshot).
It is not undoable.

- **One checkpoint per action the user ran.** An action that another one
  runs (`DRUMS_REQUANTIZE` runs `QUANTIZE`) uses its caller's checkpoint.
  A thread-local holds that checkpoint while the outer action runs. Sync
  actions run inline on their caller's thread.
- **Refused, never unprotected.** If the song is not open from its project
  file, or the file refuses the write, the action is not run, and the
  error says why.
- **Cheap.** `Store::fork` writes one row: the parent and the frontier
  (a few bytes). The branch loads as its parent forked at that frontier,
  so it shares all history up to there. On a copy of Unbreakable
  (266 MB), the row plus the WAL fold took 12 ms and the file did not
  grow. The checkpoint also saves the song first (`project_file::fork`),
  which costs whatever is unsaved, the same as any save. A compaction
  keeps every operation (snapshots are not shallow), so the fork point
  outlives it.
  Tests: `session-format`'s
  `store::a_checkpoint_shares_the_song_and_outlives_its_compaction`,
  `session-engine`'s `safety_net::tests::*`.
- **What makes it expensive.** Joining a live set as another lineage
  (`project_file::adopt` → `keep_aside`) rebases every other branch so it
  stands alone: its own snapshot and no fork point. After that, each
  checkpoint costs a full snapshot. Unbreakable's lab copy has six branches
  made whole this way, 5–25 MB each. Many checkpoints followed by a join
  grow the file by about one snapshot each. Deleting old checkpoints
  (`Store::delete_branch`) is the remedy. No action does that yet.

## 2. Named steps past compaction (not built)

The goal is a later `HISTORY` action that lists the song's steps by label
and time and goes back to any of them, after the session ends and after
the 200 undo steps run out.

**What is there already.** `SessionDoc::write` sets each step's label as
the Loro commit message (`features/session/src/sync/doc.rs`,
`set_next_commit_message`). Messages live in the oplog, and a non-shallow
snapshot keeps the oplog. So the labels already survive `Store::compact`,
even though the `updates` table's `label` column does not (compaction
deletes the rows). A probe showed this: four labelled commits, compacted,
reopened, then `travel_change_ancestors` from `oplog_frontiers()` gave
back `Cut`, `Quantize`, `Move`.

**What is missing.**

1. **Step boundaries.** Loro merges consecutive local changes from one
   peer within `change_merge_interval` (1000 s by default) when their
   messages are equal (`Change::can_merge_right`). The same probe returned
   two consecutive `Quantize` steps as one change of length 2. The fix is
   one line per doc: `set_change_merge_interval(-1)` where a `SessionDoc`
   is made or loaded. Every commit then stays its own change, which adds a
   little oplog metadata per step. Another option is to make each message
   unique, for example with a step counter after a separator.
2. **When.** Timestamps are recorded only with `set_record_timestamp(true)`.
   It is off, so every change says 0. Turn it on in the same place.
3. **Going back.** A step's frontier is the last op of its change:
   `ID { peer, counter: id.counter + len - 1 }`. Going back means forking a
   branch at that frontier. `Store::fork` takes the doc's current
   frontiers, so it needs a `fork_at(name, parent, &Frontiers)` sibling
   (about 10 lines). Then open that branch as an alternative. To view a
   step without forking, use `LoroDoc::checkout(&frontiers)` on a copy;
   never on the bridge's doc.
4. **Whose steps.** A synced or live-set song holds other peers' changes.
   A history should list this song's own peers and show the others as
   "from <peer>", the way undo leaves a collaborator's edits standing.
   Loading assigns a fresh peer id, so "own" means every peer this file's
   saves came from. Today nothing records those peers.

**Size of the work.** Items 1 and 2 are two lines, but they change how
every song's oplog grows from then on. Songs saved before the change keep
their merged steps. Item 3 plus a `HISTORY` / `HISTORY_OPEN` action pair
is about 150 lines with tests. Item 4 needs a decision first. Not built
here: items 1 and 2 change every song, so they need your call. The
checkpoints in part 1 already give a named way back before each rewrite.

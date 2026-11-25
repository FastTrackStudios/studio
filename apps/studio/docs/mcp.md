# Studio over MCP, for an agent

`studio-mcp` (`features/agent-mcp`) is Studio's MCP server for an agent
doing real work in the open session, such as mixing a song. It runs over
stdio, and the client starts it. It reaches the running Studio through
the app's control socket, `$XDG_RUNTIME_DIR/fts/studio.sock`:

- The socket's path is the same on every launch and only this user can open it.
- So there is no port to find and no token to copy.
- A call made while Studio restarts waits up to 15 s for it to come back.

Studio's own HTTP MCP is still there (random port, bearer token in
`~/.config/fts/studio-mcp.json`), for clients that want HTTP. Agents
should use this one.

## Connect

**In this repo**, nothing to do. `devenv.nix` writes `.mcp.json` with a
`studio` server, `scripts/studio-mcp`. That script builds the binary when
it is missing or stale, then runs it. Enter the dev shell once (direnv) so
`.mcp.json` is regenerated, then start Claude Code. `claude mcp list`
should show `studio: … ✔ Connected`.

**From any directory**, add it once at user scope:

```bash
claude mcp add --scope user studio -- /path/to/studio/scripts/studio-mcp
# or with an installed binary:
cargo install --path features/agent-mcp   # ~/.cargo/bin/studio-mcp
claude mcp add --scope user studio -- studio-mcp
```

Flags: `--socket PATH` (or `STUDIO_MCP_SOCKET`) for another socket,
`--app NAME` for another app's (`<name>.sock`), `--wait SECONDS` (20)
before a call is answered as a job, `--budget BYTES` (24000) before an
answer pages. Nothing in any config is secret.

## The tools

The curated tools are named for what they do. Each takes its action's own
argument schema, fetched from Studio. If Studio was down when the client
listed the tools, the bridge sends `tools/list_changed` once it answers.

| tool | action | what for |
|---|---|---|
| `session_tree` | SESSION_TRACKS | every track as a line: path, fader, pan, flags, FX (`under`, `view: list`/`full`) |
| `sections` | SESSION_SECTIONS | section names, usable as `from`/`to` everywhere |
| `mix_state` | MIX_STATE | a track and what is in it: fader, sends, FX, Signal chain with only changed params (`detail: params`/`full`) |
| `signal_block` | SIGNAL_CHAIN | one Signal block by track + block id, every param with its default; without `block`, the chain |
| `listen` | LISTEN | measure one point (`at`: post-fader, pre-fx, `block:<id>:pre`…) |
| `block_measure` | LISTEN ×2 | a block's input vs output in one call: `gain_db`, `crest_db`, gain reduction, `bands` curve |
| `listen_ab` | LISTEN_AB | try a change and put it back; `matched` is the level-matched difference |
| `listen_hits`, `listen_align` | LISTEN_HITS, LISTEN_ALIGN | a drum piece's averaged hit; mic alignment |
| `mix_measure` | MIX_MEASURE | the whole mix rendered, one flat row a track (`with_bands`) |
| `balance` | SESSION_TRACKS + MIX_MEASURE | every folder's LUFS against a reference (`under`, `depth`) |
| `track_set`, `fx_params`, `fx_set` | TRACK_SET, FX_PARAMS, FX_SET | faders, plain FX |
| `signal_block_set` | SIGNAL_BLOCK_SET | a block's params by id, and its bypass |
| `signal_run` | SIGNAL_RUN | Signal's own actions (add/remove/move blocks, presets) |
| `mix_snapshot`, `mix_restore`, `mix_diff` | MIX_* | keep and compare mixes |
| `drums_level`, `drums_fundamental`, `hits_list`, `hits_mark` | DRUMS_*, HITS_* | drum editing reads and marks |
| `session_mode`, `session_open`, `engine_status` | SESSION_MODE, STUDIO_SESSION_OPEN, STUDIO_ENGINE | the app |

The generic tools are `actions` (one line per action, `filter`),
`describe`, `run` (any action; known big answers are compacted the same
way), `batch` and `watch`. The bridge's own tools are `job`, `jobs`,
`job_cancel` and `more`.

`signal_block_set` needs a Studio built with SIGNAL_BLOCK_SET
(feat/signal-block-set). Until then it answers `no action
SIGNAL_BLOCK_SET`. Meanwhile, use `signal_run` with `SIGNAL_SET`
(`{block, bypass}`) or `fx_set`.

## Answers

- **Compact by default.** MIX_STATE of the Drums folder at depth 1 went
  from 128 KB, which the HTTP door cut at 100 KB, to 7.4 KB. SESSION_TRACKS
  went from 45 KB to 10.8 KB, or 6 KB with `under: "Drums"`.
  `detail: "params"` lists every parameter as a line. `detail: "full"` is
  Studio's answer as it is.
- **Pages, never cut.** An answer over the budget is kept. The bridge
  answers its first page with
  `page: {result: "r2", offset, count, total, next, more}`. Call
  `more {result, offset: next}` for the rest. The page is cut from the
  answer's longest list, so every page is valid JSON.
- **Jobs.** Every call is waited for up to `wait` seconds (20 by default).
  A call still running then, such as a whole-song MIX_MEASURE, answers
  `{job: "j1", state: "running", elapsed_s, step}` at once.
  `job {job, wait: 120}` waits again and gives the result when it is done.
  `job_cancel` stops waiting and drops the connection, but a render Studio
  already started finishes there. With a `progressToken`, the client also
  gets a progress notification every 5 s while it waits.
- **Errors say what to do.** Studio's own message comes with `takes` for
  bad arguments, plus the bridge's `did_you_mean`: the closest track paths
  for `no track X`, and the closest action ids for an unknown action.

## Workflows

- **Get your bearings:** `session_tree {under: "Drums"}`, then `sections`,
  then `mix_state {track: "Drums", depth: 1}`.
- **What a compressor does:** `block_measure {track: "Kick/Sum/In",
  block: "tone.comp", from: "CH 1", to: "CH 1 end"}` gives `gain_db`,
  `crest_db` and `dynamics[].max_gr_db`.
- **Try a change:** `listen_ab {track, from, to, change: [{action:
  "SIGNAL_BLOCK_SET", args: {track, block, params}}]}`. Read `matched`,
  the level-matched difference. The mix is put back unless `keep: true`.
- **Balance the kit:** `balance {reference: "Drums/Kick", under: "Drums",
  depth: 1, from: "CH 1", to: "CH 1 end"}`.
- **Whole-song numbers:** `mix_measure {from: "IN", to: "OUTRO end"}`,
  then `job {job}` until it is done.
- **Before a series of changes:** `mix_snapshot {name}`. Afterwards,
  `mix_diff {a: name}` shows what changed, and `mix_restore` undoes it.

Everything else (MIX_VARIANT*, SENDS_*, QUANTIZE, ZONES_*, …):
`actions {filter}`, then `describe`, then `run`.

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
| `listen` | LISTEN | measure one point (`at`: post-fader, pre-fx, `block:<id>:pre`…), or many in one render (`points: [{track, at}]`) |
| `block_measure` | LISTEN_BLOCK | a block's input vs output in one render: `gain_db`, `crest_db`, gain reduction, `bands` curve |
| `listen_ab` | LISTEN_AB | try a change and put it back; `matched` is the level-matched difference (per point with `points`) |
| `listen_render` | LISTEN_RENDER | a point's audio as a WAV, or a WAV per point of `points` (`to_file` a folder) |
| `listen_cache` | LISTEN_CACHE | what the renders keep to replay unchanged tracks; `limit_mb`, `forget` |
| `listen_hits`, `listen_align` | LISTEN_HITS, LISTEN_ALIGN | a drum piece's averaged hit; mic alignment |
| `mix_measure` | MIX_MEASURE | the whole mix rendered, a table a row a track (`master`, `exact`, `full`; `detail: params`, `with_bands`) |
| `balance` | SESSION_TRACKS + MIX_MEASURE | every folder's LUFS against a reference (`under`, `depth`) |
| `track_set`, `fx_params`, `fx_set` | TRACK_SET, FX_PARAMS, FX_SET | faders, plain FX |
| `signal_block_set` | SIGNAL_BLOCK_SET | a block's params by id, and its bypass |
| `signal_run` | SIGNAL_RUN | Signal's own actions (add/remove/move blocks, presets) |
| `mix_snapshot`, `mix_restore`, `mix_diff` | MIX_* | keep and compare mixes |
| `drums_level`, `drums_fundamental`, `hits_list`, `hits_mark` | DRUMS_*, HITS_* | drum editing reads and marks |
| `session_mode`, `session_open`, `engine_status` | SESSION_MODE, STUDIO_SESSION_OPEN, STUDIO_ENGINE | the app |

The generic tools are `actions` (one line per action, `filter`),
`describe`, `run` (any action; known big answers are compacted the same
way), `batch`, `script` and `watch`. The bridge's own tools are `job`,
`jobs`, `job_cancel` and `more`.

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

- **Many points, one render.** `listen`, `listen_ab` and `listen_render`
  take `points: [{track, at}, …]` (with `track` too, that point first).
  The points share one render and its replays, so ten mics cost about one
  listen, not ten. `listen` then answers
  `{from, to, points: [{track, at, figures, dynamics, latency}], rendered, ms}`,
  `listen_ab` `{points: [{track, at, a, b, difference, matched}], …}`,
  `listen_render` `{files: [{track, at, file, lufs, true_peak, latency}], …}`.
- **`block_measure` is one LISTEN_BLOCK.** Its answer is
  `{block, track, from, to, pre, post, difference, gain_db, crest_db,
  dynamics, rendered, ms}` (`bands: true` adds `bands`, `bands_hz`); it
  takes `solo`, `exact`, `fresh` and `png` as LISTEN does. Against a Studio
  from before LISTEN_BLOCK it listens twice, with the same answer.
- **`mix_measure` is a table.** By default it answers how it was rendered
  (`answered`, `tracks_rendered`, `tracks_replayed`, `realtime_factor`,
  `cache_mb`, `latency`, `ms`), then `columns` (`path`, `lufs`,
  `short_max`, `true_peak`, `crest`, `width`, `share_db`, and `raw_lufs`,
  `signal_gain_db` with `bypass_signal`, `latency` when one is late) and
  `rows`, one array a track, numbers to 0.1. Folders end in `/`, the
  master is `MASTER`. 66 tracks fit in about 5 KB. `detail: "params"` gives
  every number as an object a track, `detail: "full"` Studio's answer,
  `with_bands: true` a `bands` column. `master: false` leaves the master
  out and renders far less; `exact: false` renders in chunks on warm
  copies (about 0.05 LU off); `full: true` renders everything afresh.
- **Block meters on a replay.** LISTEN replays an unchanged point without
  rendering, and the engine then answers `dynamics[].blocks: null` with
  `blocks_why`. The bridge remembers the meters of every point it saw
  rendered (same track, `at`, span, `solo` and `exact`) and puts them back
  into a replay whose figures are bit-identical, marked `blocks_from`.
  A point first heard as a replay (rendered by someone else, or before the
  bridge started) still comes back null: `fresh: true` renders it. The
  real fix is the engine's: keep each Signal instance's block meters with
  the recording in the listen cache (`listen.rs`, `listen_points`: store
  `heard.meters` beside the recording under the same key and answer them
  on `Answered::Replayed`). Then this costs nothing and can go.

## Workflows

- **Get your bearings:** `session_tree {under: "Drums"}`, then `sections`,
  then `mix_state {track: "Drums", depth: 1}`.
- **What a compressor does:** `block_measure {track: "Kick/Sum/In",
  block: "tone.comp", from: "CH 1", to: "CH 1 end"}` gives `gain_db`,
  `crest_db` and `dynamics[].max_gr_db`.
- **Every mic at once:** `listen {from, to, points: [{track:
  "Kick/Sum/In", at: "post-fx"}, {track: "Snare/Top", at: "post-fx"}, …]}`.
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

## Many calls in one: `batch` and `script`

MCP has no request batching of its own. JSON-RPC batches came into the
spec in revision 2025-03-26 and were taken out again in 2025-06-18, the
revision this server answers in by default (it also accepts the
2025-03-26 and 2024-11-05 handshakes, but takes one message a line,
never a batch array). So chaining calls is a tool's job:

- **`batch`** runs a list of `{id, args}` steps in order, each its own
  undo step, nothing rolled back. Two additions on top of Studio's own
  batch:
  - `parallel: true` runs the steps side by side (8 at once), when every
    one is read-only (LISTEN, MIX_STATE, …). A mutating step makes the
    whole batch refused before anything runs.
  - A step's args can take an earlier step's answer:
    `{"$ref": "steps.0.figures.lufs"}` (steps count from 0; the path
    dots into objects and lists). A `$ref` cannot be combined with
    `parallel`.

  That is as far as `batch` goes. Anything that needs a loop, a
  condition or arithmetic is a script.
- **`script`** runs JavaScript, described below.

## Scripting

`script {source, args?, timeout?}` runs `source` as the body of an async
function in an embedded JavaScript engine, QuickJS (`rquickjs`):

- JSON is JavaScript's own data, so Studio's answers are plain objects.
- Its promises let independent renders run side by side.
- It builds from C in seconds, so the bridge still starts fast.

Luau (`mlua`, which `architect-scripting` uses for `SCRIPT_RUN` inside
Studio) was the other choice. Two things decided against it here:
`SCRIPT_RUN` runs on the app's own thread, a whole script as one undo
step, so it cannot wait on offline renders the way the bridge can; and
models write JavaScript more reliably than Luau.

The script sees:

| | |
|---|---|
| `studio.<tool>(args)` | any tool in the table above, by its name or in camelCase: `studio.listen`, `studio.blockMeasure`, `studio.listenRender`, `studio.mixMeasure`, `studio.balance`, `studio.trackSet`, `studio.signalBlockSet`, `studio.signalRun`, `studio.mixSnapshot`, `studio.mixRestore`, `studio.sessionTree`, `studio.actions`, `studio.describe`, … Its answer is the tool's own: compacted the same way (`detail` works), but whole. Paging only applies to what the script returns, and long calls are waited for inside (no `job`). |
| `studio.run(id, args)` | any action, with Studio's own answer (not compacted). |
| `studio.snapshot(name)` | MIX_SNAPSHOT `{name}`: a checkpoint to `studio.mixRestore({name})` to. |
| `studio.all([…])` | `Promise.all` over promises, or over functions that return them. Up to 8 calls run at once; more wait for a free slot. |
| `args` | the tool's `args`, any JSON. |
| `log(...)`, `console.log(...)` | lines returned with the answer (500 kept). |

A failed call throws an `Error` with `code`, `message`, `call` and
`failure` (Studio's whole failure, `did_you_mean` included). Catch it, or
the script fails with it.

The answer is `{result, log, calls, ms}`. `result` is what the script
returned, with numbers rounded to 4 places. An answer too big pages
like any other, through `result`'s longest list. An uncaught error
answers `isError` with the failure's `code` and `error`, plus:

- the script's `line` and `column`, and `source`, the text of that line;
- `thrown_by` when it was a studio call nothing caught;
- the `log` so far and the `calls` made.

The rules:

- **Each mutating call is an undo step** in the person's session, the
  same as when the tools are called one by one. A script that changes
  several things should `await studio.snapshot("before …")` first and
  say what it changed. If it fails halfway, what ran stays done:
  `mix_restore` puts the mix back.
- **The sandbox.** QuickJS has its own built-ins only. There is no module
  loader, no `std` or `os`, no `fetch` and no timers, so a script reaches
  nothing but `studio`, `log` and `args`. It may use 64 MB of memory.
  `timeout` (300 s by default, 3600 s at most) covers the whole run,
  JavaScript and calls alike. A script past its timeout is stopped, even
  inside a `while (true) {}`, and its calls still running are dropped.
- **It is a job like any call.** A script still running after its `wait`
  answers `{job, state: "running", step: "script: 12 calls, 8 running, 41 s"}`,
  and `job` waits for it. `job_cancel` stops the script and every call
  it has running.

### Examples: drums

**(a) Every Signal block's gain reduction on the kit's mics over a
section.** This is one listen a mic, all side by side. `fresh: true`
renders the blocks' meters even when a listen could be replayed. On the
Unbreakable session it read the 12 mics in 13 calls and 2.0 s; one
round trip a listen would be 13 tool calls.

```js
const tracks = (await studio.sessionTree({under: "Drums", view: "list", fields: ["path", "fx", "items"]}))
  .filter((t) => t.items > 0 && t.fx.includes("Signal Dev") && !/Trig|Sample/.test(t.path))
  .map((t) => t.path);
log(`${tracks.length} mics`);
const heard = await studio.all(tracks.map((track) =>
  studio.listen({track, from: args.from, to: args.to, at: "post-fx", fresh: true})));
// Or one render for all of them (the kit's pieces render together anyway):
//   const heard = (await studio.listen({from: args.from, to: args.to, fresh: true,
//     points: tracks.map((track) => ({track, at: "post-fx"}))})).points;
const rows = [];
heard.forEach((h, i) => {
  for (const d of h.dynamics ?? [])
    for (const b of d.blocks ?? [])
      rows.push({track: tracks[i], block: b.block, max_gr_db: b.gr?.max_gr_db, mean_gr_db: b.gr?.mean_gr_db});
});
return rows.sort((a, b) => (a.max_gr_db ?? 0) - (b.max_gr_db ?? 0));
```

With `args: {from: "CH 1", to: "CH 1 end"}`, the answer starts:

```json
{"calls": 13, "log": ["12 mics"], "ms": 2050, "result": [
  {"block": "tone.comp", "max_gr_db": -22.66, "mean_gr_db": -5.56, "track": "Drums/Cymbals/OHs"},
  {"block": "tone.comp", "max_gr_db": -9.89, "mean_gr_db": -2.73, "track": "Drums/Rooms"},
  {"block": "tone.comp", "max_gr_db": 0, "mean_gr_db": 0, "track": "Drums/Kick/Sum/In"}, …]}
```

**(b) Level-match a phase's make-up until it is within 0.1 dB.** This
makes the `tone` phase of `Kick/Sum/In` gain neutral. The phase's input
is `block:<its first block>:pre`. With `tone` as the instance's last
phase, its output is the Signal FX's output. The loop measures, moves
the make-up by the difference, and measures again, up to 6 times. Each
make-up change is an undo step, so it snapshots first.

```js
const {track, phase} = args;            // {"track": "Kick/Sum/In", "phase": "tone"}
const span = {from: "CH 1", to: "CH 1 end", exact: true};
await studio.snapshot(`before ${phase} make-up`);
const chain = (await studio.signalBlock({track})).chain;
const p = chain.phases.find((x) => x.phase === phase);
const input = (await studio.listen({track, ...span, at: `block:${p.blocks[0].id}:pre`})).figures.lufs;
let makeup = p.makeup_db;
for (let i = 0; i < 6; i++) {
  const out = (await studio.listen({track, ...span, at: "fx:Signal:out"})).figures.lufs;
  const off = input - out;
  log(`pass ${i}: make-up ${makeup.toFixed(2)} dB, out ${out.toFixed(2)} vs in ${input.toFixed(2)}`);
  if (Math.abs(off) <= 0.1) return {track, phase, makeup_db: makeup, off_db: off, passes: i};
  makeup += off;
  await studio.signalRun({track, action: "SIGNAL_SET", args: {phase, makeup_db: makeup}});
}
throw new Error(`still more than 0.1 dB off after 6 passes (make-up ${makeup} dB)`);
```

**(c) An EQ change, heard A/B.** This tries a cut on the snare top at
three frequencies. `listenAb` makes each change, listens, and puts it
back. The script keeps the cut that takes out the most energy in the
box band, at matched level.

```js
const track = "Snare/Sum/Top", span = {from: "CH 1", to: "CH 1 end"};
const tries = [];
for (const freq of [350, 450, 550]) {
  const ab = await studio.listenAb({track, ...span, change: [{action: "SIGNAL_BLOCK_SET", args: {
    track, block: "tone.eq", params: {"band.7.used": true, "band.7.shape": "Bell",
      "band.7.freq": freq, "band.7.gain": -4, "band.7.q": 2}}}]});
  // matched.bands: b − a per third octave with the loudness change taken out (centres in bands_hz).
  const box = ab.matched.bands.slice(13, 16).reduce((s, x) => s + x, 0) / 3; // 400–630 Hz
  tries.push({freq, gain_db: ab.matched.gain_db, box_db: box, crest: ab.matched.crest});
  log(`${freq} Hz: box ${box.toFixed(2)} dB, level ${ab.matched.gain_db} dB`);
}
tries.sort((a, b) => a.box_db - b.box_db);
return {best: tries[0], tries};
```

Here the EQ goes back each time. Add `keep: true` to the best one's
`listenAb`, or call `studio.signalBlockSet`, to leave it in.

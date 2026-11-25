# openDAW and LYNX SHARE, compared with ours

This study looks at two open-source DAWs and compares each with Studio and
its DAW engine (FastTrackStudios/daw plus Studio's engine):

- **openDAW** is a browser DAW. Its engine now runs as Rust in WASM inside an
  AudioWorklet.
- **LYNX SHARE** is a native C++20 / JUCE 8 DAW that hosts plugins and opens
  FL Studio and Ableton projects.

It asks what we should borrow and what we should not copy, ranked against
the costs we measured on 2026-11-25. It follows
[ardour-comparison.md](ardour-comparison.md). Where the answer is the same
as for Ardour, this study points there and does not repeat it.

| Measured on 2026-11-25 (about 140 tracks, 246 plugins, 32 cores) | |
|---|---|
| Song open | 22 s: project load 7.9 s, then 246 plugin state loads in series, 8.7 s; play waits |
| Whole-song offline render (276 s of audio) | 31–38 s in the block loop on 30 threads; 77–93 s in all (plugin copies, priming, cache recording) |
| Critical path | single plugin chains: a reverb and amp models, about 11 s each |
| Agent use | an AI agent mixes over MCP and needs hundreds of fast offline measurements |

**Sources** (all shallow clones, read only, nothing of theirs built or run):

| Code base | Revision | Location |
|---|---|---|
| openDAW | `9be7212e` (2026-11-25) | `/run/media/Development/research/openDAW` |
| LYNX SHARE | `adee2e68` (2026-11-25, one squashed commit) | `/run/media/Development/research/LYNX-SHARE` |
| daw | `e36551c2` (`.worktrees/daw-dev`) | |
| studio | `f948b4122` (`.worktrees/studio-devguide`) | |
| studio-adapters | `6ce1b1a` | `/run/media/Development/research/studio-adapters` |

The daw and studio worktrees contain uncommitted work by other agents, in
`render/{mod,node,offline}.rs`, `fx.rs`, `plugin.rs`, `ahead.rs`,
`sync/engine.rs`, `listen/render.rs` and `mix_measure.rs`. Line numbers on
our side are approximate and drift. Two pieces exist only in that
uncommitted work: the parallel state load (`set_state_chunks`) and
`PREPARE_FRAMES`. Studio pins daw at `d7262bea`, which does not contain
them.

**Path prefixes**

| Prefix | Means |
|---|---|
| `OD:` | `openDAW/` |
| `LX:` | `LYNX-SHARE/` |
| `DAW:` | `daw:crates/daw/standalone/src/` |
| `ENG:` | `studio:features/engine/engine/src/` |
| `SES:` | `studio:features/session/src/` |
| `ADP:` | `studio-adapters:features/adapters/` |

**How much to trust each claim.** Every claim about their code and ours
comes from reading the source. Claims marked *(inferred)* are deductions
that were not measured. Nothing here was benchmarked: neither project
publishes numbers we can compare with ours, apart from LYNX's own figure
for one project (§L2).

---

## Licences: what we may take

**Both are AGPL-3.0.**

| | Licence | Notes |
|---|---|---|
| openDAW | AGPL-3.0-or-later, or a commercial licence (`OD:README.md:258-271`) | Its CTAGDRC compressor port and its Matchering port are excluded from the commercial licence (`OD:README.md:140-143`) |
| LYNX SHARE | AGPL-3.0 (`LX:LICENSE`) | Built on JUCE 8 under AGPL |

**Why we must not copy their code.** GPL-3.0 §13 allows a work to be
combined with AGPL-3.0 code. The AGPL part keeps its network clause,
though. Studio serves its UI to browsers, and its engine can run behind a
share link. Combining would therefore put AGPL's source-offer duty on the
whole product. Never paste or port their code, their schemas or their test
files into our repos.

**What we may reimplement.** Ideas, algorithms, architecture and format
facts are not covered by their copyright. Each recommendation below is a
design idea, described in our own words and to be written from scratch.

**Take third-party pieces from their upstream sources, under the upstream
licence, never from these repos:**

| Piece | Licence | Where both projects use it |
|---|---|---|
| Signalsmith Stretch | MIT | both |
| CLAP headers | MIT | LYNX |
| DAWproject XSDs and the Bitwig example project | MIT | LYNX (in a test), openDAW (`test-files`) |
| libxmp-lite | MIT | LYNX |
| Basic Pitch | Apache-2.0 | both |

**Provenance warning for LYNX.** LYNX calls its FL Studio code
"clean-room". Yet `LX:src/flp/Flp.cpp` and `LX:src/engine/AudioEngine.cpp`
cite Ghidra function addresses from FL Studio's binaries (`FUN_00f46490`
and others), and `LX:docs/ANALISE_FL_STUDIO.md` and
`LX:docs/analise/REA_rodada*.md` are notes from that disassembly. If we
ever import `.flp`, learn its format from public sources (the PyFLP
documentation, for one) and not from LYNX's code or notes. Treat
`LX:docs/analise/ALS_FORMAT.md` the same way.

---

## Summary: what to borrow, ranked

The rank is the expected gain on the measured problems, divided by the
effort. Some items are already ranked in the Ardour study: keeping copies
warm, parallel copying and priming, play before plugins load, a disk cache
for track renders, and preparing plugins at load. Those stay as ranked
there; this list adds what the two new DAWs teach and says where they
confirm or change an Ardour item.

| # | Borrow | From | Gain against the measurements | Effort | Files it touches (ours) |
|---|---|---|---|---|---|
| 1 | **Lend the live plugin instances to an offline render while the transport is stopped**, one job at a time, instead of copying them. LYNX's `LiveExportHold` (`LX:src/engine/Exporter.h:60-100`): `open`, `claim` / `unclaim`, `lendInstrument`, `lendEffect`. An export borrows the playing instances, so stateful or licence-checked plugins render what was heard. For us: when the agent measures with the transport stopped (the usual case), `listen` and `MIX_RENDER` claim the live instances. They render, then clear the tails with `reset` or by rendering silence (Ardour #1's tail rule) and give the instances back. Live play during a claim either refuses the claim or falls back to copies. | Removes copying and `load_state` outright, even for a **first** render. Ardour #1 (warm copies) removes them only for repeat renders. A large part of the 40–55 s outside the block loop goes; priming stays (#5). It also removes the per-call `save_plugin_state` of every reached plugin (`ENG:listen/render.rs` `instances`, :927), which holds the live lock. | M: a claim on the live map; a "lent" state so the audio callback skips lent plugins or the claim waits for stop; reuses Ardour #1's reset work | `DAW:render/offline.rs` `copy_plugins` (:196), `copy_plugin` (:242), `Offline::adjust`; `ENG:listen/render.rs` `instances` (:927), `ready` (:1028); `ENG:mix_measure.rs` `render_cached` (:446), `render_span` (:259); `DAW:plugin.rs` `Plugins` (:1107) |
| 2 | **Start loading the song's plugin modules while the project is still loading.** LYNX's `ModuleWarmup` (`LX:src/core/ModuleWarmup.h`, `startModulesFor`) loads slow modules early, on their own thread (a Waves shell takes about 10 s), so the load is already done when the plugins are created. For us: once the session document is read (`load.doc`), collect the distinct plugin bundles and do the `dlopen` plus CLAP `clap_entry.init` / factory lookup of each on a pool while `build_on` runs. `fx::instantiate` then finds the entry already loaded. | Moves the dynamic loading and entry setup out of the serial 8.7 s and into the 7.9 s project load, which it overlaps. The 8.7 s is instantiation plus state load, about 35 ms per plugin, and the saving depends on how that splits *(inferred)*. Works with Ardour #2 and #3 and needs neither. | S | `ENG:project_file.rs` `load` (:1213), `build_on` (:1310); `DAW:fx.rs` `instantiate` (:135); `DAW:audio_engine/plugin_host.rs` `load` (:627), `load_entry` (:638) |
| 3 | **Make edits an agent sends race-safe, and let it ask what changed.** LYNX's remote-control protocol (`LX:docs/REMOTE_CONTROL.md:60-110`, `LX:src/engine/RemoteControl.cpp`): every answer carries `projectId`, a per-process `controlEpoch` and a monotonic `revision`. Every edit must carry `expectedProjectId` / `expectedEpoch` / `expectedRevision` or it is refused. `changes {fromRevision}` returns up to 128 revision changes, and a gap means "read again". One accepted request is one undo step, a no-op adds no step and does not advance the revision, and a batch of 1–128 JSON-Patch ops is one request. openDAW's scripts work the same way: a script's edits are recorded as `UpdateTask`s and replayed by `applyUpdates(updates, checksum)` as one undo step, refused when the graph's checksum is stale (`OD:packages/studio/scripting/src/ScriptHostProtocol.ts:8-10`, `ScriptWorker.ts:17`). For us: add `revision` and `epoch` to every MCP answer, an optional `expect_revision` on `track_set` / `fx_set` / `script` / `batch`, and a `changes {since}` tool fed by the song history. Make one `script` or `batch` call one undo step. | The agent stops re-reading `session_tree` / `mix_state` after each change and cannot apply a plan to a song the person has changed meanwhile. It also gives `mix_measure`'s cache a cheap "nothing changed" answer. Fewer calls per measurement round. | S–M | `studio:features/agent-mcp/src/tools.rs` (`curated` :78, `generic` :232); `ENG:actions.rs` `register_all` (:64); song history (`docs/design/song-history.md`); `ENG:listen/render.rs` keys |
| 4 | **Check periodically that the engine still matches the document.** openDAW keeps two copies of the project, the main thread's box graph and the engine's Rust mirror. A checksum round trip every second or so (`OD:packages/studio/core-wasm/src/WasmEngine.ts:107-115`, `OD:crates/boxgraph/src/checksum.rs`) catches drift and escalates when they differ. For us: a hash of the bridge's view of each track (we already compute `track_prints` and state fingerprints) against the `SessionDoc`, checked after an edit burst and before a measurement series. Report drift as a wide event (`engine.drift`). | Trust in hundreds of agent measurements: a measurement of an engine that silently dropped an edit is worse than a slow one. No speed gain. | S | `SES:sync/bridge.rs`, `SES:sync/engine.rs` `apply_parts` (:928); `DAW:render/offline.rs` `track_prints` (:414) |
| 5 | **Warm plugins up until what they report stops changing, not for a fixed time.** LYNX's `PluginWarmup` (`LX:src/core/PluginWarmup.h`, `warmup::run`) renders silent blocks that are thrown away. It stops once at least 1 s of audio and 1 s of wall time have passed and a hash of every plugin's latency and parameters (`signature`) has held still for 0.3 s, or at 10 s. It catches convolution reverbs loading their IR, samplers streaming, and bridged plugins applying state late. For us: in `Offline::prime`, compare a signature of the latencies and parameter values (we read latencies there already) and stop early once it is stable, or keep going while it moves. | Correctness: no half-loaded first second, and the PDC plan uses settled latencies. Possibly shorter primes when the plugins settle at once *(inferred)*. Mostly fixes rare wrong measurements, not the averages. | S | `DAW:render/offline.rs` `prime` (:289); `ENG:listen/render.rs` `run_chunk` (:1914) |
| 6 | **Decode compressed media in parallel at open, into a disk cache that outlives the session.** LYNX decodes every sample of a project on up to 6 threads while it parses (`LX:src/engine/AudioEngine.cpp` `prepareFlpImport`, :4724, the loop at about :5040-5130). Samples over 30 s (`streamAboveSeconds`, `LX:src/engine/SampleLibrary.h:45`) go into a memory-mapped disk cache kept between sessions, which `StreamPrefetcher` reads 4 s ahead. On native we decode compressed files whole and keep no disk cache (`DAW:source.rs`, `Memory`). | Only for songs with MP3/FLAC/OGG stems, where it cuts `load.media` and makes a reopen free. Measure `load.media` first: our test song may be all WAV (PCM is already memory-mapped). | S–M | `DAW:source.rs`, `DAW:source_cache.rs`, `DAW:materialize.rs:292-300`; `DAW:prefetch.rs` |
| 7 | **Run agent renders in a crash-isolated process, and later sandbox plugins.** LYNX has two layers. `PluginSandbox` (`LX:src/engine/PluginSandbox.h/.cpp`, `SandboxWorker.cpp`, `SandboxShared.h`) runs plugins in child `--plugin-host` processes, with audio, MIDI and parameters crossing shared memory each block; its modes follow Bitwig's (per plugin, per type, per vendor, all together). Scanning runs in a child process (`--scan-one`) and blacklists plugins that crash. openDAW gives each WCLAP instance its own WASM memory and drops a plugin that traps (`OD:packages/studio/core-wasm/src/wclap/wclap-bridge.ts:1321`). For us, first and cheap: the agent's offline renders already need no live instances once #1 is in place, so they can run in a `studio engine` child (`FTS_ENGINE=spawn`) that shares the cache on disk (Ardour #7). | A segfault in one of 246 plugins during the agent's hundredth render kills the person's open session today. No speed gain; a child process also takes the render's locks off the live process (Ardour #4). | M (render child), L (per-plugin sandbox) | `ENG:studio_engine/headless.rs`, `ENG:studio_engine/link`; `DAW:audio_engine/plugin_host.rs`; `ENG:listen/render.rs` |
| 8 | **Draw the playhead and meters on a frame clock that stops when idle, without repainting the view.** LYNX's `FrameTicker` (`LX:src/ui/FrameTicker.h`) drives the playhead and meters from vblank (capped at 170 Hz), detaches after about 0.3 s without change, and is woken by a 20 Hz poll. `MotionOverlay` (`LX:src/ui/MotionOverlay.h`) composites the playhead (DirectComposition / XRender / CoreAnimation) without repainting the arrangement. openDAW reads meters and engine state from SharedArrayBuffers on its animation frame (`OD:packages/lib/fusion/src/live-stream/README.md`, `OD:packages/lib/std/src/sync-stream.ts`), so a meter costs no message. For us: the playhead as its own layer in Blitz/Vello, and one frame clock for the views that stops when nothing moves. | UI responsiveness while the agent renders. Not one of the measured problems. | M | `features/ui/daw-ui`, the arrangement and mixer views (arrangement-view / mixer-view repos); Ardour #4 for the state side |
| 9 | **Test the importers by round trip, against fixtures we are allowed to keep.** LYNX's tests run import → export → import and compare (`LX:tests/` `als175a_export_round_trip`, `dawproject190jf_round_trip`, `dawproject190jf_zip_round_trip`, `dawproject190jf_bitwig_readme_example`, …). openDAW tests its DAWproject importer and exporter (`OD:packages/studio/core/src/dawproject/DawProjectImporter.test.ts`, `DawProjectExporter.test.ts`, `DawProjectTempo.test.ts`). For us: the same round-trip property tests in `ADP:ableton` and `ADP:interchange` (DAWproject), with fixtures from Bitwig's MIT dawproject repo and projects we export ourselves. Locally only, never committed: run our reader over `OD:test-files/*.dawproject` to find crashes (their licence for those files is unclear). | Importer fidelity. Not a measured problem. | S | `ADP:ableton` (2 fixtures today), `ADP:interchange/src/dawproject` (`io.rs`, `session.rs` `to_dawproject` :61, `from_dawproject` :269) |
| 10 | **Fuse serial chains in the scheduler.** LYNX gives an insert whose only successor has only it as input that successor too, so a chain costs no hand-off per link (`LX:src/core/RackRenderer.cpp:1603-1615`, `graphFused`). It fuses insert → insert only: fusing a channel with its insert made the graph run depth-first and doubled overruns on a test project. | Small: hand-offs, not the 11 s chains. Worth it live at small blocks *(inferred)*. Does **not** shorten the critical path (Ardour #6 stands). | S | `DAW:render/schedule.rs` `Schedule::build` (:139), `run_parallel` (:315) |

**What neither DAW solves.** Neither shortens a single long chain:

- LYNX runs a chain on one thread on purpose, at 512-frame offline blocks
  (`LX:src/engine/AudioEngine.h:1218`).
- openDAW renders everything on one thread.

Ardour #6 (larger offline blocks, time slices with pre-roll, pipelining)
is still the only plan for the 11 s floor.

**Where they confirm the Ardour list.**

| Ardour item | What the new DAWs show |
|---|---|
| #3, play before plugins load | openDAW plays a sample that has not loaded yet as silence and never waits for it (`OD:crates/engine/src/sample.rs`, Requested → Allocated → Ready; `OD:packages/studio/core-wasm/src/boot.ts:80-120`). Only its offline render waits (`queryLoadingComplete`, `OD:packages/studio/core-wasm/src/processor.ts:269`). That is the rule #3 proposes for plugin chains. LYNX, like Ardour and like us, loads plugins one at a time before play (`LX:src/engine/AudioEngine.cpp` `preloadNextPlugin`, :5156, on the message thread). |
| #8, prepare at load | LYNX's `play()` (`LX:src/engine/AudioEngine.cpp:1199`) only sets the start values and a flag: every instance is created and prepared at load. On Linux it also pre-touches each plugin's memory (`LX:src/platform/PrefaultMemory.h`, `MADV_POPULATE_WRITE`) so the first blocks do not page-fault. That is a cheap addition to #8. |
| Render ahead | LYNX's `RenderAhead` (`LX:src/core/RenderAhead.h`) is the same design as our `DAW:audio_engine/ahead.rs`: the part nobody plays live is rendered a few blocks ahead into a ring, and it is never used offline. Nothing to change. |
| Worker pool | LYNX's `WorkerPool` (`LX:src/core/WorkerPool.h/.cpp`) spins for 60 µs, then sleeps, the same as our `LanePool` (`DAW:render/parallel.rs:29`, `SPIN`). It adds `setWakeLimit` / `helpPercent`, which wake only as many workers as the measured work needs, and `park()` at the end of a block. This might lower live CPU *(inferred)*; it is not a measured problem. |
| Stems in one render | LYNX's `StemPass` taps each insert's output in one render (`LX:src/core/RackRenderer.h:664`, `RenderTaps`; `LX:src/engine/Exporter.cpp:880+`). Our probes and `record` (`DAW:render/offline.rs:445`) already do this. |

## What not to copy

| Do not copy | Where | Why |
|---|---|---|
| Undo as whole-project copies, and a full rebuild of the engine state on every edit | LYNX: `undoStack` of `Song` copies (`LX:src/engine/AudioEngine.h:1895`), `editSong` → `publishSong()` → `RackState::build` on the message thread (`LX:src/engine/AudioEngine.cpp:5393`) | It scales with the song, not with the edit. Our Loro history and the bridge's per-part apply are better. Keep them. |
| Jumping out of a crashed plugin with `siglongjmp` on the audio thread | LYNX `LX:src/platform/CrashGuard.h` | It skips destructors and can leave locks held, malloc's included (the file says so). Isolate with a process (#7), or let the crash take down only a child. |
| One audio thread for the whole engine, and all media decoded into wasm32 memory | openDAW: single-threaded by design (`OD:plans/wasm-audio/03-threading.md`), non-shared `WebAssembly.Memory` (`OD:packages/studio/core-wasm/src/engine-modules.ts:64-70`), 4 GB ceiling, an out-of-memory error still traps the engine (`OD:plans/wasm-audio/memory-ceiling.md`) | Fine for a browser sketchpad. Wrong for 140 tracks and 246 plugins. Our web path (`studio:features/engine/runtime/src/web_audio.rs`) already streams chunks (`DAW:streamed.rs`) instead of holding everything resident. Keep it that way. |
| Two hand-written copies of the timeline math | openDAW keeps the tempo, loop and interpolation math in both TS and Rust, held together by parity tests (`OD:plans/wasm-audio/04-architecture.md`) | Our one engine compiled to native and wasm avoids this. Don't grow a second copy in a view. |
| A CRDT patched up by a repair layer | openDAW: Yjs maps per box, with a deterministic `Reconcile.ts` that restores graph invariants after merges (`OD:packages/studio/core/src/ysync/`, `OD:docs/live-collab-deterministic-reconcile.md`) | Not a ban: we have the same risk with Loro. Write invariants into our document's types and validation, and keep repair as the last resort. |
| Creating plugins one at a time on the UI thread | LYNX `preloadNextPlugin` | This is our 8.7 s problem in another form. LYNX does it so a progress window can name each plugin, and its plugin creation can re-enter the message loop (issue #134). |
| Files of tens of thousands of lines, and screenshot test `.inc` files inside `src/` | LYNX `Main.cpp` (40.8k lines), `RackRenderer.cpp` (8.5k), about 80 `*Shot*.inc` in `LX:src/` | Our one-crate-per-feature rule exists to prevent this. |
| Tests that need the user's own FL Studio install to run | LYNX: `LX:tests/tests.cpp` walks the user's FL demo projects and DLLs (:1242, :2022) and skips without them | Fidelity that CI cannot check. Ship fixtures we are allowed to ship (#9). |

---

## openDAW

openDAW is the Studio app by André Michelle, a browser DAW at
`opendaw.org`. The live engine is now Rust compiled to WASM. Its old
TypeScript DSP engine is deleted, with no fallback left:
`OD:packages/studio/core/src/EngineVariant.ts` panics if no engine is
installed, and `OD:packages/studio/core-processors/src` now holds only
meters, recording and peaks. The design notes are in
`OD:plans/wasm-audio/*`, including a 400-line `diary.md`.

### O1. The audio engine

**Host.** `OD:packages/studio/core-wasm/src/processor.ts` registers
`engine-wasm-processor`. `process()` (:376) → `#render()` (:427) →
`engine.render()` (:446), once per 128-frame render quantum. The JS
analysers run after it: peaks, spectrum, goniometer, loudness (:472-479).

**Graph.** In `OD:crates/engine-env/src/engine_context.rs`:

- Nodes come from `register_processor`. Edges come from `register_edge`
  (audio) and `register_order_edge` (sidechain ordering only).
- `process()` sorts again only when `needs_sort` is set, then runs a flat
  queue of processors.
- The sort (`OD:crates/engine-env/src/topological_sort.rs`) is an iterative
  depth-first search that does not allocate.
- A cycle is refused when it is made, with `would_cycle` (:194); a feedback
  route is left silent (`audio_unit/routing.rs:163`). We instead append
  cycle members in index order (`DAW:render/graph.rs:16`).
- Delay compensation is in `engine-env/src/compensation.rs`.

**Blocks.** `RENDER_QUANTUM = 128` (`OD:crates/engine-env/src/lib.rs:10`).
Each quantum is split into pulse-range `Block`s
(`OD:crates/abi/src/lib.rs:128`), so a tempo change or a loop wrap falls
inside a sub-block. Devices then split at event offsets
(`render_instrument` / `render_effect`, `abi/src/lib.rs:1754, 1832`).
Ours renders device-sized blocks and splits at events inside nodes.
Theirs is the same idea at a fixed size.

**Threads.** One, by design. The worklet cannot wait on workers, so there
are no WASM threads (`OD:plans/wasm-audio/03-threading.md`):

- **Device modules.** Devices are position-independent WASM side modules
  linked into the engine's memory and table
  (`OD:packages/studio/core-wasm/src/device-linker.ts`), so a device call
  is WASM to WASM.
- **What still uses shared memory.** SharedArrayBuffer and Atomics carry
  the control flags, the engine state `SyncStream`, the LiveStream
  telemetry and MIDI.
- **Off the audio thread.** Peaks, OPFS, BPM detection, stretching,
  scripts and inference run in Workers.

**Instant play.** The worklet is always rendering. Play flips the
transport (`OD:packages/studio/core/src/EngineFacade.ts:133-139`,
`OD:crates/engine/src/lib.rs:2121` `Engine::play`). Devices are built when
their boxes reach the engine. Samples load on demand, and a missing sample
plays as silence. All device modules are compiled once, eagerly, at boot
(`loadEngineModules`).

**Compared with ours.** Our live engine has a dependency scheduler with a
critical-path claim order (`DAW:render/schedule.rs`), a lane pool and
render ahead. openDAW has none of these. What we can borrow is its rule
for play: never wait for a resource; render without it until it is ready
(Ardour #3).

### O2. The project model and loading

**Schema.** One schema (`OD:packages/studio/forge-boxes/src/schema/**`,
113 files) generates the TS classes and the Rust registry
(`OD:packages/lib/box-forge`: `ts-class-writer.ts`, `rust-registry.ts`):

- Fields have numeric keys.
- Pointers are typed addresses (a UUID plus a field-key path), and a rule
  says what each pointer accepts.
- Deleting a box cascades to anything that holds a mandatory pointer to it
  (`OD:docs/graph.md`).

**Serialisation.** Binary throughout:

| Level | Format |
|---|---|
| Box | "FLDS" magic, then key, length and payload per field (`OD:packages/lib/box/src/serializer.ts`) |
| Graph | `BoxGraph.toArrayBuffer` (`graph.ts:587`) |
| Project | "OPEN" magic, format version 2 (`OD:packages/studio/adapters/src/project/ProjectSkeleton.ts:36-119`) |
| Bundle | `.odb`, a zip that also holds the samples (`ProjectBundle.ts`) |

**Undo.** `BoxEditing` (`OD:packages/lib/box/src/editing.ts:63`):

- `modify()` (:243) runs a transaction, and `mark()` seals the pending
  changes into one step.
- `undo` and `redo` (:178 / :208) roll back if they fail.
- Validation runs at the end of a transaction, and an invalid transaction
  rolls back.

This is close to our song history and restore marks. Theirs also
validates graph invariants at commit, which we should do too (see the
CRDT row under *What not to copy*).

**Engine sync.** One way: UI → engine.

- `SyncSource` serialises update tasks (new / primitive / bytes / pointer /
  delete, `OD:packages/studio/core-wasm/src/sync/serialize-update-tasks.ts`).
- They travel as transferable bytes to `engine.apply_updates`
  (`processor.ts:628`), which applies them to the Rust mirror
  (`OD:crates/boxgraph`).
- The checksum check is borrow #4.

**Collaboration.**

- Projects sync with Yjs over `y-websocket` (`OD:packages/studio/core/src/ysync/`,
  server in `OD:packages/server/yjs-server/server.js`), repaired afterwards
  by `Reconcile.ts`.
- Undo stays local.
- `OD:packages/studio/p2p` is WebRTC transfer of **assets** (samples,
  soundfonts, WCLAPs), not project sync.

Our Drive / Loro model covers the same ground offline-first; see
`docs/design/sync.md`.

**Open speed.**

- `Project.load` decodes the skeleton, migrates it, and sends a full dump
  to a new worklet (`OD:packages/studio/core-wasm/src/WasmEngine.ts:96-131`).
- Samples come from an in-memory cache, then OPFS, then the network
  (`OD:packages/studio/core/src/samples/GlobalSampleLoaderManager.ts:94`).
- Peaks are generated in a worker and stored beside the audio (`:166`).
- Devices are not lazy; loading them at run time is only a plan
  (`OD:plans/loading-devices-at-runtime.md`).

openDAW publishes no timings. Its projects are small by our standard: few
tracks, stock devices only.

### O3. Offline render and export

**Same engine, faster than realtime.** Offline renders run in a separate
Worker running the same Rust engine
(`OD:packages/studio/core-wasm/src/offline-worker.ts`, driven by
`OD:packages/studio/core/src/OfflineEngineRenderer.ts`). There is no
OfflineAudioContext:

- The `step()` loop is synchronous and never yields (`offline-worker.ts:385-394`).
- Chunks of 128 × 375 frames are pulled through `QuantumStepper`
  (`OfflineEngineRenderer.ts:411`).
- The project reaches the engine through the same full dump and the same
  sample RPC as live, and the render waits until loading completes.

**Stems.** The channel count is stems × 2
(`OD:packages/studio/adapters/src/EngineProcessorAttachment.ts:59`), with
an optional metronome stem.

**Other uses.** Freeze, bounce, mixdown, video export and a benchmark page
(`OD:docs/performance.md`) all use the offline engine.

**Compared with ours.** It has one live/offline path, as ours does, but on
one thread. It has nothing like our per-track cache.

### O4. Devices and plugins

**Stock devices.** 37 crates in Rust/WASM
(`OD:crates/stock-devices/device-*`: synths, a sampler, a soundfont player,
a compressor, a convolver, a Dattorro reverb, an EQ, a vocoder, autotune,
…), listed in `DEVICES` (`engine-modules.ts:76`):

- The device ABI is in `OD:crates/abi`: `Ports::from_descriptor` is the
  only `unsafe` boundary, and `Instrument` / `AudioEffect` / `MidiEffect`
  are the traits.
- State lives in blocks the engine assigns; module statics are forbidden
  (`OD:plans/wasm-audio/05-memory.md`).

**JS inside the render.** Two bridges run JavaScript during the render:

- Script devices (`script-bridge.ts`).
- NAM, through a separate Emscripten instance (`nam-bridge.ts`), copying
  at most 128 samples per call.

**Third-party plugins: WCLAP**, CLAP compiled to wasm32 with WASI:

- The host is `OD:packages/studio/core-wasm/src/wclap/wclap-bridge.ts`
  (1431 lines); the Rust side is `OD:crates/wclap-common` and
  `device-wclap*`.
- Each instance gets its own memory (up to 1 GB), and a trap drops the
  instance. It still runs on the audio thread, so a slow plugin is not
  isolated.
- Its editor is an iframe on a separate origin.
- Its state is a blob in the box, saved with a debounce of about 100 ms.
- The plan says it is not production ready (`OD:plans/webclap.md:155-185`).

**Compared with ours.** Our processing cores already follow the same rules
(no_std, no allocation in `process`, `AudioNode: Send`; AGENTS.md
"Processing cores"). WCLAP is worth watching for our web build, which
today runs only our own blocks in the browser
(`studio:features/adapters/web/worklet`).

### O5. Import and export

**DAWproject.**

- XML classes in `OD:packages/lib/dawproject/src`; mapping in
  `OD:packages/studio/core/src/dawproject/`: `DawProject.decode/encode`,
  `DawProjectImport.read`, `DawProjectExporter.write`.
- Imported: tracks, clips, notes, warped audio, tempo and time signatures.
- **Not imported: parameter automation**
  (`DawProjectImporter.ts:359-361`, `// TODO How to get the actual
  parameter?`).
- The DAWproject EQ maps to its own Revamp EQ. Third-party plugins are not
  mapped.

**Other formats.** MIDI, WAV, Opus, ffmpeg, SoundFont 2.

**Fixtures.** `OD:test-files/` (several `.dawproject` files,
`bitwig.example.xml`, `.od` projects).

**Compared with ours.** `ADP:interchange` DAWproject reads and writes, and
carries plugin state as raw bytes (`session.rs` `to_dawproject` :61,
`from_dawproject` :269), with 21 fixtures. Nothing in openDAW is ahead of
it except its tests' shape (borrow #9).

### O6. UI rendering

**No React, no virtual DOM.** openDAW has its own JSX
(`OD:packages/lib/jsx/src/create-element.ts`), which makes real DOM nodes
that subscribe to observables and tear down with their lifecycle.

**Canvas 2D on demand.** Painting goes through `CanvasPainter`
(`OD:packages/studio/core/src/ui/canvas/painter.ts`), with one canvas per
track lane (`RegionLane.tsx:22-24`). Peaks come from mip levels
(`OD:packages/lib/fusion/src/peaks/PeaksPainter.ts`,
`peaks.nearest(unitsEachPixel)`).

**Engine → UI without messages.** The engine state arrives as a
`SyncStream` and the meters as `LiveStream`, both over SharedArrayBuffer,
read once per animation frame at about 60 fps
(`OD:packages/lib/dom/src/frames.ts`).

**Compared with ours.** Our views are Dioxus on Blitz/Vello natively and
in the web build. Borrow #8's "read telemetry from shared memory on the
frame" is the same lesson as Ardour #4. One canvas per lane, repainted
only when that lane changes, is a fair model for the arrangement's
`daw-view-kit` painting *(inferred: we did not profile our arrangement)*.

### O7. Agent-facing APIs

**Scripting.** `OD:packages/studio/scripting` (`Api.ts`, about 3000 lines)
runs user scripts in a Worker:

- A script's edits replay as one undo step, guarded by a checksum
  (borrow #3).
- `project.mixdown()` renders from a script (`Api.ts:2967-2971`,
  `OD:plans/script-mixdown.md`).

**Missing for an agent.**

- No action registry: shortcuts are per-area factories (`StudioShortcutManager.ts`).
- No headless product mode, only a Node test harness that renders `.od`
  files through the real engine
  (`OD:packages/studio/core-wasm/test/helpers/render-od.ts`).
- No MCP or LLM integration. `OD:packages/lib/inference` is ONNX models
  (stems, audio to MIDI, tempo), not language models.

**Compared with ours.** Our Action List (`ENG:actions.rs`), the MCP bridge
(`studio:features/agent-mcp`), `studio engine` headless
(`ENG:studio_engine/headless.rs`) and the `script` tool are well ahead.
Take only the checksum-guarded, one-step script.

### O8. Where openDAW is weaker than our design

- **One audio thread.** Every track, device, WCLAP and JS bridge shares
  one core; a heavy plugin stalls the mix.
- **Memory.** All media is decoded inside wasm32 memory, with no
  streaming from disk and a 4 GB ceiling. A memory grow detaches every JS
  view, a known source of bugs (`engine-modules.ts:64-69`). A shadow-stack
  overlap between devices was noted as unsolved (`05-memory.md`; it may be
  stale).
- **Plugins.** No native plugins. WCLAP is not ready, and JUCE-style
  editors need a threading model that was never built.
- **No fallback.** A browser without SIMD, cross-origin isolation or
  AudioWorklet cannot run it.
- **Two copies of the timeline math**, TS and Rust.
- **Stale docs.** The SDK README and `OD:docs/performance.md` still
  describe the TS engine.

---

## LYNX SHARE

LYNX SHARE is a native C++20 / JUCE 8 DAW, alpha, shaped like FL Studio
(channels, patterns, playlist, mixer inserts). Its own format is `.lynx`,
versioned JSON (`LX:src/engine/ProjectFile.h`). `.blx` is BandLab's
format, which it only imports. Its design notes are
`LX:docs/ARCHITECTURE.md` and `LX:docs/STATUS.md`.

### L1. The audio engine

**The real engine.** There is a `juce::AudioProcessorGraph`
(`LX:src/engine/AudioEngine.h:1467`), but it has essentially one node,
`ChannelRackProcessor`, which wraps `RackRenderer`
(`LX:src/core/RackRenderer.cpp`, 8.5k lines).

**Snapshots.** The UI edits a `Song`, builds an immutable `RackState` from
it, and hands it to the audio thread lock-free through `SnapshotExchange`
(`LX:src/core/SnapshotExchange.h`). The audio thread hands the old state
back to be freed. This is our render-snapshot builder
(`DAW:render/mod.rs:934`, `SnapshotBuilder`).

**Scheduling.** A dependency task graph:

- `RackState::buildTaskGraph` (`LX:src/core/RackRenderer.cpp:1534`)
  stores channels and mixer inserts as a DAG in CSR arrays (`graphIndegree`,
  `graphNext`, `graphRoots`).
- `RackRenderer::runTaskGraph` (:6891) runs it with atomic pending counts
  and a ready queue.
- Serial chains are fused (borrow #10).
- A cycle or a unit pinned to the audio thread falls back to layers.

Ours instead claims nodes in critical-path order from one cursor, with
workers spinning on their inputs' `done` flags
(`DAW:render/schedule.rs:139, 315`). LYNX's ready queue never spins on an
unfinished input. Ours orders by measured cost, which matters more for
our critical path.

**Threads.**

| Thread | How it works |
|---|---|
| Workers | Performance cores − 1, at most 7 (`LX:src/engine/AudioEngine.cpp:476` `workerCount`). They take tasks from an atomic ticket (`WorkerPool::parallelFor`), spin, then sleep on a semaphore, and all of it can be tuned through `LYNX_POOL` |
| Render ahead | `LX:src/core/RenderAhead.h`, on by default |
| Disk read-ahead | `StreamPrefetcher` |
| Linux scheduling | Real-time through SCHED_FIFO, then RealtimeKit, then the desktop portal (`LX:src/platform/LinuxRealtime.h`) |

Our live pool is smaller (cores/3, at most 3) but adds render ahead with
its own pool (`DAW:render/parallel.rs:259, 270`). Our offline pool is far
larger: cores − 3, at most 63, against their 7.

**Blocks.** The device buffer is whatever the user picks. `prepare(sr,
2048)` splits larger blocks, events cut chunks to the sample, and
automation runs on a 128-sample grid. Offline renders use
`renderBlockSize = 512` (`AudioEngine.h:1218`).

**PDC.** `LX:src/core/Pdc.cpp:57` `computePdc` uses FL's per-edge model:
the longest path is never delayed, and sends, sidechains and manual
latency are included. The plan reuses the previous plan's delay lines
(`RackRenderer.cpp:1677` `buildPdc(previous)`). Ours plans per block in
`pdc::plan` (`DAW:render/mod.rs:1565`); reusing delay lines across
rebuilds is worth checking on our side *(inferred)*.

**Play.** Instant, because everything is loaded first (see the table
under *Where they confirm the Ardour list*).

### L2. The project model and loading

**Model.** `Song` (`LX:src/core/Song.h`, 3.4k lines, no JUCE), with
stable IDs (`uids::normalize`).

**`.lynx` files.**

- JSON, with notes as compact arrays.
- Saved atomically (temp file + rename), then read back to verify.
- Plugin state is base64 inside the JSON.
- `.lynxpack` packs a project with its media.

**Undo.** Whole `Song` copies, named by diffing two copies
(`LX:src/core/History.h` `describeChange`). See *What not to copy*.

**Loading** (`LX:src/engine/AudioEngine.h:1125-1156`):

1. `prepareFlpImport` (`AudioEngine.cpp:4724`) parses any format and
   decodes every sample on up to 6 threads, and pre-stretches stretched
   clips in parallel.
2. `preloadNextPlugin` (:5156) creates plugins **one at a time on the
   message thread**.
3. `finishFlpImport` adopts the result as the song.

`keepRunningPlugins` (:5294) reuses running instances when the same
project is opened again, `ModuleWarmup` loads slow modules early
(borrow #2), and `LX:src/core/PluginHold.h` opens a project without its
third-party plugins.

**Speed.** The README claims FL's "Thief" demo opens in 4.3 s, against
FL's 7.1 s, on one Windows PC. Phase timings go to `LX:src/core/LoadTrace.h`.

**Compared with ours.** Our document (Loro, `SessionDoc`) with a
per-part bridge is the better model. LYNX's load is no more parallel
than ours where it counts (plugins).

### L3. Offline render and export

**Its own engine, the same code path.** `ExportJob`
(`LX:src/engine/Exporter.h/.cpp`, `renderOffline` :579) builds its own
`AudioEngine` with no device. It runs the same `RackRenderer` and task
graph at 512-frame blocks, as fast as the CPU allows, with its worker pool
below normal priority and render ahead off.

**Rendering what was heard.** `LiveExportHold` lends the live instances
to the export (borrow #1). `PluginWarmup` waits until the plugins have
settled (borrow #5).

**Checks.** `--exportparitytest` / `ExportParityTest.cpp` checks that the
live and exported output match. That is a test we should have between
live play and `listen` *(our parity is asserted by sharing the code, not
by a test)*.

**Stems.** `StemPass` renders every insert's stem from one render
(`Exporter.cpp:880+`), with per-stem latency (`planStemTap`).

**Formats.** WAV, FLAC, OGG, MP3, AIFF and M4A, several from one render.

**Compared with ours.** Our offline renders share the live renderer too
(`DAW:render/offline.rs`). We have the per-track cache, which LYNX lacks,
and far more offline threads. LYNX has no cache, so every export renders
everything.

### L4. Plugin hosting and native engines

**Formats.**

| Format | How it is hosted |
|---|---|
| VST3 | JUCE |
| CLAP | Its own `ClapPluginFormat` (`LX:src/engine/ClapFormat.cpp`, 1.9k lines) |
| VST2 | `Vst2Format.cpp` |
| JSFX | ysfx |
| LV2 | Yes |
| ARA 2 | `AraHost.cpp` |
| 32-bit on Windows x64 | Bridged by `LX:src/bridge/Bridge32Main.cpp` |

**Threads.** Plugins are created on the message thread (`PluginHost.h:666-707`).

**CLAP threading follows the spec** (`ClapFormat.cpp:272-360, 1210-1215`):

- `activate`, `deactivate` and `destroy` run on the main thread;
  `start_processing` / `stop_processing` run on the audio thread.
- `thread_check` tells the two apart.
- `request_restart` reactivates the plugin on the UI thread.
- An instance deleted on another thread is queued for teardown on the
  message thread.

Our `thread-check` answers "yes" for both main and audio
(`DAW:audio_engine/plugin_host.rs:348-361`), and `clap-main` pumps every
plugin every 50 ms (Ardour #4). LYNX's answer is the honest one. A plugin
that trusts `thread_check` may misbehave under ours *(inferred; no bug
seen)*.

**State.** Plugin state is the base64 of JUCE's `getStateInformation`.
LYNX converts to and from FL's Fruity Wrapper layout
(`stateFromFlWrapper`, `flWrapperRecord`).

**Isolation.**

- In-process: `CrashGuard.h`, which jumps out of a faulting plugin (*What
  not to copy*).
- Out of process: `PluginSandbox`, with Bitwig's modes, off by default
  (borrow #7).
- Scanning runs in a child process, incrementally, with a blacklist and
  timeouts (`LX:src/core/PluginScanPolicy.h`).

**Native stand-ins for FL Studio's plugins** (`LX:src/core/NativeEngines.cpp`,
DSP in `LX:src/core/Sytrus*`, `Harmor*`, `FlSynths`, `FlEffects`,
`FlExtraFx`):

- About 30 instruments: 3x Osc, Sytrus, Harmor, FPC, DirectWave, Slicex,
  and others.
- About 20 effects: Maximus, Gross Beat, EQUO, Vocodex, and others.
- Many start from the user's own FL install's default presets
  (`Needs::flStudio`).
- `PluginSubstitutes.cpp` / `StandIns.cpp` replace missing third-party
  plugins.
- `LX:src/plugin/` builds its own engines as VST3/CLAP/LV2 plugins from
  the same `RackRenderer`.

**Compared with ours.** Our effects (fts-fx), Signal and the sampler are
our own designs, not stand-ins. Stand-ins for missing plugins are worth
having when we import a project whose plugins aren't installed: the
DAWproject and `.als` importers carry state bytes for plugins that may
not exist here. Mapping a missing plugin onto a built-in by kind (EQ,
compressor, reverb), as LYNX's `StandIns.cpp` does, is safe to
reimplement.

### L5. Project import and export

**`.flp`** (`LX:src/flp/Flp.cpp`, no JUCE):

- An event stream; `EventReader::next` (:258) reads the size from the
  event ID's top two bits, and `0xAC` escapes to 32-bit IDs.
- FL 21+ files saved without a registration have their values XORed by
  file offset, and the reader undoes it (:281-351).
- Version branches cover FL 1–3 through 21+.
- `parseProject` (:1853) → `importFlp` (`LX:src/core/FlpImport.cpp:1042`).
  Writing is in `FlpWrite.cpp` / `FlpExport.cpp`.
- See the provenance warning before learning from it.

**`.als`** (`LX:src/core/AlsImport.cpp` `importAls` :2936):

- Its own inflate (`Gzip.cpp`, `Inflate.cpp`) and a read-only XML DOM
  (`XmlLite.h`; the README claims under a second for a 27 MB set).
- Live devices are mapped in `AlsDevices*.cpp`.
- Export (`AlsExport.cpp`) fills element skeletons (`AlsSkeleton.h`)
  generated from real Live 11/12 sets by `tools/als_skeleton.py`.
- The skeleton idea is safe to reimplement and useful for our writer:
  write a set Live accepts by filling a template extracted from a set Live
  itself saved, rather than building every element by hand.
  `ADP:ableton/write/xml_writer.rs` is worth checking against this
  *(inferred: we did not test whether Live opens our output)*.

**Other formats.**

| Format | Where | Notes |
|---|---|---|
| `.dawproject` | `LX:src/core/DawProject.cpp` (`importDawProject` :2159) | Read and written |
| `.band` | `BandFormat.cpp` / `BandImport.cpp` | GarageBand 6–11 refused |
| `.blx` | `BlxImport.cpp` | Tested on made-up files only |
| `.flm` | `FlmImport.cpp` / `FlmExport.cpp` | |
| Tracker modules | `ModuleImport.cpp` | libxmp-lite |
| MIDI, SoundFont, GIG, SFZ, Kontakt | | |

**Tests.**

- `LX:tests/tests.cpp` plus 772 `.inc` files, about 4450 cases.
- Round trips for ALS, DAWproject, FLP, FLM, BAND, BLX and trackers
  (borrow #9).
- CLI checks `--roundtrip`, `--jsonroundtrip` and `--dawprojcheck190jf`.
- A performance gate with JSON baselines (`LX:tests/perf/test_perf_gate.py`).
- The only shipped fixture is the Bitwig README project
  (`LX:tests/data/dawproject/BitwigReadmeProject.inc`). Take that file from
  Bitwig's repo, under MIT.

**Compared with ours.**

| Format | Ours | LYNX |
|---|---|---|
| Ableton | `ADP:ableton`: about 12k lines, reads and writes Live 8–12+, 2 fixtures | Reads Live sets, exports Live 11/12 from skeletons |
| DAWproject | `ADP:interchange`: reads and writes, 21 fixtures | Reads and writes |
| Logic, Pro Tools, AAF | `ADP:logic`, `ADP:protools`, AAF | None |
| FL Studio (`.flp`, `.flm`) | None | Reads and writes |

LYNX's corpus of formats is wider at FL's end, ours at the professional
end. Neither opens a DAWproject or `.als` straight into a playable song:
ours uses `.als` only to split an AbleSet setlist (`ENG:show.rs`).

### L6. UI rendering

- **Painting.** JUCE components, painted by hand with Direct2D on Windows
  (GDI as a fallback, `LX:src/ui/WindowOwner.h:155`). OpenGL only for
  visualisers. JUCE itself carries 40+ patches (`LX:cmake/LynxPatchJuce.cmake`).
- **Frame clock and playhead.** `FrameTicker` and `MotionOverlay` (borrow
  #8). About 40 fixed 30 Hz timers remain from before.
- **Meters.** They repaint only the rectangles that changed and share
  gradients so the brush cache hits (`MixerComponent.cpp:5045, 5189`).
- **Profiling.** `--uiperf`, `--paintbench` and `--paintcachetest`.

**Compared with ours.** LYNX is native widgets with a hand-tuned paint
cache. Our Blitz/Vello stack redraws through a scene graph, so the
LYNX-specific tricks (brush caches) don't transfer. The frame clock that
detaches when idle and the playhead as its own layer do transfer.

### L7. Agent-facing APIs and headless use

**CLI.** `--export`, `--render`, `--export-flp` / `-flm` / `-als`,
`--no-window`, `--selftest` and others (`LX:src/Main.cpp` around
4440-4600).

**Remote control.** `LX:docs/REMOTE_CONTROL.md`, `LX:src/engine/RemoteControl.cpp`:

- Newline JSON over loopback TCP, authenticated by an HMAC challenge, off
  by default, and written for "producer agents".
- Operations: `api.describe`, `session.inspect`, `project.read`,
  `project.edit` (JSON Patch with revision guards), `transport.*`,
  `history.*`, plus UI-driving operations in a demo mode.
- No MCP. This is borrow #3.

**Scripting.** Embedded pocketpy, sandboxed with a time limit
(`LX:src/core/Python.h`), for FL's piano-roll and Edison scripts.
`ActionSearch.h` is a command palette, not an action model.

**Server.** `LX:server/` is the online-features backend: share links and a
WebSocket relay for end-to-end-encrypted online sessions
(`LX:src/engine/LiveSessions.cpp`). It is not a render server.

**Compared with ours.** Our Action List (about 580 actions), the MCP
tools, `script` / `batch`, jobs and paging, and the headless `studio
engine` are a much richer surface. LYNX's revision guards are the piece
we lack.

### L8. Where LYNX is weaker than our design

- **Edits scale with the song.** Undo copies the whole song, and every
  edit rebuilds the whole rack state on the message thread.
- **Small offline pool, no cache.** At most 7 workers, and no per-track
  cache: every export renders all of it.
- **Plugins load one at a time on the UI thread**, re-entrantly (issue
  #134).
- **Risky crash handling.** `siglongjmp` out of plugins on the audio
  thread.
- **Format fidelity CI cannot check.** It depends on a local FL install
  and its DLLs.
- **Monolithic files**, and screenshot code in `src/`. Its history is
  squashed to a single commit.
- **Provenance risk** from code citing disassembled FL functions.

---

## Questions this study leaves open

| Question | Why it matters |
|---|---|
| How the 8.7 s splits between `dlopen` / CLAP create-init and `load_state` | Sizes borrow #2 against Ardour #2. Add spans around `fx::instantiate` and `load_live_state` |
| Whether our test song has compressed media at all | Decides whether borrow #6 is worth doing. Read `load.media` |
| What the live audio callback does with a plugin that is lent (#1) | Skip it dry, or refuse the claim while playing. Pick by what the person hears if they press play mid-measurement |
| Whether any of our plugins depends on an honest `thread_check` (§L4) | A cheap check against our 246 plugins |

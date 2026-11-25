# Ardour's engine, compared with ours

What Ardour's engine does about scheduling, disk, session load, export,
latency and plugin threads; how our engine (FastTrackStudios/daw plus
Studio's engine) does the same things; and what we should change.

The changes are ranked against four costs measured on 2026-11-25. The song
has about 140 tracks and 246 plugins, 63 of them Signal CLAP instances, on a
32-core machine.

| | Measured |
|---|---|
| Whole-song `MIX_RENDER` (276 s of audio) | 77–93 s, of which the block loop is 31–38 s on 30 threads |
| Critical path | single plugin chains: a hall reverb about 11 s, NAM amp chains about 11 s each |
| Song open | 22 s: project load 7.9 s, then 246 plugin state loads in series, 8.7 s; play waits |
| `SIGNAL_FEED` UI poll | 26–93 ms, waiting on plugin calls |

**Sources.**

- Ardour at `28bd133` (shallow clone, `/run/media/Development/research/ardour`).
- daw at `b4592ec9` (`.worktrees/daw-dev`).
- studio at `35fe9c0c6`.

**Path conventions.**

| Prefix | Means |
|---|---|
| `A:` | `ardour/libs/ardour/` |
| `DAW:` | `daw:crates/daw/standalone/src/` |
| `ENG:` | `features/engine/engine/src/` |

**How much to trust each claim.** Claims about Ardour and about our code are
read from source unless marked *(inferred)*. Claims about REAPER, Bitwig and
Cubase come from their documentation and behaviour, not their code, and are
marked *(reported)*.

---

## Summary: what to do, ranked

### First, a correctness check (small)

**A plugin is prepared for the first block it sees, and later larger blocks
fail.**

- How it happens:
  - `node.rs` prepares a plugin lazily with that block's `frames` (`DAW:audio_engine/render/node.rs:586-600`).
  - CLAP `activate` gets `max_frames_count = block_size` (`DAW:audio_engine/plugin_host.rs:1467-1471`).
  - `process_*` returns `BlockTooLarge` when a later block is larger (`plugin_host.rs:1581-1582`).
  - The node treats that error as "skip this plugin" (`node.rs:651-653`).
  - `FxParams::process` never splits a block (`render/automate.rs:54-100`).
- When it bites: render-ahead chunks grow from one device block up to 1024 frames (`ahead.rs:1408-1417`), and the run-start chunks go up to 4096 (`OWN_FRAMES`). A plugin first prepared on a 128-frame callback block would therefore go dry, silently, once chunks grow.
- I did not reproduce it; it may be that the first block is always the largest.
- How Ardour avoids it: every processor gets the engine's maximum block size up front (`Route::set_block_size`), and the size changes only through an explicit `set_block_size` that flushes plugins. A bounce does this with `bounce_chunk_size = 8192` (`A:session.cc:190, 6508`).
- **Fix:** prepare with the largest block the engine will ever pass, `max(device, CHUNK, OWN_FRAMES, offline BLOCK)`, or split a block in `FxParams::process` when it is larger than the plugin's prepared size. Count `BlockTooLarge` the way `PLUGIN_BUSY_SKIPS` counts busy skips.

### The ranked list

| # | Change | Expected gain against our measurements | Effort | Touches |
|---|---|---|---|---|
| 1 | **Keep the offline copies warm; stop making fresh copies for every render.** Reuse the copies `listen` already keeps (by state fingerprint) in `MIX_RENDER`'s `render_span` and in `exact`/`cold` renders. Clear tails by rendering silence or with a CLAP `reset()` that works, never by making a new instance. Load state only into copies whose fingerprint moved. | Most of the 40–55 s spent outside the block loop on a repeat render (copying 246 instances, loading their state, priming). A first render keeps its cost. | M (a Signal `reset` fix plus host reuse) | `ENG:mix_measure.rs:285-305, 482-483, 857-863`; `ENG:listen/render.rs` `ready` (:1028-1100), drop of cold copies (:1985-1995); `DAW:render/offline.rs` `copy_plugins` (:182-204); Signal's plugin `reset` |
| 2 | **Make copying and priming parallel, after finding what Signal instances wait on.** `COPY_THREADS = 1` because "Signal copies side by side measured no faster … (they wait on each other)". That is a shared lock or registry in Signal, not a law. Fix it, then run copy, `load_state` and `prepare` on the offline pool, one track per job. | A first render's setup (the part #1 can't remove) and song open (#3) both scale with cores. Instantiation is about 8.7 s / 246 ≈ 35 ms per plugin. On 16–29 threads that becomes ~1 s if the contention goes. | M (profiling Signal, then a pool loop) | `ENG:listen/render.rs:876-891` (`COPY_THREADS`), `side_by_side`; `DAW:render/offline.rs:275-318` (`prime`); Signal session registry / `restate` |
| 3 | **Let a song play before its plugins load.** Land the project, start transport and build FX chains in the background, track by track. Each chain is swapped in whole when it is ready, prepared (activated) on the loading thread, and announced. Until then the track plays dry or muted (a choice, default muted for tracks with instruments). Batch the `plugin_instances.write()` per chain. | Play starts after about 7.9 s, not 22 s. With #2 the plugin phase itself shrinks from 8.7 s to about 1–2 s. Ardour is as slow as we are here: it loads every plugin in series, inline in `set_state` (§3). The model is REAPER's offline and delayed FX loading *(reported)*. | M–L (bridge apply order, a "chain pending" state in the snapshot) | `features/session/src/sync/engine.rs` `apply_parts` (:927-951), `apply_chain` (:2047-2104); `ENG:project_file.rs` `build_on` (:1310-1349); `DAW:fx.rs` `instantiate`/`load_live_state` (:135-219); `DAW:plugin.rs` `Plugins::write` (:949-988) |
| 4 | **Take UI and state reads off the process lock.** `SIGNAL_FEED` calls `with_plugin_instance` and `run_action("SIGNAL_REVISIONS")` for each of 63 ports on every poll, and `clap-main`'s `pump` locks all 246 plugins every 50 ms. Each wait lasts as long as an in-flight `process`, which can be a 1024–4096-frame render-ahead chunk. Instead: Signal publishes its revisions and values into a lock-free snapshot (a seqlock or triple buffer written at the end of `process` or from its own thread); the host reads only that. `pump` visits only plugins that called `request_callback`. This is Ardour's pattern: UI writes go to `_shadow_data`, which `run()` copies in (`A:lv2_plugin.cc:3498-3503`); VST3 processing try-locks and skips (`A:vst3_plugin.cc:718`). | 26–93 ms becomes well under 1 ms. Fewer dry blocks (`PLUGIN_BUSY_SKIPS`) while the UI is open. | M (host plus a Signal API) | `ENG:clap_host.rs:95-183` (`pump`); `ENG:clap_host/signal.rs:315-414, 672-776`; `DAW:plugin_host.rs:316-318` (`request_callback` flag per plugin) |
| 5 | **Cut the cost of recording each track for the cache.** Size each probe's buffer once, for the span (known before the first block), not as a growing `Vec`. Record only tracks the cache can serve later: tracks with plugins, or with feeders that do. Don't record a track whose key already holds that span. | A few seconds of the overhead; no more reallocation during the loop. | S | `ENG:listen/render.rs:1548-1552, 2026-2053`; `Kept` (:524-641) |
| 6 | **Shorten the critical path that is one chain.** Ardour can't help here: a route is one node and its chain runs in series (§1). REAPER, Bitwig and Cubase also keep a chain on one thread *(reported)*. Three ways, cheapest first: **(a)** a larger offline block (Ardour bounces at 8192) to amortise each call's fixed cost; measure 4096. **(b)** Render a long chain whose inputs are already cached (the hall reverb reads its sends from the cache) in time slices in parallel, each with a pre-roll as long as its tail. That is the `exact: false` chunking (`CHUNKS = 3`), applied per track and only where it pays. **(c)** Offline software pipelining: plugin *i* on block *n+1* while plugin *i+1* runs block *n*, which removes the per-block barrier for long chains. | The 31–38 s loop has an ~11 s floor from single chains. (a) is a guess at 5–15 %; (b) can halve the reverb's 11 s; (c) brings a chain from Σ plugins down to max(plugin). First measure the sum over blocks of each block's critical path against the loop time (the per-node costs are already in `Schedule::build`). | (a) S, (b) M, (c) L | `ENG:mix_measure.rs:35`, `ENG:listen/render.rs:62` (`BLOCK`), `:1581-1634, 1824-1838` (chunks); `DAW:render/schedule.rs:139-375`; `DAW:render/node.rs:562-668` |
| 7 | **Keep the per-track cache on disk.** It is keyed by content already (`track_prints` plus state fingerprints plus feeder keys), so recordings can be files under the song (Ardour keeps peak files and reuses them while they are newer than their audio, `A:audiosource.cc:278-349`), and survive a reopen or engine restart. A track that no change reaches then costs nothing in the next session's first render. This is freeze or bounce without anyone having to ask for it. | Removes the first-render cost (#1's remainder) for every unchanged track after a restart or reopen. | M | `ENG:listen/render.rs` `Kept`, `own_prints`/`keys` (:762-800, 1147-1166); `DAW:render/offline.rs:398-412` |
| 8 | **Prepare plugins on the loading thread, not in the first block.** `prepare` (CLAP `activate` + `start_processing`) first runs inside a render: in the callback's run-start prime or on a render-ahead worker. Ardour configures and activates processors at session load, under the process lock (`A:session_state.cc:320-330`), so its first cycle runs DSP only. | Removes a hitch at the first play after open (246 activations inside the first blocks). Fixes the block-size problem above if done with the maximum block. | S | `DAW:render/node.rs:586-600`; the end of the bridge build (`apply_chain`) |

**What not to copy.**

- **Don't hold play for a full disk refill.** Ardour does (§2); we don't need to.
- **Don't block the engine to bounce.** `write_one_track` calls `block_processing()` (§6).
- **Don't export at the engine block size with a butler round-trip every cycle** (§4).
- **Don't wake workers with semaphores for each node** at live block sizes (§1).
- **Keep render ahead and the per-track cache.** Ardour has nothing like either (§7).

---

## 1. Process graph

### Ardour

**Nodes and edges.**

- A node is a route: track or bus, with its whole processor chain.
- `ARDOUR::topological_sort` (`A:graph_edges.cc:281`) tests every ordered pair with `Route::direct_feeds_according_to_reality` (`A:route.cc:3979`), which is O(N²). There is an edge when:
  - the other route's `all_inputs()` (main input, every IOProcessor input and every `PluginInsert` sidechain input, `A:route.cc:3938`) is fed by this route's output, or
  - an `IOProcessor` on this route (a send, a plugin sidechain IO) feeds it. `InternalSend::feeds` (`A:internal_send.cc:413`) is `_send_to == other`, **unless the send allows feedback, in which case there is no edge at all**.
- Aux buses pull: `InternalReturn::run` merges each send's buffers under a `TryLock` (`A:internal_return.cc:38`).
- Cycles are rejected. Kahn's algorithm leaves edges, so `Session::resort_routes_using` keeps the old graph and emits `FeedbackDetected` (`A:session.cc:2631-2637`).

**The chain and trigger model.**

- Setup:
  - `GraphChain` (`A:graph.cc:604`) holds each node's downstream set and its `init_refcount` (the number of direct feeders), keyed per chain. Old and new chains can therefore coexist.
  - The nodes with no inputs form the init trigger list.
- Each cycle:
  - `Graph::prep()` (`A:graph.cc:201`) resets every refcount and pushes the init triggers onto an MPMC trigger queue (pre-reserved to 1024 entries).
  - `GraphNode::run` → `process()` → `finish()` decrements each downstream refcount; a node whose count reaches zero is enqueued (`A:graphnode.cc:71-109`). Terminal nodes count down `_terminal_refcnt`.
- A graph swap is a `shared_ptr` assignment whose deleter runs on the butler. The code says "It was never rt-safe to re-chain the graph" (`A:session.cc:2669-2677`).

**Threads.**

- `how_many_dsp_threads()` (`A:utils.cc:698`) gives cores − 1 by default (config `processor-usage = -1`).
- There is one `Graph::main_thread` plus N−1 helpers, all SCHED_FIFO (`A:graph.cc:105`), with no CPU affinity.
- With one thread there is no graph at all: `process_routes` walks the sorted route list (`A:session_process.cc:240`).

**Waiting.** Ardour blocks on semaphores and has no spin option.

- `Graph::run_one` (`A:graph.cc:290-320`): a thread that pops a node posts `_execution_sem` once for each further queued node, up to the number of idle threads. A thread with nothing to do increments the idle count and `sem_wait`s.
- The backend callback thread **sleeps** for the whole graph: `_callback_start_sem.signal(); _callback_done_sem.wait();` (`A:graph.cc:451-452`).
- The graph's main thread does `prep()` and then works like a helper.
- The only spinning is a `sched_yield` loop waiting for all workers to go idle (`A:graph.cc:262`).
- A route takes its `_processor_lock` with `TryLock` and **silently skips the cycle** if it fails (`A:route.cc:4396`).

### Ours

**Nodes and edges.**

- A node is a track: media, the incoming sums, the FX chain, the fader, sends and the parent share (`DAW:render/node.rs:1-12, 245-916`).
- Edges are the parent share and sends (`DAW:render/schedule.rs:83-94`).
- A sidechain is a send with `to_sidechain`, gathered into the track's `sidechains` cell and given to each plugin (`node.rs:264-273, 614-622`).
- Order comes from Kahn's algorithm (`DAW:render/graph.rs:16-55`). Tracks in a cycle are appended in index order, not rejected.

**Scheduling** (`Schedule::build`, `schedule.rs:139-263`).

- Each node's `reach` is its own cost from the last block plus its longest downstream path. The claim order sorts by `reach`, so the critical path goes first.
- Tracks with a plugin that isn't `parallel_safe`, or whose cell lock is busy, are pinned to the render thread (`DAW:render/mod.rs:1801-1818`).
- Ardour has no equivalent. Its trigger queue is FIFO in readiness order, so a long chain can start late.

**Waiting.**

- Workers take a node from one shared atomic cursor and **spin** until its inputs are `done` (`schedule.rs:284-375`).
- The `LanePool` (`DAW:render/parallel.rs`) publishes each batch with one atomic store. Workers spin for 60 µs, then `park_timeout(100 ms)`; offline pools yield every 4096 spins.

**Thread counts** (`parallel.rs:259-293`).

| Pool | Workers |
|---|---|
| Live | cores/3, at most 3 |
| Render ahead | cores/2, at most 15 |
| Offline | cores − 3, at most 63; 29 on this machine |

The same scheduler serves live, render-ahead and offline renders (`mod.rs:1000-1008`).

### Compared

| | Ardour | Ours |
|---|---|---|
| Node | route (whole chain) | track (whole chain) |
| Ready order | FIFO trigger queue | critical path first (`reach`) |
| Wait | semaphores, a futex wake per node | spin, then park |
| Live threads | cores − 1, all on the period | 3 live + up to 15 ahead |
| Cycles | rejected, old graph kept | appended in index order |
| Feedback sends | no edge (order free) | — |
| Busy chain | route skips the cycle | plugin skipped (dry), counted |
| Single-thread path | walks the route list | same scheduler |

- **The ordering:** ours is better for offline throughput and long chains.
- **Ardour's semaphore wakes** cost a syscall per node. At a 64–128-frame period that is a real share of the deadline; that is why ours spins. Offline, spinning 30 threads on a per-block barrier wastes cores that idle. Our yield every 4096 spins is the right trade. *(inferred)*
- **Both run a chain in series.** Neither can make an 11 s chain shorter except by splitting time (#6).

## 2. Disk I/O and transport start

### Ardour

**Buffers.**

- The buffering presets overwrite the seconds settings (`A:disk_io.cc:96-148`). The default, Medium, is a **10 s** playback buffer per channel with a 256 k-sample read chunk.
- A buffer is `max(2·chunk, preset)` samples, `memset` to fault its pages in (`A:disk_reader.cc:91-114`).
- `PBD::PlaybackBuffer` keeps a reserve of 8191 samples for small backward seeks (`libs/pbd/pbd/playback_buffer.h:42, 183-187`).

**Refill.**

- `DiskReader::run` asks the butler for more once `write_space >= chunk` (`A:disk_reader.cc:541-567`).
- The butler (`A:butler.cc:187`) queues every track's `do_refill` onto the IO task list: `max(cores − 2, 2)` RT threads (`A:io_tasklist.cc:80`).
- Reads are 256 kB to 4 MB, sequential (`A:disk_reader.cc:1326-1336`).
- MIDI is entirely in RAM.

**Locate waits for the disk.**

1. `Session::locate` (RT thread, `A:session_transport.cc:166`) sets the latency pre-roll and moves the transport FSM to `WaitingForButler`.
2. The butler runs `non_realtime_locate` on every route in parallel (`:1300`).
3. That reaches `DiskReader::seek(pos, complete_refill = true)` (`A:disk_io.cc:212`, `A:disk_reader.cc:878-956`). A move within `reserved/6` is ignored. Otherwise the buffers are reset and refilled **until full**:

   ```cpp
   while ((ret = do_refill_with_alloc (false, read_reversed)) > 0) ;
   ```

4. While it is locating, the reader outputs silence (`A:disk_reader.cc:399-413`). The transport rolls only after `butler_completed_transport_work` fires `LocateDone` (`A:session_process.cc:95-99`, `A:transport_fsm.cc:395-423`).
5. A locate while rolling first declicks: the transport fades out (`DeclickToLocate`).
6. After `LocateDone`, the latency pre-roll runs (§5).

Play feels instant because the parallel reads are large and sequential, a short jump stays inside the buffer, and the declick hides the gap *(inferred)*. **Ardour does wait on locate.** It waits for 10 s of every track to be read.

### Ours

**Media.**

- WAV is memory-mapped (`PcmFile`). Compressed media is decoded whole into memory (`DAW:audio_engine/source.rs:1-33`).
- `SourceCache` loads each file once (`DAW:source_cache.rs:106-120`).
- The `fts-media-prefetch` thread touches the pages of the next 3 s past the playhead every 150 ms, even while stopped (`DAW:prefetch.rs:24-56`). The doc cites a measured 14 ms cold page fault in the callback.
- The browser path (`streamed.rs`) keeps about 9 s ahead in 16 k-frame chunks, read by a `stream-butler`; a missing chunk reads as silence.
- **Play waits on nothing in media.**

**Run start.** At play or locate the callback renders the ahead tracks itself until it is 3 blocks ahead, or for at most 64 blocks (`DAW:ahead.rs:407-416, 1005-1054`).

### What it means for us

Keep ours. Memory mapping plus prefetch gives Ardour's result (no I/O in the callback after a short jump) without a 10 s refill per track. Two things to borrow:

- **Pre-roll for latency.** Ardour runs `_remaining_latency_preroll` after a locate, so latent plugins' outputs are aligned from the first audible sample. Our plan already shifts items by `plan.input` and drops `latency` samples offline. Check that a live locate doesn't play the first `master` latency of compensated output as silence or garbage *(not verified)*.
- **A declick on locate.** Ardour fades the transport out and in. Render ahead crossfades editions (`ahead.rs:15-33, 1291-1300`). Check that a locate while rolling fades too.

What makes our play wait is **plugins, not media**: song open (§3) and a first `prepare` inside the first blocks (ranked #8).

## 3. Session load

### Ardour

**Threads and order.**

- The session is built on the thread that constructs it: the GUI thread in `ardour_ui_session.cc:417`, or the main thread of the headless tools.
- `post_engine_init` starts the butler, then runs `set_state` **synchronously** (`A:session_state.cc:224-300`).
- The order: tempo map, sources, locations, regions, playlists, bundles and VCAs, then **routes** (`load_routes`, `A:session_state.cc:2840`), one XML child after another. For each route: `new AudioTrack/MidiTrack` → `init` → `set_state`, then all routes are added at once.

**Plugins.**

- `PluginInsert::set_state` (`A:plugin_insert.cc:2727`) finds and instantiates the plugin inline, creates its parameters, and calls the plugin's own `set_state`.
- LV2 runs its worker synchronously during the restore (`A:lv2_plugin.cc:1036-1045`). VST3 masks `restartComponent` while loading (`A:vst3_plugin.cc:1643-1648`).
- **Nothing is parallel or deferred.**

**After the routes.**

- Every route's processors are reconfigured under the process lock (`A:session_state.cc:320-330`), so the plugins are active before the first cycle.
- Then `hookup_io`, `initialize_latencies`, and "Filling playback buffers", a `request_locate` (`:415-419`).

**What is lazy.**

- Refills are skipped while `loading()`.
- Missing peaks are built on 2 `PeakFileBuilder` threads (`A:source_factory.cc:114-122`; deferred at `A:session_state.cc:3569`).
- Peak files are reused when they are newer than the audio.

A large Ardour session with hundreds of plugins opens as slowly as ours, for the same reason: one thread, one plugin after another. The code says nothing about it.

**Other DAWs** *(reported)*:

- REAPER can open a project with FX offline, or load FX lazily.
- Bitwig hosts plugins in separate processes, so a slow plugin doesn't hold the engine thread.

### Ours

**The open sequence.** `StudioSession::open_first_as` (`ENG:song_open.rs:69-127`) runs `open.engine` → `open.prepare` → `open.read`. Inside it:

1. `project_file::load` (`ENG:project_file.rs:1213-1304`): store → `SessionDoc::from_loro` → `build_on` (the `song.build` span, `:1310-1349`) → land.
2. `build_on` runs on **one** `song-build` thread with a `current_thread` runtime, against a `Stage` engine that shares `plugin_instances`.
3. The FX arrive through `Bridge::join`:
   - `apply_parts` is a sequential `for` over tracks (`features/session/src/sync/engine.rs:927-951`).
   - It awaits `apply_chain`, a sequential loop over FX (`:2047-2104`).
   - For each FX: `add_at` → `fx::instantiate` (CLAP create/init) → `plugin_instances.write()` → `set_state_chunk` → `load_live_state` → `load_state` + `restart_if_requested` (`DAW:fx.rs:135-219`).
4. `Plugins::write` polls `try_write` with 50 µs sleeps and waits out any render holding a read lock for a block (`DAW:plugin.rs:949-988`).

**What already runs in parallel:**

- audio attach, on its own `audio-attach` thread;
- media prefetch;
- the snapshot builder;
- separate songs, each on its own stage.

**Where the 22 s go:**

| Time | Where |
|---|---|
| 8.7 s | the 246 iterations of `apply_chain` |
| 7.9 s | `load.store`, `load.doc`, the non-FX part of `load.build`, and `load.media` |

The span fields `load.store_ms`, `load.doc_ms`, `load.build_ms`, `load.land_ms` and `load.media_ms` already split the 7.9 s; read them before working on it. Once loaded, the first `prepare` of each plugin happens inside the first rendered blocks (`node.rs:586-600`).

### Compared

We are where Ardour is: serial and inline. The gains are beyond Ardour:

- parallel instantiation once Signal's contention is found (#2);
- play before the chains are in (#3);
- prepare on the loading thread (#8).

The stage design already makes #3 natural. Tracks can land before their chains, and a chain can be swapped in whole with one `plugin_instances.write()` per chain instead of one per plugin.

## 4. Export and freewheel

### Ardour

**The same graph.**

- `AudioEngine::process_callback` calls `Freewheel(nframes)` instead of `Session::process` while freewheeling (`A:audioengine.cc:542`).
- `process_export` → `process_without_events` → `process_routes`: the same `GraphChain`, the same thread pool.
- The backend loops without the device; the ALSA backend's "Freewheelin'" branch zeroes inputs.
- **The block size stays the engine period.** `ExportGraphBuilder::process` asserts it.

**Its limits:**

- **The butler is synchronised every cycle.** `_butler->wait_until_finished()`, "since we're running faster than realtime" (`A:session_export.cc:265-270`).
- **Each timespan is its own freewheel.** Stop, wait for the latency callbacks, restart (`A:export_handler.cc:233-240`).
- **Start-up.** A full refill on the export thread (`A:session_export.cc:165`), then `export-preroll` (default **2 s**) of `fail_roll` "to clear out buffers (reverb tails etc)" (`:317-332`), then the latency pre-roll (`ExportGraphBuilder::process`, `A:export_graph_builder.cc:170-200`), then a post-roll of `worst_latency_preroll()` "to feed and flush latent plugins" (`A:export_handler.cc:363-369`).
- **Latency is frozen** during export (`A:session.cc:7474`).
- **Plugins are told.** VST3 gets `kOffline` (`A:vst3_plugin.cc:1902`). LV2 gets the `freeWheeling` port and a synchronous worker (`A:lv2_plugin.cc:2902, 3508`).
- **Normalisation is two passes.** Pass 1 writes a float temp file while peak and loudness readers run. Pass 2 reads it back about 4086 samples per freewheel cycle through Normalizer → Limiter → Analyser → Encoder, on a `Threader` pool (`A:export_graph_builder.cc:782-953`).
- **Several formats share one pass.** Every format of one timespan is fed from it.
- **No copies.** Export plays the live session's own plugin instances; nothing else runs meanwhile.

### Ours

- **The same scheduler**, on an offline pool of 29 workers plus the caller, with up to 2 renders at once (`DAW:render/parallel.rs:284-339`).
- **A 1024-frame block** (`ENG:mix_measure.rs:35`, `ENG:listen/render.rs:62`) and a 1 s pre-roll (`PREROLL`).
- **Media is read in memory, with no butler** to wait on.
- **Separate plugin instances.** Offline renders run their own copies so the live session keeps playing:
  - `render_span` saves each live instance's state (taking its lock, which renders that plugin dry for a block during playback, `DAW:render/offline.rs:179-181`), then makes a fresh instance and loads the state into it, one at a time (`ENG:mix_measure.rs:285-305`, `DAW:render/offline.rs:228-252`).
  - The comment says why: "Fresh copies every render: a plugin's `reset` is not trusted" (Signal keeps reverb tails).
  - `listen`'s `ready()` reuses warm copies by fingerprint, but `MIX_RENDER` defaults `exact = cold = true` (`ENG:mix_measure.rs:857-863`), so it never does, and cold copies are dropped after the render (`ENG:listen/render.rs:1985-1995`).
- **Priming.** `prime` (`DAW:render/offline.rs:275-318`) is a serial loop of `prepare` (CLAP `activate` + `start_processing`) plus a latency read for every plugin.
- **Recording.** Every rendered track gets a post-fader probe appended to a growing `Vec` each block (`ENG:listen/render.rs:1548-1552, 2026-2053`).

### Compared

The block loop is not the problem: 31–38 s for 276 s of audio is 7–9× realtime, which Ardour's period-sized, butler-synchronised freewheel won't beat *(inferred)*. The 40–55 s around the loop are ours alone, because Ardour makes no copies.

**What Ardour shows is that tails can be cleared without a new instance.** Its 2 s `fail_roll` exists for exactly that, though it takes 2 s of render per export. We have three options, cheapest first:

1. Signal implements CLAP `reset()` properly (it clears delay lines and reverb state).
2. Render silence of the instance's tail length before the pre-roll, Ardour's way.
3. Keep copies warm and track whether they are dirty: a copy that rendered span X is dirty, so give it `reset`; a copy whose state fingerprint moved gets `load_state`.

This is ranked #1, with #2 for the remaining first-render setup.

## 5. Latency compensation and plugin threads

### Ardour

**Computation.**

- `Session::update_latency_compensation` (`A:session.cc:7646`) walks routes in graph order calling `Route::update_signal_latency` (`A:route.cc:4762`). That sums the processors' `effective_latency()`, sets each `LatentSend`'s input and output delay, sets the sidechain input port latencies, and aligns an unconnected output to master.
- When send latencies move it re-iterates, up to 5 times ("should probably recurse or process the graph in a 2 step process", `A:session.cc:7372, 7406-7427`).
- `Route::apply_latency_compensation` (`A:route.cc:4916`) sets the route's delay line to `play_lat_in − play_lat_out − signal_latency`.

**Applying it.**

- `DelayLine::set_delay` only sets a pending delay. `run` changes it with a fade-out, silence and fade-in, or a crossfade (`A:delayline.cc:294`).
- **The disk reads ahead.** A route processes at `start + signal + output latency`, and each processor sees its own offset (`A:route.cc:421-463, 573-590`).
- After a locate, `_remaining_latency_preroll` no-rolls each route until its own latency is reached (`A:session_transport.cc:1787`, `A:route.cc:4361-4391`).

**Plugin threads.**

- A route goes to whichever graph worker pops it, so **any plugin is called from any DSP thread from one cycle to the next**, with no affinity. Ardour assumes a plugin's `process` may move between threads as long as two calls never overlap, which the route's processor lock and the graph guarantee.
- **LV2.** Each instance has its own `LV2Worker` thread; responses are delivered after `run()` (`A:worker.cc:40-44`). The host tells the plugin the DSP thread count and its scheduling class (`A:lv2_plugin.cc:627-651`).
- **VST3.**
  - `connect_and_run` try-locks `process_lock` and skips the block if it is busy (`A:vst3_plugin.cc:718-721`).
  - Preset loads take it (`:882, 900`).
  - `restartComponent(kReloadComponent)` deactivates and reactivates off the RT thread (`:1650-1667`).
  - `IRunLoop` is the glib main loop, i.e. the GUI thread (`A:vst3_host.cc:98-213`).
  - **`save_state` calls `getState` without the process lock** (`A:vst3_plugin.cc:2837`), so a state save from the GUI overlaps processing. That is spec-legal for VST3 but racy for some plugins.
- **UI parameter writes** go through shadow data copied in at the start of `run()`.

### Ours

**Computation and applying it** (`DAW:render/pdc.rs:88-142`).

- A track's chain latency is the sum of the latencies of its enabled plugins, in a lock-free `ChainLatencies` table.
- `plan` walks once in processing order: a parent's input is the max of its children; a send destination's is the max of its taps. It works out every edge's delay in one pass, without iterating.
- The delay is applied at each edge in the producing node (sends `node.rs:825-853`, parent share `:895-916`).
- Items are offset by `plan.input`; each plugin's transport by `input + chain latency so far` (`node.rs:603-605`).
- Live, a change takes effect one block later. Offline, `prime` fills the table before the first block and the output drops its leading `latency` samples.

**Plugin threads.**

- Like Ardour, a plugin's `process` runs on whichever lane takes its track. CLAP is `parallel_safe` unless `FTS_RENDER_CLAP_SERIAL=1` (`DAW:plugin_host.rs:2113-2124`).
- **The main thread is the audio thread.** The host answers `is_main_thread == is_audio_thread == true` (`plugin_host.rs:348-361`). Main-thread work (`pump_main_thread`, timeline binding, param info) runs on `clap-main` under the plugin's own cell lock, every 50 ms (`ENG:clap_host.rs:95-166`). That is the same lock `process` holds (`DAW:plugin.rs:814-869`, `node.rs:578`).
- **The consequences run both ways:**
  - UI reads wait for `process` (§ `SIGNAL_FEED`, 26–93 ms).
  - `process` skips a plugin whose lock is held for more than 2000 spins (`PLUGIN_BUSY_SKIPS`). Ardour's VST3 path does the same.

### Compared

**PDC.** Ours is cleaner: one pass, no 5-iteration cap, per-edge delays. Two things to check against Ardour:

- **Delay changes are faded.** Ardour fades or crossfades a delay that changes. Check that our `Delay` ring does too when a plugin's latency changes live *(not verified)*.
- **The latency pre-roll after a live locate** (see §2).

**Threads.** The model is the same: any thread, never two at once. The difference is that our main-thread work is serialised with `process` by one lock, where Ardour's goes to the GUI thread (VST3), its own worker (LV2), or shadow data (parameters).

That lock is the cause of the `SIGNAL_FEED` stalls. Recommendation #4 removes reads from it; plain-data reads should never need the plugin. CLAP's thread model permits main-thread calls concurrent with `process` for most extensions. Separating "main" from "audio" in our host would allow it in general, but that is a larger change; do #4 first.

## 6. What an agent's offline measuring can borrow

| Ardour has | What it does | For us |
|---|---|---|
| **Freeze / bounce** (`Track::bounce_range` → `Session::write_one_track`, `A:session.cc:6408`) | Blocks the whole engine (`block_processing()`), runs the track at an 8192 block on the **live** instances, reads the playlist directly, drops `bounce_get_latency` samples, post-rolls the tail, and writes a file. It rejects plugins with a connected sidechain. | We already do better with the per-track cache (no engine block, keyed by content). Borrow the **persistence**: the cache as files under the song (#7), and an explicit "freeze" in the UI that just pins a cache entry. |
| **Peak files** reused when newer than the audio (`A:audiosource.cc:278-349`), built on 2 deferred threads | Load-time analysis cached on disk | The same rule for our per-track recordings (#7), and for media analyses (onsets, loudness) keyed on `MediaKey`. |
| **Region read cache** (`AudioRegion::_readcache`, `A:audioregion.cc:803-892`) | Caches region-FX output | Our offline blocks (`signal-offline-blocks`) are analysis turned into automation; a region-level cache is a good fit for stretch renders too (`render/stretch_render.rs`). |
| **AnalysisGraph** (`analyze_range`, 8192-sample chunks, no plugins) | Loudness, peak and spectrum of raw audio | A measurement of a dry track needs no render. Make sure `mix_measure` on a track with no FX (or a cached one) never builds a renderer. |
| **One pass, many outputs** (every format of a timespan from one render) | | Batch an agent's measurements per span: one render, every probe and every metric. A `script` of several `listen` calls over the same span should share one render. |
| **Normalisation in two passes** | Analyse, then apply from a temp file | Not needed: ours measures and changes the mix, not the file. |

## 7. Where Ardour is weaker than our design

- **No render ahead.** Ardour's graph runs one period per callback inside the deadline, while the backend thread sleeps on `_callback_done_sem` (`A:graph.cc:452`). A 246-plugin song has to fit every plugin into each 64–256-frame period on cores − 1 threads. Ours renders non-live tracks up to 200 ms ahead in chunks of up to 1024 frames on 15 threads, and only live-monitored paths run at the device block, as REAPER's anticipative FX and Cubase's ASIO-Guard do *(reported)*. Keep it.
- **No offline isolation.** Ardour's export and bounce use the live instances and stop the session; a bounce blocks the engine. Ours renders beside a playing session. That is why copies exist; fix their cost (#1, #2), not the design.
- **No content-keyed cache.** Ardour re-renders everything on every export; freeze is manual and per track. Our per-track cache (`Kept`, keys over prints, state fingerprints and feeders) is something it lacks.
- **Freewheel is slow at its own edges.** Period-sized blocks, a butler round-trip every cycle, a full refill and a 2 s pre-roll per timespan, and a separate freewheel restart for each timespan. Hundreds of short measurements would each pay all of that *(inferred)*.
- **The graph:** O(N²) edge construction, a non-atomic chain swap ("never rt-safe"), a tempo map read per thread that can differ within a cycle (`A:graph.cc:334-340`), and feedback sends with no ordering at all.
- **Locate waits** for a full 10 s refill per track.
- **The VST3 state save races `process`** (no lock). Don't copy it; our lock is correct, just too coarse.

**Don't import Ardour's semaphore wake per node** for the live path. Our spin-then-park pool suits small periods better. Offline, keep yielding.

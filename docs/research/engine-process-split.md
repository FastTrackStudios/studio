# The engine as its own process: what other audio software does, and what Studio should do

Research for the goal: **the `studio` CLI is the whole engine**, the desktop
app spawns it (or connects to one already running, locally or remotely),
everything is drivable from the CLI and MCP, and the GUI is a remote
control. Written 2026-10-27 against `main` at `ed25a307` (branch
`feat/no-issue-project-sync`). Read with `docs/design/one-engine.md` and
`docs/design/set-engine.md`.

How to read the citations: a URL is a primary source (official manual,
source file, header, developer page) unless it is marked *(forum)* or
*(secondary)*. Claims marked **inferred** are drawn from shipped binaries
or file names, not from documentation, and say so. Repo paths are
relative to the root of this checkout.

## Summary

1. **Splitting the engine from the GUI is common, but it comes in two
   different forms.** The *protocol-first* systems — SuperCollider,
   Pure Data, Sonic Pi, Csound's UDP server, FMOD/Wwise authoring against
   a running game — run a server that knows nothing of the GUI. Any
   client can drive it, and the GUI is just one client. Among the
   commercial DAWs, only **Bitwig** ships a separate engine
   executable (`BitwigAudioEngine-X64-AVX2` beside a JVM GUI and
   `BitwigPluginHost-*`). Its engine is built as a *slave replica of the
   document* (`float/engine/slave/…`, an `EngineSharedMemoryManager`,
   named pipes). That is inferred from the binary, not documented. Ardour,
   REAPER, Tracktion, Qtractor, Live, Logic, Studio One and Pro Tools run
   one process. Their remote surfaces (OSC, WebSockets, MCP, HTTP,
   EUCON) are threads inside it.
2. **Ardour is the warning for Studio.** libardour is "cleanly separated"
   and runs headless (`hardour`, `luasession`). Even so, the manual says
   headless use is limited "by the fact that most actions in Ardour are
   provided by the Editor GUI". Studio has the same problem today. The
   Action List and the MCP server live in the window crate
   (`apps/session/daw/src/actions.rs`, `mcp_server.rs`), and 55 files in
   `apps/session/daw/src` take a `&Standalone` directly. Splitting the
   process is easy. Moving the *editing logic* out of the window is the
   real work.
3. **Nobody makes an RPC per frame.** In-process Ardour polls meters on a
   40 ms timer (`gtk2_ardour/timers.cc`). REAPER's web remote polls
   `TRACK` (which carries `last_meter_peak`) on a timer. SuperCollider
   puts control buses and scope buffers in shared memory. The GUI keeps a
   local model, receives pushed changes, and fills in motion locally (the
   playhead from a stamped position, the way Ableton Link maps host time
   to beats). Studio already has the pieces: `Peaks::meters` (a pushed
   ~30 Hz `MeterFrame`) and `TransportSync::positions` (clock-stamped
   playhead plus a clock estimator).
4. **Sample accuracy comes from a timestamp in the command, not from a
   fast channel.** SuperCollider sends messages "with a time stamp,
   indicating the precise time the sound is expected to hit the hardware
   output" (default `latency = 0.2`). Unstamped messages jitter by "about
   2 ms each event". Studio's `ScheduledLocate` is the same idea and
   already exists.
5. **Bulk data does not go through the control channel.** Peaks live in
   files on disk (Ardour `peaks/`, REAPER `.reapeaks`, Studio's
   `peak_store`). Audio between processes goes through shared memory,
   with sockets or semaphores used only to signal (yabridge, Carla, JACK,
   PipeWire, Bitwig). vox itself once had an SHM transport and **removed
   it** (`vox-core/CHANGELOG.md`). Shared memory belongs in a side
   channel, not in the RPC.
6. **Some projects tried this and stopped.** Zrythm v2 scaffolded a separate
   engine process (`src/engine-process`, QLocalSocket IPC). It is
   currently **disabled**: "FIXME: temporarily disabled - engine-process
   not currently used". The Pd core has to flow-control its own GUI
   socket (a ping round trip every 1024 bytes). Renoise users report GUI
   lag with plug-in sandboxing. macOS has been seen killing Logic's
   `AUHostingService` as "idle" (forum). Ardour's maintainers reject
   *plug-in* sandboxing on cost: 256–768 context switches per block in a
   large session.
7. **Recommendation:** make the boundary real *in-process first*. The GUI
   speaks only vox clients over the memory link it already has
   (`build_in_process_daw`). Then move the engine-side logic, the
   data actions and the MCP server into an engine crate that
   `studio engine` serves over a Unix socket, plus WS/iroh for remote
   clients. The desktop app attaches to a running engine or spawns one
   (in its own session, so it survives a GUI crash). On iOS it keeps the
   same engine in-process over the memory link, because iOS cannot spawn a
   helper process. Meters, positions and changes are pushed. Peaks come
   from files. Shared memory is added only for scopes and analyzers, when
   measurement shows it is needed. Plug-ins stay in the engine process
   for now.

## Survey

### Summary table

| DAW / system | Engine a separate process? | GUI ↔ engine channel | Headless? | Notes | Sources |
|---|---|---|---|---|---|
| **SuperCollider** | Yes. `scsynth`/`supernova` server; `sclang` is a client | OSC over UDP (default) or TCP; shared memory for control buses and scope buffers | Yes; the server never has a GUI | The canonical case. "If the client crashes, the server keeps on working". Many clients per server. Timestamped bundles give exact timing | [ClientVsServer], [ServerTiming], [MultiClient], [server_shm] |
| **Pure Data** | Yes. `pd` core and `pd-gui` (Tcl/Tk) | Localhost TCP socket. Core→GUI sends Tcl commands; GUI→core sends Pd messages. Either side can start first | Yes (`-nogui`; libpd) | Core flow-controls the GUI (ping every 1024 bytes). A watchdog process guards the realtime core | [pd_connect.tcl], [s_inter.c], [pd#1694] |
| **Csound** | Optional: `--port=N` daemon | UDP: orchestra code, channel set/get, OSC (Cs7) | Yes | Frontends (CsoundQt, the VS Code plug-in) run it in-process via the API or attach over UDP | [cs-flags], [cs-udp], [CsoundThreaded] |
| **Sonic Pi** | Yes. Qt GUI, Ruby "Spider" server, Erlang "Tau" (MIDI/OSC), `scsynth` | OSC between all of them; a boot daemon allocates ports and a token | The server stack needs no GUI | Four processes and a keep-alive loop. Boot and port problems are its most-filed issue | [sonicpi-wiki], [sonicpi#3410] |
| **Bitwig Studio** | Yes. `BitwigAudioEngine-X64-AVX2` + JVM GUI + `BitwigPluginHost-*` + `BitwigVampHost` | Not documented. *Inferred* from the engine's strings: named pipes, local sockets, `/dev/shm/bitwig-posix-*`, an engine shared-memory manager | No (engine started by the app) | "If your audio engine crashes, it doesn't bring down the program – simply reload the engine". Engine is loaded per project tab. Five plug-in hosting modes | [bw-modern], [bw-plugins], [bw-zip] |
| **Ardour / Mixbus** | No (libardour + gtk2_ardour in one process) | In-process signals marshalled to the GUI thread by per-thread request ring buffers; GUI timers at 40/100 ms. Surfaces: OSC (3819), WebSockets (3818), MCP over HTTP (4820) | Partly: `hardour` (demo, built from source), `luasession` | "Most actions … are provided by the Editor GUI". Refuses plug-in sandboxing on cost grounds | [ardour-tree], [hardour], [ardour-lua], [ardour-threads], [ardour-ws], [ardour-mcp], [ardour-osc], [ardour-plugins], [timers.cc], [abstract_ui.h], [mixbus-ext] |
| **Tracktion Engine / Waveform** | No; the engine is a JUCE module (a library) | In-process | Possible, but the module pulls in JUCE GUI modules and needs a message loop | "the engine is inherently async"; the engine provides no UI | [te-features], [te-headless] |
| **REAPER** | No (plug-in bridge `reaper_host_x86_64` only) | In-process; web remote = HTTP polling of `/_/` commands; OSC via `.ReaperOSC` patterns | No; `-renderproject`, `-batchconvert`, `-nonewinst` still run the app | Command line in 7.75 has no headless/daemon flag. Anticipative FX renders ahead on worker threads | [reaper -h], [reaper-www], [reaper-osc], [reaper-bridge] |
| **Zrythm v2** | Scaffolded, **disabled** | QLocalSocket `ZrythmEngineSocket`; 5 message types | Not in use | "FIXME: temporarily disabled - engine-process not currently used" | [zr-ipc], [zr-app] |
| **Qtractor** | No (one JACK client, Qt GUI) | In-process; disk threads via lock-free ring buffers | No | NSM-aware | [qtractor-manual] |
| **LMMS** | No; VSTs bridged into `RemoteVstPlugin` | Shared memory + sockets (moved off shared semaphores for 32/64-bit) | No | Bridging only for Windows VSTs under Wine | [lmms#2739] |
| **Carla** | Engine in-process; plug-ins optionally in bridge processes | Four shm segments: audio pool, RT control ring + semaphore, non-RT client ring, non-RT server ring | Yes (`carla-single`, the backend lib) | Bridge spawn + shm setup measured 20–800 ms per plug-in | [carla-defines], [carla-utils], [carla#2084] |
| **yabridge** | Plug-ins in Wine host processes | Unix sockets for calls; shm for audio buffers ("the sockets act as a form of synchronisation") | n/a | Ad-hoc extra sockets when one is busy (mutual recursion) | [yabridge-arch] |
| **Non/New Session Manager** | Session manager spawns each app as its own process | OSC over UDP (`NSM_URL`, `/nsm/server/announce`, `/nsm/server/add`) | Yes | The model for "a supervisor launches and saves engines" | [nsm-api], [nsm-new] |
| **JACK** | Yes: `jackd` server, apps are clients | Shared memory for buffers; FIFO/futex wake-up chain | Yes | Clients run `process()` in a SCHED_FIFO thread; JACK2 async mode adds a period of latency | [jack-callbacks], [jack1v2], [jack-lkml], [jackd.1] |
| **PipeWire** | Yes: server + client nodes | memfd shared memory for buffers/activation records; eventfd wake-ups straight between peers | Yes | Main thread = IPC/config; data threads = RT only; async links add one cycle | [pw-sched], [pw-design] |
| **AUv3 / Logic / MainStage** | Plug-ins out-of-process (AUv3 default; Logic on Apple silicon: `AUHostingService`) | Apple's XPC extension service | n/a | iOS: out-of-process is the only option; same-extension instances share a process | [au-diffs], [iplug2#1419], [mixxx#16106], [au-forum], [logic-forum] |
| **Ableton Live** | No; no plug-in sandbox | In-process; Max for Live via bundled Max; Link syncs separate apps | No | Link: each app has its own timeline; host-time ↔ beat mapping | [link], [m4l-help] |
| **Pro Tools** | AAE in the app; HDX mixes and AAX-DSP plug-ins run on the card | EUCON over Ethernet (EuControl) for surfaces and the Avid Control app | No | AAE replaced DAE in PT 11 | [pt11-faq], [eucon] |
| **Studio One** | Not documented | — | No | No primary source found for any out-of-process mode | — |
| **Logic Remote** | Remote app, not an engine split | Wi-Fi or USB; Logic accepts the connection on the Mac | n/a | The GUI-as-remote pattern Apple ships | [logic-remote] |
| **FMOD Studio** | Authoring tool connects to the running game's engine | "Live Update" (TCP, port 9265) + profiler | The runtime has no GUI | Almost every change shows up live; `.strings.bank` changes need a rebuild | [fmod-live] |
| **Wwise** | Same shape: authoring ↔ game's sound engine | Profiler connection; WAAPI (WAMP, port 8080) drives *authoring* | Runtime has no GUI | WAAPI is the scripting/MCP surface over the authoring app | [waapi] |
| **VST3 (design precedent)** | Processor and controller *may* be separated | Host-mediated parameter changes + `IConnectionPoint` messages | n/a | "even on different computers", only with `kDistributable` | [vst3-api] |
| **Vienna Ensemble Pro** | Yes: server app; DAW plug-in is a client (localhost or LAN) | Its own network protocol | Server runs on its own | Adds 0/1/2 buffers of latency for live play; compensated on playback | [vep-plugin] |

### Notes per system

**SuperCollider** is what the user wants, in its purest form. The
server "knows nothing about SC code, objects, Object Oriented
Programming"; client and server talk "a specific subset of CNMAT's Open
Sound Control … (over TCP or UDP)", on one machine or across a network,
and "if the client crashes, the server keeps on working, i.e. the audio
does not stop" [ClientVsServer]. Several clients can share one server
(`maxLogins`; each gets a `clientID`) [MultiClient]. For timing it does not
rely on a fast transport. Messages carry an OSC time stamp "indicating
the precise time the sound is expected to hit the hardware output", taken
from the client's `latency` (`0.2 is the default`), and "all sequencing
routines should use latency". Messages sent without one ("play … immediately on
receipt") lag "by about 2 ms each event — and not by the same amount each
time". Live input should use no latency [ServerTiming]. The one exception
to "everything is a message" is shared memory: `server_shm.hpp` maps a
segment named `SuperColliderServer_<port>` holding the control buses and
128 scope buffers, which the language reads synchronously [server_shm].
**Lesson:** commands go over a protocol, high-rate observation goes
through shared memory, and timing comes from timestamps.

**Pure Data.** Two processes over a localhost socket. Either one can start
first and hand the other a port [pd_connect.tcl]. The protocol is the
weak point: the core sends raw Tcl, and a 2022 discussion proposes
abstracting it so other GUIs can be written [pd#1694]. The core rations
what it sends to the GUI. `GUI_UPDATESLICE 512` bytes per idle period, and
after `GUI_BYTESPERPING 1024` bytes it sends `pdtk_ping` and waits for the
reply before sending more [s_inter.c:747-748, 985-1000]. A
watchdog process with higher realtime priority stops the core if the
GUI's pings stop arriving [s_inter.c:1720-1727]. **Lesson:** a GUI that
falls behind must not grow the engine's queue without bound. Use
backpressure, or coalesce updates to the latest value.

**Sonic Pi** runs four cooperating processes (GUI, Ruby "Spider" server,
Erlang "Tau" for MIDI/OSC, scsynth). A boot daemon hands out ports and a
token and keeps them alive [sonicpi#3410]. The GUI's API is OSC commands
[sonicpi-wiki]. **Lesson:** a multi-process boot is a common source of
"GUI was unable to connect to the server" bug reports. Keep it to one
engine process and make discovery deterministic.

**Csound.** `--port=N` makes Csound a UDP daemon that accepts "commands and
instruments/orchestra code (implies --daemon)" [cs-flags], [cs-udp]. The
API alternative `CsoundThreaded` runs `Perform()` on its own thread "fed
by a thread-safe FIFO of messages", quantised to one k-period
[CsoundThreaded]. **Lesson:** the same engine runs both in-process
(API) and as a daemon (UDP). The process boundary is a deployment choice.

**Bitwig.** Officially: "keeping its application, audio engine, and
plug-ins in separate threads … If your audio engine crashes, it doesn't
bring down the program – simply reload the engine and keep working", and
you "manually load the audio engine for whichever project you are working
on" [bw-modern]. The plug-in modes run from "Within Bitwig" (a crash
"would also crash the audio engine") to "Together", "By manufacturer",
"By plug-in" and "Individually". Changing mode needs the project reopened
or "reload the audio engine" [bw-plugins]. *Shipped binaries:*
Bitwig 6.1.4 for Linux contains `bin/BitwigAudioEngine-X64-AVX2`,
`bin/BitwigPluginHost-X64-AVX2`, `bin/BitwigPluginHost-X86-AVX2`,
`bin/BitwigVampHost`, and `bin/bitwig.jar` run by a bundled JRE. Read
from the central directory of the official zip [bw-zip].
**Inferred** from the engine binary's strings: build paths under
`float/engine/slave/src/main/model-cpp/…` and `model-include/flt/core/…`
(the engine holds a *replica* of the document model: tracks, timelines,
devices). Also `EngineSharedMemoryManager`, `SharedMemoryChunkInfo`,
`/dev/shm/bitwig-posix-`, "Could not create named pipe for new audio
thread communication", "Could not create local client socket for port",
and "Error creating connection for project". So Bitwig's GUI owns the
document, and the engine is a slave that mirrors it over pipes and shared
memory, one connection per project. Analysis (Vamp) also runs out of
process. **Lesson:** the one commercial DAW that split the processes made the
*GUI* the owner of the document. That is the opposite of what Studio
wants (CLI/MCP-first). It is what lets Bitwig restart a crashed engine
from the GUI's copy. Studio needs the reverse: the engine survives the GUI and
keeps its own autosave.

**Ardour (and Mixbus, which is built on it).** The source overview calls
`gtk2_ardour` "cleanly separated from the backend and processing engine",
with `luasession` a "fully fledged commandline interface to libardour"
and `headless/` "hardour – headless Ardour, mostly demo-code"
[ardour-tree]. `hardour` creates the `AudioEngine`, starts it and loads a
`Session` [hardour]. A developer answer says it needs a
control surface (OSC) to be driven, and is not part of releases
*(forum)* [ardour-headless-forum]. The manual is blunt: "Interaction is
limited by the fact that most actions in Ardour are provided by the Editor
GUI" [ardour-lua]. Threads: a UI thread (GUI, or OSC/MIDI/Mackie
surfaces), the realtime process thread, and the butler for disk I/O and
non-realtime transport work [ardour-threads]. Cross-thread calls go
through per-thread `RequestBuffer` ring buffers (`PBD::AbstractUI`)
[abstract_ui.h]. The GUI repaints meters on timers: `rapid(100)`,
`super_rapid(40)`, `fps(40)` ms [timers.cc]. Remote surfaces are in
process: OSC on 3819 with opt-in feedback bits (including `/strip/meter`)
[ardour-osc], [ardour-osc-fb], a WebSockets/JSON server on 3818
[ardour-ws], and an experimental **MCP HTTP surface on 4820** that
exposes transport, tracks, plug-ins, regions and MIDI as tools
[ardour-mcp]. Peaks are a disk cache in `peaks/`, keyed on file mtime
[ardour-session]. On plug-in sandboxing: "there must be at least two
context switches" per plug-in run. A 384-plug-in session needs
"256 to 768 context switches per block", at an assumed ~30 µs each,
"7.7msec to 23msec spent doing nothing but context switches", so
"it doesn't scale up to work with large sessions with low latency
settings" [ardour-plugins]. **Lessons:** (a) a library split is not
enough while the editing logic lives in the GUI; (b) even in-process,
meters are a 25 Hz poll; (c) the cost of crossing a process boundary
matters *on the audio path*, not on the control path.

**Tracktion Engine** "doesn't provide … any kind of UI" [te-features].
Headless use works, but the module drags in JUCE GUI modules, and the
maintainer notes that "the engine is inherently async", so a message loop
is needed [te-headless]. **Lesson:** an engine that depends on a
message loop needs a headless message loop. Studio's equivalent is
tokio + `architect::platform`, already headless in `--engine`.

**REAPER.** `reaper -h` (7.75) lists `-renderproject`, `-batchconvert`,
`-nosplash`, `-newinst | -nonewinst` ("add media files and/or run scripts
in an already-running instance"), `-close[all][:save|:nosave][:exit]`
and `-noactivate`. There is no headless or daemon mode: every mode still
runs the app [reaper -h]. Its plug-in bridge is a separate binary
(`Plugins/reaper_host_x86_64`), with modes "separate process", "dedicated
process" and native [reaper-bridge]. The bundled web remote is
HTTP polling. `wwr_req_recur("REQUEST", interval)` re-requests status on a
timer, `GET /_/TRANSPORT;TRACK…`, and each `TRACK` line carries
`last_meter_peak` [reaper-www]. OSC is pattern-configured
(`Default.ReaperOSC`) [reaper-osc]. Anticipative FX renders non-live
tracks ahead of time on worker threads (default ~200 ms), outside the
realtime callback *(forum)* [reaper-afx]. **Lesson:** REAPER gets its
responsiveness from strict threading inside one process, not from a
process split. Its remote is coarse polling, and that is enough for a phone.

**Zrythm v2.** The tree has `src/engine-process/` with
`AudioEngineApplication` ("ZrythmEngine"), a `QLocalServer` on
`ZrythmEngineSocket`, and an `IPCMessage` enum of `SetVolume`, `Play`,
`Stop`, `GetStatus`, `StatusUpdate` [zr-ipc]. The GUI has
`launch_engine_process()` with a `QProcess`. But `main()` is under `#if 0`,
`add_subdirectory(engine-process)` is commented out, and the app says
"FIXME: temporarily disabled - engine-process not currently used"
[zr-app]. **Lesson:** a five-message IPC is easy. Carrying the whole
document model across the boundary is the hard part, and it was put off.

**Plug-in bridges: Carla, yabridge, LMMS, AUv3, Logic.** All of them
use the same pattern. Audio goes through shared memory sized before
processing starts. A cheap signal (semaphore, futex, socket message)
says "go". Control goes on separate channels by timing class. Carla
has four shm regions: an audio pool, an RT control ring with a semaphore,
and non-RT rings in each direction [carla-utils]. Its RT opcodes carry
frame offsets (`ControlEventParameter // uint/frame …`) [carla-defines].
A bridge takes 20–800 ms to spawn and set up [carla#2084]. yabridge:
"store the audio buffers in a big block of shared memory and use the
sockets for all other data … the sockets act as a form of
synchronisation" [yabridge-arch]. LMMS moved its sync from shared
semaphores to sockets because `sem_t` cannot be shared between 32- and
64-bit processes [lmms#2739]. AUv3 loads out-of-process by default.
`kAudioComponentInstantiation_LoadInProcess` is "Only available on
macOS" [au-diffs], [iplug2#1419]. Instances of one extension share a process
[au-forum]. Mixxx switched to out-of-process loading to survive crashing
AUs [mixxx#16106]. Logic on Apple silicon hosts AUs in `AUHostingService`,
and users report it being killed by `memorystatus` as idle *(forum)*
[logic-forum].

**JACK and PipeWire** are the OS-level form of "engine in one process,
clients in others". JACK clients run `process()` in a realtime thread,
with strict rules (no malloc, locks or I/O) [jack-callbacks]. JACK1 woke
clients through a chain of FIFOs [jack-lkml]. JACK2 has an async mode
that "writes the outputs computed at the previous cycle", adding a period
of latency [jack1v2]. PipeWire: "a main thread that will do all IPC …
One (or more) data processing threads that only do the data processing".
Buffers and activation records are set up "in shared memory before the
node is scheduled", and remote clients wake peers "directly without going
to the server first" [pw-sched]. **Lesson for Studio:** the GUI is not
on the audio graph, so none of this per-cycle machinery is needed between
GUI and engine. It is needed only if plug-ins are ever sandboxed.

**Game audio (FMOD, Wwise)** is the closest analogue to "the GUI is an
editor attached to a running engine". FMOD Studio connects to the game
("File > Connect to Game…", port 9265). "Almost every change" applies live,
except changes to the `.strings.bank` (names), which need a rebuild
[fmod-live]. Wwise's authoring app connects to a game's sound engine for
profiling. WAAPI is a WAMP API over the *authoring* app (port 8080),
which community MCP servers now wrap [waapi]. **Lesson:** the editor
holds the authoring model; the engine holds the runtime state; the link
streams edits and profiling data. Renames and structure changes are the
expensive cases.

**VST3** builds the split into the plug-in API: the processor and the edit
controller can run "in a different context, even on different
computers", but only if the plug-in sets `kDistributable`. Parameter
changes go through the host into the processor's `IParameterChanges`
(sample-offset queues). Private data goes as `IMessage`s, and "you CANNOT
rely on the implementation detail that the connection is done directly"
[vst3-api]. **Lesson:** parameter changes are host-mediated,
sample-stamped events, not shared memory writes. Studio's engine can
treat GUI edits the same way.

**Vienna Ensemble Pro** is a product built on "the engine is a server":
the DAW plug-in is a client of a VE Pro server on localhost or the LAN. It
adds 0, 1 or 2 buffers of latency for live playing ("2 buffers"
recommended) and compensates it on playback [vep-plugin]. **Lesson:**
only *live input* pays for a process hop. Playback is planned ahead and
compensated.

**Ableton.** Live has no documented plug-in sandbox (absent from the
release notes; users ask for it on the forum). Max for Live is a bundled
build of Max that Live starts [m4l-help]. The process model of Max for
Live is not documented in any primary source found. Link is relevant for
timing: "each Link-enabled app has its own independent timeline", and
Link maps host time to beats per peer, using the audio callback's
host time at output [link].

**Pro Tools.** AAE replaced DAE in Pro Tools 11, rebuilt as a 64-bit engine
[pt11-faq]. With HDX the mixer and AAX-DSP plug-ins run on the card's
DSPs: a hardware engine the host app controls. EUCON connects surfaces
and the Avid Control app over Ethernet through EuControl [eucon].
**Studio One:** no primary source found for any out-of-process engine
or plug-in mode. **Logic Remote** controls Logic, GarageBand or MainStage
over Wi-Fi or USB, with the Mac accepting the connection [logic-remote].

## What this means for Studio

### What Studio has today (from the code)

- One engine per process (`daw_standalone::Standalone`), with a set engine
  mixing many songs (`set-engine.md`).
- The GUI already talks to the engine through vox *in-process*:
  `build_in_process_daw` serves `Standalone`'s router on a
  `LocalServer` and hands back a `daw_control::Daw`
  (`crates/daw/standalone/src/bootstrap.rs:45`).
- A headless Session engine exists. `session-desktop --engine` serves
  "the whole daw facade" over `ws://0.0.0.0:4040/vox` and iroh
  (`apps/session/desktop/src/engine_server.rs`). Signal's
  `studio-desktop --engine` serves the rig on the same `EngineHost`,
  with MCP on 127.0.0.1:47812 (`apps/studio/desktop/src/engine_main.rs`).
  A supervisor spawns it or attaches to it, optionally through a systemd unit
  (`apps/studio/desktop/src/engines.rs`).
- Push streams already exist: `Peaks::meters` (one `MeterFrame` per
  project, ~30 Hz) and `TransportSync::positions` (clock-stamped playhead
  at least every ~20 ms, plus `clock_now` for NTP-style offset
  estimation) (`crates/daw/proto/src/peak/service.rs`,
  `transport_sync/service.rs`). Each object type also has a `#[subscribe]`
  event stream (tracks, markers, regions, tempo map, transport, event bus).
- Peaks: `Peaks::take_peaks` plus a `.reapeaks` mipmap sidecar store in
  standalone (`docs/research/item-peaks.md`).
- vox transports: `vox-stream` (TCP, **Unix sockets**, stdio, with
  length-prefix framing), `vox-local` (Unix socket / Windows named pipe),
  `vox-fdpass` (SCM_RIGHTS fd passing), `vox-websocket`, `vox-iroh`. The
  SHM transport was removed (`vox-core/CHANGELOG.md:89`). The REAPER
  bridge already serves vox on a Unix socket
  (`features/adapters/reaper/daw-bridge/src/lib.rs`).
- **The coupling that has to go:** the Action List and the MCP server (47811)
  are in the window crate `session-daw` (Dioxus/Blitz). The list has
  two kinds of action: *window* actions (transport keys, selection, edit
  cursor, zoom, panels; "running one queues it … the window applies the
  queue on its next pass") and *session data* actions ("run against the
  engine directly") (`apps/session/daw/src/actions.rs:1-19`). `hits.rs`,
  `actions.rs`, `open.rs`, `comp.rs`, `grow.rs` and about 50 other files
  take `&daw::standalone::Standalone`. The `studio` CLI drives *the
  running window* over MCP (`apps/session/desktop/src/cli/mod.rs`).

### Recommended architecture

```
studio engine  (the CLI binary; one process = one engine, one-engine.md)
  Standalone + SetAudio + rigs-as-tracks
  engine services  (vox): daw facade + session/set + editing (hits, comp, quantize, …)
  Action List      (data + editing-state actions) ── MCP :47811 (loopback)
  listeners: unix socket  $XDG_RUNTIME_DIR/fts/engine-<id>.sock (0600)
             ws :4040/vox + iroh  (opt-in, for phone/browser/other machines)
  side channel (later, if measured): memfd rings passed over the socket (vox-fdpass)
  autosave / change journal on disk; peaks (.reapeaks) on disk

studio desktop (Blitz GUI)            studio CLI verbs        models (MCP)
  model replica + view state            same vox clients       same Action List
  attaches or spawns ── unix socket ──► engine ◄── unix socket / MCP
iOS / web: the same engine crate in-process over the memory link (no spawn)
```

**1. GUI ↔ engine transport.**
- *Local:* vox over a Unix domain socket (`vox-stream`; named pipe on
  Windows via `vox-local`). Use the same generated clients as WS and iroh,
  so a remote GUI and a local GUI run the same code. Use a socket per engine
  in `$XDG_RUNTIME_DIR`, mode 0600. No TCP port is needed for the local
  case. Don't build a shared-memory *RPC* transport: vox dropped one, and
  no system in the survey uses shared memory for commands.
- *Remote:* WS or iroh, as `EngineHost` serves them now, off by
  default on a laptop and on for the rig box.
- *In-process* (iOS, web, tests): the existing `LocalServer` memory
  link. The GUI cannot tell the three apart. That is what "exactly the
  same engine" means: one engine crate, one service router, three links.
  On iOS a helper process cannot be launched (platform constraint; not
  verified in a source here), so the in-process link is permanent there,
  not a stopgap.

**2. Waveforms, meters, peaks.**
- *Meters:* keep the pushed `MeterFrame` at ~30 Hz. Even 200 stereo
  tracks are tens of kB/s over a local socket. The GUI interpolates and
  applies ballistics at its own frame rate. Ardour repaints meters at
  25 Hz in-process [timers.cc]. Coalesce to the latest frame per client,
  and never queue (Pd's lesson).
- *Playhead:* never stream positions per frame. The GUI extrapolates from
  `StampedPosition` (host micros, playhead, rate, sequence) through
  `ClockEstimator`. Over a local socket the offset is near zero. This is
  the Link model.
- *Peaks:* the engine writes `.reapeaks` mipmaps next to the media (or in
  a cache dir) and returns their *paths*. A local GUI `mmap`s them, and a
  remote GUI pulls them in chunks with `take_peaks` (i16-quantised, per
  `item-peaks.md`). This matches the Ardour `peaks/` cache and REAPER's
  files. While recording, the engine pushes incremental peak blocks
  for the armed takes on a subscription.
- *Scopes, spectrum, tuner, per-sample analysis:* this is the only place
  for shared memory. It is a side channel: the engine creates a memfd
  ring (seqlock or SPSC) per consumer and passes the fd over the Unix
  socket with `vox-fdpass`. The GUI reads it on its frame tick. This is
  SuperCollider's scope-buffer design [server_shm]. Build it only when a
  measured view needs more than the socket gives. Remote clients get a
  decimated subscription instead.
- *Audio previews* (audition, scrub, click): the engine plays them on
  its one device. The GUI never opens audio.

**3. 120 fps without per-frame RPC.**
- The GUI keeps a **model replica**: a snapshot on connect, then the
  ordered change journal (`#[subscribe]` events with revisions). The frame
  loop reads only the replica, view state and caches. **It never awaits
  an RPC.** This is the `rust-dx` skill's "change journals and persistent
  snapshots", applied across the process boundary.
- Gestures run locally, and the engine gets intent. A drag renders from
  the replica plus the gesture. The engine gets one command at commit, or
  a throttled stream (≤ 30–60 Hz) for things that must sound live
  (faders, with engine-side smoothing). The engine's echo replaces the
  optimistic value.
- Sample accuracy is in the command: anything that must land on a frame
  (song transitions, punch, cue) is a `ScheduledLocate` or another
  stamped command in the sync clock. It is never "send it fast"
  (SuperCollider's bundles).
- Rendering caches (waveform tiles, text layout, frame faces) are pure
  GUI state.

**4. Startup and lifecycle.**
- *Discovery first:* the app reads `~/.config/fts/engines/*.json` (pid,
  socket, ws/iroh addrs, project), the pattern of the existing
  `*-mcp.json` files. If a live engine answers, the app attaches.
- *Spawn otherwise:* `studio engine --socket … [--project …]` in **its
  own session/process group** (`setsid`), so a GUI crash or ⌘Q does not
  take the audio down. Generalise `engines.rs` (child or systemd unit,
  "only ever kills what it controls").
- *Exit policy:* the engine lingers when the last client disconnects. It
  exits after N seconds only if the transport is stopped and nothing is
  dirty. `--persist` or the systemd unit keep it running for a live rig. Quit in the
  app asks the engine to save and stop. Closing the window does not.
- *Many GUIs:* the engine is the single writer. Commands are serialised
  in the engine, undo is engine-side, and every client sees the same
  journal. Each GUI keeps its own view state (zoom, scroll, panels). Store
  selection, edit cursor and time selection in the engine, as project
  state, so CLI/MCP verbs like "quantize the selection" have something to
  act on, and two windows on one project agree.
- *Platform notes:* on macOS, mark the engine as user-interactive audio
  work (no App Nap). Logic's `AUHostingService` being killed as idle is
  the cautionary report *(forum)*. Lock a project to one engine.

**5. Crash isolation.**
- *GUI crash:* the engine plays on (SuperCollider, Pd). On restart the GUI
  reattaches, takes a snapshot and resubscribes. This is the main win for a
  live show.
- *Engine crash:* the GUI shows "engine stopped" and offers a restart. The
  engine reloads from its own autosave/change journal. Bitwig can reload
  from the GUI's copy because there the GUI owns the document. In Studio
  the engine owns it, so durability is the engine's job.
- *Plug-in crash:* keep plug-ins in the engine process for now. Ardour's
  arithmetic shows the cost at scale [ardour-plugins]. Sandboxing can come
  later as a per-plug-in option (Bitwig's "Individually" override, REAPER's
  "dedicated process"). It would use the Carla/yabridge shape: shm audio
  pool sized at activation, RT ring + semaphore, non-RT rings.

**6. What must stay in the GUI process.**
The model replica and its indexes, view state, gesture state and
optimistic values, render caches (waveform tiles from peaks, glyphs,
frame faces), input → action resolution (the key map), and the
interpolation of meters and playhead. Nothing in that list is
authoritative; everything can be rebuilt from the engine.

**7. Migration path from the in-process design.**

0. **Measure the baseline.** Time command round trips and frame times on
   the current memory link, then on a Unix socket in a micro-bench. That
   gives the budget, and it checks the expectation (not sourced here) that
   a local socket round trip is far below a frame.
1. **Make the boundary real in-process.** Move what the window does with
   `&Standalone` (hits, comp, grow, take/comp ranking, open, prepare,
   vocal level, …) into an engine-side crate behind vox services. The GUI
   uses clients over the memory link only. Enforce it in Cargo:
   `session-daw` does not depend on `daw-standalone` (an `inprocess`
   feature on the *host* crate links it). This is the step Ardour never
   finished, and the one that makes everything else mechanical.
2. **Move the Action List.** Data actions and editing-state actions
   (selection, cursor, set/song actions) plus the MCP server go to the
   engine. Window actions stay in the GUI. A GUI registers its window
   actions with the engine when it attaches, so `studio run SESSION_VIEW`
   reaches the focused window, and with no window attached it is listed
   `enabled: false` "no window attached". `studio` CLI verbs then talk to the
   engine, not the window.
3. **`studio engine`.** The CLI binary serves the engine: Unix socket by
   default; `--ws`/`--iroh` opt-in; MCP on 47811. Fold
   `session-desktop --engine` into it, and Signal's `studio-desktop
   --engine` too, once rigs are tracks (`one-engine.md` steps 4–5).
4. **The app attaches or spawns.** `FTS_ENGINE=inprocess|spawn|<addr>`,
   with in-process as the default until parity. iOS and web stay in-process.
5. **Data plane.** Pushed meters, stamped positions and the change journal
   already exist; add peak paths and live recording peaks. Add the memfd
   side channel only for a view that measures short.
6. **Flip the desktop default to spawn.** Add tests: GUI killed while
   playing → audio continues → GUI reattaches; engine killed → GUI
   recovers from autosave; two GUIs on one engine stay consistent; the
   `set-engine.md` frame-time check runs over the socket.

### Risks and open questions

- **Remote document latency.** A fully replicated model means
  every structural edit (rename, reorder, new track) goes engine → journal
  → replica. FMOD treats names and structure as the expensive case.
  Keep journal events small and ordered, and test against JT's
  11-song set.
- **Two writers during migration.** While some window code still edits
  `Standalone` directly, a second GUI or the CLI can race it. Step 1 must
  finish before the desktop default flips.
- **Document ownership.** Bitwig, the nearest precedent, puts the
  document in the GUI. Studio's choice to put it in the engine is right
  for CLI/MCP-first, but it moves autosave and crash recovery onto the
  engine.
- **Not verified here:** the Bitwig GUI↔engine protocol (inferred from
  strings only), Max for Live's process model, Studio One's plug-in
  hosting, and the iOS spawn restriction.

## Sources

Primary unless marked.

- [ClientVsServer] SuperCollider, Client vs Server — https://doc.sccode.org/Guides/ClientVsServer.html
- [ServerTiming] SuperCollider, Server Timing — https://doc.sccode.org/Guides/ServerTiming.html
- [MultiClient] SuperCollider, Multi-client Setups — https://doc.sccode.org/Guides/MultiClient_Setups.html
- [server_shm] SuperCollider `common/server_shm.hpp` — https://github.com/supercollider/supercollider/blob/main/common/server_shm.hpp ; Server class (`hasShmInterface`) — https://doc.sccode.org/Classes/Server.html
- [pd_connect.tcl] https://github.com/pure-data/pure-data/blob/master/tcl/pd_connect.tcl
- [s_inter.c] https://github.com/pure-data/pure-data/blob/75381de0ff127e9bfb36cacb589c1a0f6df4397d/src/s_inter.c (lines 747–748, 985–1000, 1720–1727)
- [pd#1694] https://github.com/pure-data/pure-data/discussions/1694
- [cs-flags] https://csound.com/docs/manual/CommandFlags.html
- [cs-udp] https://csound.com/docs/manual/udpserver.html
- [CsoundThreaded] https://csound.com/docs/api/classCsoundThreaded.html
- [sonicpi-wiki] https://github.com/sonic-pi-net/sonic-pi/wiki/Sonic-Pi-Internals----GUI-Ruby-API
- [sonicpi#3410] boot logs — https://github.com/sonic-pi-net/sonic-pi/issues/3410
- [bw-modern] Bitwig, The Modern Foundations of Bitwig Studio — https://www.bitwig.com/modern-foundations/
- [bw-plugins] Bitwig user guide, Plug-in Handling and Options — https://www.bitwig.com/userguide/latest/vst_plug-in_handling_and_options/
- [bw-zip] Bitwig Studio 6.1.4 Linux archive (file list from its central directory; strings of `bin/BitwigAudioEngine-X64-AVX2`) — https://downloads.bitwig.com/flathub/BitwigStudio-6.1.4.zip , via the Flathub manifest https://github.com/flathub/com.bitwig.BitwigStudio
- [ardour-tree] Ardour source overview — https://community.ardour.org/files/doxygen/index.html
- [hardour] https://github.com/Ardour/ardour/blob/master/headless/load_session.cc
- [ardour-headless-forum] *(forum, developer answers)* https://discourse.ardour.org/t/how-do-i-load-and-play-an-ardour-session-from-the-command-line-headless/100304
- [ardour-lua] https://manual.ardour.org/lua-scripting/
- [ardour-threads] https://ardour.org/transport_threading.html
- [ardour-ws] https://manual.ardour.org/using-control-surfaces/websockets-server/
- [ardour-mcp] https://manual.ardour.org/using-control-surfaces/mcp-http/
- [ardour-osc] https://manual.ardour.org/using-control-surfaces/controlling-ardour-with-osc/osc-control/
- [ardour-osc-fb] https://manual.ardour.org/using-control-surfaces/controlling-ardour-with-osc/feedback-in-osc/ ; https://manual.ardour.org/using-control-surfaces/controlling-ardour-with-osc/calculating-feedback-and-strip-types-values/
- [ardour-plugins] Why doesn't Ardour offer "plugin crash protection"? — https://ardour.org/plugins-in-process.html
- [ardour-session] https://manual.ardour.org/working-with-sessions/whats-in-a-session/ ; https://manual.ardour.org/working-with-sessions/cleaning_up/
- [timers.cc] https://github.com/Ardour/ardour/blob/master/gtk2_ardour/timers.cc
- [abstract_ui.h] https://github.com/Ardour/ardour/blob/master/libs/pbd/pbd/abstract_ui.h
- [mixbus-ext] Harrison Mixbus manual, External Control — https://rsrc.harrisonconsoles.com/mixbus/mixbus-live-manual/11/en/topic/external-control
- [te-features] https://github.com/Tracktion/tracktion_engine/blob/develop/FEATURES.md
- [te-headless] *(forum, maintainer reply)* https://forum.juce.com/t/tracktion-engine-headless-without-x11/42091
- [reaper -h] REAPER 7.75 for Linux, `reaper -h` output (local install)
- [reaper-www] REAPER 7.75 `Plugins/reaper_www_root/main.js` (local install; protocol comment at the top of the file)
- [reaper-osc] REAPER 7.75 `Plugins/Default.ReaperOSC`; OSC docs http://www.cockos.com/reaper/sdk/osc/osc.php
- [reaper-bridge] https://reaper.blog/2012/02/run-plugin-as-dedicated-process/ *(Cockos-affiliated blog)*; REAPER binary `Plugins/reaper_host_x86_64`
- [reaper-afx] *(forum)* https://forums.cockos.com/showthread.php?p=1840896
- [zr-ipc] Zrythm `src/engine-process/ipc_message.h`, `audio_engine_application.h`, `main.cpp` at 1d50dc9d — https://github.com/zrythm/zrythm/tree/1d50dc9d4236e2a87f8553c3536b05f83c926d73/src/engine-process
- [zr-app] Zrythm `src/gui/backend/zrythm_application.cpp` (lines 46–47, 708–749) and `src/CMakeLists.txt:109` at 1d50dc9d — https://github.com/zrythm/zrythm/blob/1d50dc9d4236e2a87f8553c3536b05f83c926d73/src/gui/backend/zrythm_application.cpp
- [qtractor-manual] https://www.rncbc.org/archive/old/qtractor-0.1.1-user-manual.pdf ; https://www.qtractor.org/
- [lmms#2739] https://github.com/LMMS/lmms/pull/2739
- [carla-defines] https://github.com/falkTX/Carla/blob/main/source/utils/CarlaBridgeDefines.hpp
- [carla-utils] https://github.com/falkTX/Carla/blob/main/source/utils/CarlaBridgeUtils.hpp
- [carla#2084] https://github.com/falkTX/Carla/issues/2084
- [yabridge-arch] https://github.com/robbert-vdh/yabridge/blob/master/docs/architecture.md
- [nsm-api] http://www.non.tuxfamily.org/nsm/API.html
- [nsm-new] https://new-session-manager.jackaudio.org/api/index.html
- [jack-callbacks] https://jackaudio.org/api/group__ClientCallbacks.html
- [jack1v2] https://github.com/jackaudio/jackaudio.github.com/wiki/Differences-between-jack1-and-jack2
- [jack-lkml] Paul Davis on LKML (2004) — https://lkml.iu.edu/hypermail/linux/kernel/0412.0/0072.html
- [jackd.1] https://manpages.debian.org/testing/jackd2/jackd.1.en.html
- [pw-sched] https://docs.pipewire.org/page_scheduling.html
- [pw-design] https://docs.pipewire.org/page_design.html
- [au-diffs] Apple, AudioUnit API changes (macOS 10.11) — https://developer.apple.com/library/archive/releasenotes/General/APIDiffsMacOSX10_11/Objective-C/AudioUnit.html
- [iplug2#1419] (quotes `AudioComponent.h`) — https://github.com/iPlug2/iPlug2/issues/1419
- [mixxx#16106] https://github.com/mixxxdj/mixxx/pull/16106
- [au-forum] *(Apple developer forum)* https://developer.apple.com/forums/thread/65909
- [logic-forum] *(forum)* https://www.logicprohelp.com/forums/topic/151900-what-happens-when-auhostingservice-crashes/ ; https://forum.juce.com/t/juce-plug-ins-crashing-under-logic-pro-10-8-1/59020
- [link] https://ableton.github.io/link/
- [m4l-help] https://help.ableton.com/hc/en-us/articles/360000036850-Max-for-Live-bundled-in-Live ; https://support.cycling74.com/hc/en-us/articles/360049845974-Common-questions-about-Max-for-Live
- [pt11-faq] https://kb.avid.com/pkb/articles/faq/Pro-Tools-11-FAQ
- [eucon] https://resources.avid.com/SupportFiles/ProMixing/EUCON_Application_Setup_v2025.6.pdf
- [logic-remote] https://support.apple.com/guide/logicremote-logicpro-ipad/connect-logic-remote-chsa0054e974/ipados
- [fmod-live] https://fmod.com/docs/studio/editing-during-live-update.html
- [waapi] https://blog.audiokinetic.com/everyone-can-use-waapi-wwise-core/
- [vst3-api] https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/API+Documentation/Index.html
- [vep-plugin] https://www.vsl.co.at/manuals/vep7/vep-plugin

Repo sources: `docs/design/one-engine.md`, `docs/design/set-engine.md`,
`docs/research/item-peaks.md`, `crates/daw/standalone/src/bootstrap.rs`,
`crates/daw/proto/src/peak/service.rs`,
`crates/daw/proto/src/transport_sync/{service,types}.rs`,
`apps/session/desktop/src/{engine_server,session_engine}.rs`,
`apps/session/desktop/src/cli/mod.rs`,
`apps/studio/desktop/src/{engines,engine_main}.rs`,
`apps/session/daw/src/{actions,mcp_server}.rs`,
`features/adapters/reaper/daw-bridge/src/lib.rs`; vox at `08db2c4`
(`rust/vox-{stream,local,fdpass}`, `rust/vox-core/CHANGELOG.md`).

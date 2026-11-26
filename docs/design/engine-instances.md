# Engine instances

**Status:** design, 2026-11-25. Nothing here is built yet.

**The ask:** several engines run on one machine at once, each with its own
song or setlist, state, sockets and ports. One agent works on keyflow in one
setlist while another works on DSP in another, and neither disturbs the
other or the owner's own Studio. The UI (`studio-connect`, run by
`apps/studio`) **chooses** which engine it connects to. A remote engine is
the same thing at another address. An agent's MCP server is pointed at one
instance.

This follows the plan's first promise (`fts-home/docs/monorepo-plan.md`,
"The two promises"): the UI takes an engine *address*, never a child it
owns, and the protocol is the same locally and over the network.

## 1. What exists today

**Two folders, two files per engine.** Each engine is known in two places:

- **The launcher's folder.** `daw-extension --engine` holds `engine.lock`,
  `control.sock`, `engine.json` and `engine.log` in `FTS_DAW_ENGINE_DIR`,
  else `$XDG_RUNTIME_DIR/fts/daw` (`crates/studio-engine/extensions/daw/src/owner.rs:24-59`).
  Studio sets this to `$XDG_RUNTIME_DIR/fts/studio-app-engine[-<instance>]`
  (`apps/studio/src/engine.rs:263-271`, `apps/studio/src/main.rs:219-223`).
- **The engine's registry file.** The same process (through
  `engine_host::run`, `extensions/daw/src/lib.rs:61-68`) binds a facade
  socket at `$XDG_RUNTIME_DIR/fts/engine-<pid>.sock`
  (`crates/studio-engine/engine/studio-engine/src/engine_host.rs:168-180`).
  It then publishes `Found {pid, socket, mcp, ws, build}` to
  `~/.config/fts/engines/studio-<pid>.json`
  (`engine_host.rs:128-135`, `crates/studio-connect/client/engine-client/src/studio_engine/link.rs:167-189`).

**Finding an engine.** Studio and engine-client each read that registry,
take the newest entry whose socket accepts a connection, and spawn an
engine when none does (`apps/studio/src/engine.rs:92-139,398-427`,
`link.rs:204-227`). Studio keeps its own copy of the finder because it does
not link engine-client (`engine.rs:6-14`).

**`STUDIO_INSTANCE`** (commit `af410f52f`, `apps/studio/src/instance.rs:1-65`)
renames four things:

- the window's app id: `Studio-<i>`;
- the app's doors: `studio-<i>-mcp.json`, and `studio-<i>.sock` under both
  `fts/` and `app-shell/`;
- the engine folder: `studio-app-engine-<i>`.

The registry is still shared. A named instance only *filters* it: it keeps
the engines whose socket is under its own `XDG_RUNTIME_DIR`. That filter is
work in progress, not yet committed in this checkout
(`engine.rs:106-117`, `link.rs:191-202`). This only works because
`scripts/agent-instance/run.sh:9-12` gives the agent a whole private
`XDG_RUNTIME_DIR` and points Wayland, D-Bus and PipeWire back at the
desktop's. The README says the agent's window shows the person's song
without it (`scripts/agent-instance/README.md:14-16`).

**MCP.** There are two doors:

- **The engine's door** serves its Action List over HTTP on
  `127.0.0.1:47811` (`DEFAULT_PORT`,
  `crates/studio-engine/engine/studio-engine/src/studio_engine/mcp.rs:22-23`).
  It moves with `FTS_MCP_PORT`. Unless `FTS_MCP_PRIVATE` is set, it
  publishes `~/.config/fts/session-mcp.json` (`mcp.rs:26,139-145`).
  Studio picks a free port and sets the door private
  (`main.rs:216,226-230`, `engine.rs:324-363`). It even checks
  `/proc/net/tcp` to see whether another process stole the port
  (`engine.rs:145-183`).
- **studio-mcp**, the agents' stdio server, talks to the **app's** control
  socket, not to the engine. It looks in this order: `--socket`, then
  `STUDIO_MCP_SOCKET`, then `<FTS_CONTROL_DIR | XDG_RUNTIME_DIR | tmp>/fts/<app>.sock`
  with `--app studio` (`crates/studio-connect/mcp/src/upstream.rs:76-101`,
  `mcp/src/main.rs:3-30`). So an agent needs a window to reach an engine.

**systemd.** The launcher starts the engine as the transient user unit
`studio-engine` (`owner.rs:204-251`). The handoff's
`stack-dev/stack/restart-all.sh` and `start-studio.sh` run the owner's
stack as the user unit `studio-native`, with one hard-coded engine folder,
and drive it through `/tmp/smcp_mix.py`.

**On disk now.** `~/.config/fts/engines/` holds about 60 `studio-<pid>.json`
files, and `$XDG_RUNTIME_DIR/fts/` the same number of `engine-<pid>.sock`
files. Engines that crashed never remove theirs, so the registry only
grows.

## 2. Where instances collide

| # | What | Where | Effect |
|---|---|---|---|
| 1 | Engine MCP port 47811 | `mcp.rs:22`, `engine_host.rs:233-238`, `scripts/drum-lab.sh:38` | The second engine retries, then serves no Action List (`mcp.rs:73-92`), or a client reaches the wrong engine |
| 2 | Shared registry | `link.rs:170`, `engine.rs:93` | Any Studio attaches to the newest engine, so a window can cross-link to an agent's engine (`.claude/skills/drum-mixing/SKILL.md:90`). There is no stale cleanup |
| 3 | `last-open.json` in the registry dir | `link.rs:507-527`, `engine_host.rs:48` | `--reopen` opens the song *another* instance last had open |
| 4 | systemd unit name `studio-engine` | `owner.rs:205` | A second `systemd-run` fails and quietly falls back to `setsid`, so `systemctl --user stop studio-engine` stops whichever engine holds the name |
| 5 | Build replacement | `owner.rs:30-32` (`Replace`) | A window of another worktree's build that attaches stops a running engine. With a shared folder, that is someone else's |
| 6 | Headless discovery file `session-mcp.json` | `mcp.rs:26,143` | The last engine to start overwrites it |
| 7 | Engine log `~/.cache/fts/studio-engine.log` | `engine.rs:242-250` | Every instance writes to one log |
| 8 | Dev overrides `overrides.json` | `apps/studio/src/hosted.rs:225-235`, `algorithms.rs:16` | One instance's `STUDIO_EXT_DEV` swaps extensions in the others (`scripts/agent-instance/README.md:27-29`) |
| 9 | Settings written by the engine: `audio-device.json`, `hit-detect.json`, `render-worker.json`, `drum-edit.json`, `~/.config/signal/*` | `audio_device.rs:74`, `hits.rs:162`, `render_worker/policy.rs:194`, `signal/rigs/host/src/store.rs:14` | An agent's experiment changes the owner's defaults |
| 10 | Recent and collab state `~/.local/share/Session/{recent.tsv,live-sync-locked}` | `projects.rs:185`, `collab.rs:322` | Mixed between instances |
| 11 | The song file itself | the `.session` both engines open | Two engines on one song write over each other's edits |
| 12 | `/tmp` scratch: `/tmp/smcp*.py` (handoff scripts), `/tmp/fts-daw-<pid>.sock`, `/tmp/daw-bridge-<pid>.log` | `adapters/reaper/.../daw-bridge/src/lib.rs:182,458` | Agents overwrote each other's helper. The pid-keyed files pile up |
| 13 | Tests' fixed temp names | `apps/studio/src/chart_sync.rs:354`, `session_file.rs:351` (in `#[cfg(test)]`) | Flaky only when two worktrees test at once |
| 14 | Telemetry: service names `studio-engine`, `session`, `studio-render-worker` | `owner.rs:319`, `engine-host/src/lib.rs:178`, `render_worker/serve.rs:296` | Spans from every instance look like one engine in OpenObserve |
| 15 | Audio device | the engine opens its saved device, else the system default | All instances play to the owner's speakers |

**Already safe:**

- the render cache: it lives in memory, per process (`listen/render.rs:2113-2122`);
- the render worker's folder: `render-worker-<pid>` (`render_worker/client.rs:207-212`);
- kache: keyed by content;
- per-worktree `target/`;
- sample libraries and rig models: read only.

## 3. Design

### Identity

- An **instance name** matches `[a-z0-9][a-z0-9_-]{0,31}`.
- The owner's own instance is **`main`**. An unset or empty `STUDIO_INSTANCE`
  means `main`, so today's single-instance behaviour is the default.
- `just engine start` with no name makes one: `<owner>-<4 hex>`, for
  example `keyflow-a3f1`.
- One variable names it everywhere: **`STUDIO_INSTANCE`**. It is read by
  the engine (which instance it is), the UI (which engine it connects to
  first) and studio-mcp (which engine it drives).
- **The window is always "Studio"** (owner's decision, 2026-11-25): app id
  and title are `Studio` for every instance, so the desktop's window rules
  (Loft/niri) apply to all of them. The instance name never reaches the OS
  window identity; the UI shows it inside the app (the engine chip). This
  replaces the earlier `Studio-<name>` app id, which kept agent windows out
  of the owner's rules on purpose; an agent that must stay off the owner's
  layout runs without a window (headless engine plus studio-mcp).
- An instance holds:
  - `name`
  - `owner`: `{kind: human | agent, id}`, where `id` is the T3 thread id or
    worktree path
  - `started_at`
  - `song` and `set`: what is open, updated as it changes
  - `exe`: the engine binary, which tells you the worktree
  - `build`

### Discovery: the registry

`$XDG_RUNTIME_DIR/studio/engines/<name>.json` replaces both
`~/.config/fts/engines/studio-<pid>.json` and `engine.json`. The runtime
dir is a tmpfs that is cleared at logout, so stale files cannot outlive a
reboot.

```json
{ "schema": 1, "name": "keyflow-a3f1", "pid": 41234,
  "owner": {"kind": "agent", "id": "thread:…"}, "started_at": "2026-11-25T10:02:11Z",
  "song": "/…/Crescendum/unbreakable.session", "set": "/…/Crescendum.setlist",
  "addr": "unix:///run/user/1000/studio/keyflow-a3f1/facade.sock",
  "ws": null, "mcp": "http://127.0.0.1:38417/mcp",
  "exe": "/run/media/Development/t3-worktrees/studio/kf/target/desktop-dev/studio-engine",
  "build": "…", "heartbeat": "2026-11-25T10:04:31Z" }
```

- **Liveness is a lock, not a guess.** The engine holds `flock` on
  `$XDG_RUNTIME_DIR/studio/<name>/engine.lock` for as long as it runs.
  - The lock makes names unique: a second `start x` finds it held and
    exits, as `engine.lock` already does per folder.
  - An entry is stale when its lock can be taken. Any reader that finds a
    stale entry deletes it and its folder. This replaces `alive()` probing
    the socket and `serves_its_own()` checking the port.
- **The file is written atomically**, with write-then-rename. It is
  rewritten when the song or set changes, and as a heartbeat every 5 s. The
  heartbeat is for remote listings and display. Local truth is the lock.
- **Remote engines** are advertised through **Drive's peer connections**,
  not by scanning the tailnet. The Drive node already joins the owner's
  devices and authenticates them. It reads its machine's registry and
  serves the entries that have a `ws` address to signed-in peers of the
  same org: `GET /engines` on the node (`:8096`).
  - The UI lists them beside local entries.
  - With no Drive node, an address can be typed in (`--engine ws://host:port/vox`).

### Addressing and transport

- **No fixed ports.** Each instance has `$XDG_RUNTIME_DIR/studio/<name>/`
  (mode 0700), which holds:
  - `engine.lock`
  - `control.sock`: the launcher, start and stop
  - `facade.sock`: vox lanes, the UI's link
  - `token`: mode 0600
  - `tmp/`
  - `render-worker/`

  The MCP door always binds `127.0.0.1:0`, and the bound URL goes into the
  registry. `DEFAULT_PORT` and its retry loop go. `drum-lab.sh` reads the
  registry instead.
- **One address type**, `EngineAddr`, in studio-core. It is a URL:
  `unix:///…/facade.sock` or `ws://host:port/vox`. Both carry the same vox
  lanes (`engine_host.rs:198-229` and `EngineHost` at `:118-127`), so the
  client's code is the same for both.
- **Auth.** The local socket needs no token: it is the user's 0600 file.
  Remote is the `ws` listener, opt-in with `--listen tailnet` (the
  machine's tailnet IP, port 0). It requires the instance's token, through
  the PermissionsGate that `engine_host.rs:119` is missing (studio#362).
  - Drive passes the token to authenticated peers along with `/engines`.
  - A typed-in address asks for it.
  - The MCP door on loopback also requires the token (`FTS_MCP_TOKEN`
    already exists, `mcp.rs:17-20`), so one instance's MCP client cannot
    reach another instance's door by guessing ports.

### Isolation

One path helper in studio-core, `instance::Paths`, gives each process its
dirs. Nothing else joins `config_dir()/fts` for state the engine writes.

| Kind | `main` | Other instances |
|---|---|---|
| Runtime (sockets, lock, tmp) | `$XDG_RUNTIME_DIR/studio/main/` | `…/studio/<name>/`; the engine sets `TMPDIR` to `…/tmp` for itself and its children |
| State (last-open, recent, overrides, collab lock) | `$XDG_STATE_HOME/studio/main/` | `$XDG_STATE_HOME/studio/<name>/`, deleted by `just engine stop --clean` |
| Log | `$XDG_STATE_HOME/studio/main/engine.log` | per instance |
| Settings (audio device, hit detect, render worker, Signal config) | read and written as today | **read** from the shared file, **written** to the instance's state dir, which wins on the next read. The owner's defaults never change because of an agent |
| Audio device | saved choice | `--audio null` by default for `owner.kind = agent` (offline renders and `listen` still work). `--audio default` to hear it |
| Song file | open for writing | **One writer per `.session`.** The registry's `song` is a claim: an engine refuses to open for writing a song that another live instance holds, and opens it read-only, or as a copy with `--copy`, which puts it in the instance's state dir |
| systemd unit | `studio-engine@main` | `studio-engine@<name>` |
| Telemetry | `service.name` unchanged | Adds `service.instance.id=<name>` and `studio.owner=<id>` on every span (engine, worker, app). OpenObserve filters by them, and the dashboards keyed on `service.name` keep working |

**Shared, read only:**

- sample libraries, rig models and `SIGNAL_LIBRARIES_DIR`;
- `tts-cache` and `Session/streamed`: keyed by content and version, written
  with write-then-rename;
- kache;
- the render cache: in memory today. If it moves to disk, key it by
  content hash and share it.

**Build replacement** (collision 5) applies only when the attaching build's
`exe` is in the same worktree as the running engine's. For anything else,
the UI shows "other build" and connects anyway.

### The UI's engine picker

- **Where.**
  - The model, a list of `EngineEntry`s plus `connect(addr)`, lives in
    `crates/studio-connect/client/engine-client`.
  - The view lives in `crates/studio-connect/views/link`, which already
    handles attach and spawn (`native.rs:40`).
  - The app shows an engine chip in the title bar, and `ENGINE_PICK` in
    the Action List.
- **What it shows.** One row per engine, local first, then remote by
  machine. Each row shows:
  - name and owner (human or agent thread);
  - song or set;
  - its age, and whether it is alive;
  - this build or another;
  - whether it is the one connected.

  The last row is **"No engine (view only)"**.
- **Switching.** The UI drops the link to the old engine and keeps the
  views on the session document (Loro, synced by Drive). It then links the
  new engine and replays transport and meters. It never stops the engine it
  leaves; stopping is a separate action, allowed only for an engine this
  UI started or one owned by `human`.
- **Start-up.** The UI connects to `STUDIO_INSTANCE`'s engine, or `main`.
  - `FTS_ENGINE=spawn` (the default) starts that instance if no engine of
    that name is running.
  - `FTS_ENGINE=attach` waits.
  - The UI never takes "the newest engine". Both `ours()` heuristics and
    the private `XDG_RUNTIME_DIR` in `run.sh` go.
- **No-engine mode.** The session opens from its `.session` document. It
  has no transport or meters, and its actions that need the engine are
  shown disabled.

### MCP

- **Resolution**, in order:
  1. `--socket` or `STUDIO_MCP_SOCKET`: explicit, as today.
  2. `--instance <name>`.
  3. `STUDIO_INSTANCE`.
  4. `<worktree>/.studio-instance`, a gitignored file that `just engine start`
     writes.
  5. `main`.
- **What it talks to.** studio-mcp reads the registry entry and drives the
  **engine's MCP door** with its token, so no window is needed. When the
  instance's window is running (`studio-<name>.sock`), it goes through the
  window instead, which adds the UI's own actions. It re-resolves the
  entry on every reconnect, so an engine restart on a new port is
  followed.
- **One studio-mcp per agent**, which `.mcp.json` starts in the agent's
  worktree. Two new tools:
  - `engines`: the registry list;
  - `engine_use {name}`: switches this server's instance for the rest of
    the session, for when the server started before the engine.
- **No instance given and `main` not running:** each tool answers `no
  engine "main" is running; running: keyflow-a3f1, dsp-07c2 (engine_use to
  pick one)`. It never falls back to another instance. Every answer carries
  `instance` beside `revision`, so a transcript shows which engine was
  changed.

### Workflow

```bash
just engine start kf --set ~/Drive/Studio/Setlists/Crescendum.setlist --song unbreakable --copy
just engine list                 # name, owner, song, age, addr, build (this worktree?)
just engine stop kf [--clean]    # control.sock stop; --clean drops its state dir
just engine logs kf
just studio --engine kf          # STUDIO_INSTANCE=kf for the window; no XDG_RUNTIME_DIR tricks
just studio                      # main, as today
```

- `just engine start` runs **this worktree's** engine binary and writes
  `.studio-instance`. An agent's worktree and its `target/` therefore come
  with its own engine, built from its own code.
- **An agent's brief says:**

  > Use engine instance `kf` (`just engine start kf --set … --copy` if
  > `just engine list` doesn't show it). Drive it only through the `studio`
  > MCP, which reads `.studio-instance`. Never stop or open songs on
  > `main`. Scratch files go in `$XDG_RUNTIME_DIR/studio/kf/tmp`.

- **Several agents in one checkout**, like the `monorepo` worktree today,
  cannot share `.studio-instance`. Each sets `STUDIO_INSTANCE` in its
  thread's environment instead, or passes `--instance` in its MCP config.
- `scripts/agent-instance/` shrinks to `shot.py` and `mcp.py`, which take
  `--instance`. `stack-dev/stack/restart-all.sh` becomes
  `just engine stop main && just studio`.

## 4. Migration

Each task is one PR-sized commit that keeps `just check` green. Tasks
marked ∥ can run in parallel once T1 has landed.

| Task | Group | What | Depends |
|---|---|---|---|
| **T1** registry and paths | core | A new `crates/studio-core/proto/engine-registry` crate holding `InstanceName`, `EngineAddr` (parse and display, `unix:`/`ws:`), `Entry`, `Paths` (runtime, state, tmp, log), `register()` (flock, atomic write, heartbeat task hook), and `list()` (clears stale entries). Unit tests for parsing, staleness and the atomic write. Moves `Found` and `publish` out of engine-client, which also removes studio-engine's use of `engine_client::studio_engine::link` (`studio_engine/mod.rs:36`), a core-to-connect edge. | none |
| **T2** ∥ engine serves an instance | engine | Changes three files: (1) `engine_host.rs`: name from `--instance` or `STUDIO_INSTANCE`, `facade.sock` and the lock in `Paths`, register with `Entry`, MCP on port 0 with a token, update `song`/`set` on open. (2) `owner.rs`: launcher dir = `Paths::runtime`, unit `studio-engine@<name>`, `Replace` only when the `exe` worktree matches. (3) `mcp.rs`: drop `DEFAULT_PORT` and `session-mcp.json`. | T1 |
| **T3** ∥ engine state per instance | engine | `last-open`, `recent.tsv`, `live-sync-locked`, `overrides.json`, the engine log and `TMPDIR` through `Paths`. Settings files read shared and written per instance. `--audio null` for agent owners. The song claim and `--copy`. `service.instance.id` on the OTel resource in `telemetry.rs`. | T1 |
| **T4** ∥ client connects by address | connect | engine-client lists `Entry`s and connects by `EngineAddr`, with the Unix socket and ws on the same vox code. Removes `of_this_instance`. `apps/studio/src/engine.rs` drops its own finder, `ours()`, `serves_its_own()` and `free_port()`, and uses engine-client. `instance.rs` defaults to `main`. | T1 |
| **T5** picker | connect | The `views/link` picker, the title-bar chip, `ENGINE_PICK`, and switching without stopping. The no-engine row. | T4 |
| **T6** ∥ studio-mcp per instance | connect | The resolution order, the direct-to-engine door with its token, `engines` and `engine_use`, and `instance` in every answer. Tests in `mcp/tests/bridge.rs` beside the `STUDIO_MCP_SOCKET` ones. | T1 |
| **T7** ∥ recipes and docs | tools | Justfile `engine start\|list\|stop\|logs` (replacing `engine ACTION`, `Justfile:43`) and `studio --engine`. `.studio-instance` in `.gitignore`. `scripts/agent-instance` trimmed. `drum-lab.sh` reads the registry. The brief template in `docs/agents/`. `apps/studio/docs/development.md`. The handoff's `restart-all.sh`. | T2 for running, T1 for writing |
| **T8** remote | engine + connect | `--listen tailnet` with the PermissionsGate and token (studio#362). The Drive node's `/engines` for peers. The picker's remote section. The plan's integration test: the UI links an engine on another host. | T2, T5 |
| **T9** cleanups | engine, tests | `/tmp` names in daw-bridge and tests go to `Paths::tmp` or `tempfile`. A one-off sweep of `~/.config/fts/engines/` and `$XDG_RUNTIME_DIR/fts/engine-*.sock`. | T2 |

**Order:** T1 (about half a day; land its API first). Then T2, T3, T4, T6
and T7 in parallel. Then T5 and T9. T8 is last.

**The collision list closes as follows:**

| Task | Collisions |
|---|---|
| T1 + T2 | 1, 2, 4, 5, 6 |
| T3 | 3, 7, 8, 9, 10, 11, 14, 15 |
| T9 | 12, 13 |

# Developing Studio locally

Studio (`apps/studio`, package `studio-app`, app and binary **Studio**) is
one Dioxus app for desktop, iOS and the browser:
- on the desktop, Blitz draws it through app-shell-native;
- it runs on Architect's application framework and app-shell-extensions.

Its features are extensions, compiled in, and any of them can be swapped
for a development copy while Studio runs (see [Working on an
extension](#working-on-an-extension)).

Everything below runs inside the repo's dev shell (`nix develop`, or direnv),
from the repository root unless it says otherwise. Use dx 0.8.0-alpha.3,
the version the app is built with.

## Build and run the desktop app

```bash
# The engine Studio starts (the `daw` extension's program, `--engine`)
cargo build -p daw-extension --profile desktop-dev

# The app
cd apps/studio
dx build --platform linux --renderer native --features native --no-default-features
../../target/dx/Studio/debug/linux/app/Studio
```

**The engine.**
- Studio finds the engine program, `daw-extension`, in this order:
  1. `FTS_DAW_ENGINE_BIN`;
  2. beside the app;
  3. the build's `target/<host>/desktop-dev/`, then `target/debug/`.
- Studio starts the engine when a session opens, and the engine outlives the window.
- Its log is `~/.cache/fts/studio-engine.log`.

**Checks and tests:**

```bash
cargo check -p studio-app --no-default-features --features native   # the desktop build
cargo check -p studio-app --features web --target wasm32-unknown-unknown  # the browser build
cargo test -p studio-app
just test                                                           # the repo's core test set
```

Never use `-p app-shell`: features/app/shell, the older apps' shell, has the
same package name.

**The web build.** It's served by a Drive node at `/studio`:

```bash
cd apps/studio && dx build --platform web --release --base-path studio
```

## iPhone and iPad

iOS runs no second process, so the engine is linked into the app: the
`daw` extension, linked like the views, starts it in the app's own
process, and Studio drives it over the in-memory link instead of loopback
(`src/engine.rs`, `engine::Door::Here`). On a Mac with Xcode:

```bash
apps/studio/ios/build.sh sim                       # a debug .app for the simulator
python3 -I apps/studio/ios/sim-test.py --app <it>  # Seam 3: a session opens and plays with the click
python3 -I apps/studio/ios/sim-room-test.py --app <it> --engine <daw-extension>  # Seam 2: it joins a room and follows
apps/studio/ios/build.sh device                    # a release .app for a phone
apps/studio/ios/deploy-iphone.sh                   # onto a phone on the cable
MARKETING_VER=<x.y.z> apps/studio/ios/deploy-testflight.sh  # to TestFlight
```

The simulator test boots its own simulator, opens a song it writes into
the app's container, and drives the engine through the app's MCP door,
which an iOS build opens only when launched with `FTS_MCP=1`. CI runs it
on airlock after every merge (`ios` in checks.yml).

**Rooms on a phone.** A room's link opens Studio in the app's own scheme,
`fasttrackstudio://join/<room>` (the URL type `build.sh` declares;
studio-shell's `ios_scene::on_link`), and Studio joins it
(`src/room_link.rs`); `Studio --join <link>` does the same at launch on
any native build. The engine linked into the app keeps and finds rooms in
the app's own Studio folder (`<home>/Rooms`, `FTS_ROOMS_DIR`), beside
the setlists a room loads. The room simulator test leads a room from a
desktop engine, copies the Studio folder into the app's container (as
Drive carries it to a device), opens the link on the simulator, and reads
both sides over MCP: the phone opens the leader's setlist, follows its
song, playhead and stop, shows in the leader's room, and moves to the
next setlist without reconnecting.

A phone starts with no environment, so it reaches a collector only when
the build bakes one in (`STUDIO_OTLP_ENDPOINT` and `STUDIO_OTLP_TOKEN` at
build time, studio-otel's `build.rs`, as the desktop release does). The
simulator test points the app at a receiver of its own instead and checks
that its spans arrive and that `--crash-test` arrives as a `crash` span.

## What a session needs

- **Songs and setlists** are plain files in the Studio folder
  ([storage.md](storage.md)). Drive syncs them when it is installed, and is
  never required.
- **Ignition's visualizer and cue list** currently read their show from the
  environment Studio starts with. FastTrackStudios/Ignition#64 replaces
  this with settings and the song's session.

  | Variable | What it is |
  |---|---|
  | `IGNITION_BIN` | the room's renderer program (Ignition's `ignition` binary) |
  | `IGNITION_ROOT` | Ignition's `features/lighting` folder |
  | `IGNITION_LIVE_SET` | the show's Ableton Live set |
  | `IGNITION_CUES` | the folder of the show's cue files (`<lane>.json`) |

  Without them the session opens, and the Lighting window is empty.
- **Window placement** (the three-window Stage) is the desktop's job:
  - The windows are titled `Studio — <set>`, `Lighting — <set>` and `Chart — <set>`, plus `Launcher`.
  - On KDE with Loft, window rules matching app id `Studio` and those titles put them on their displays.
  - Studio itself only lays out its panes.

## Driving it without the mouse

Every feature is an action on Studio's Action List, served three ways:

- **MCP for an agent**: `studio-mcp` over stdio, on the control socket (no port, no token, survives restarts), with curated mixing tools, compact answers, pages and jobs. `.mcp.json` names it as `studio`; see [mcp.md](mcp.md).
- **MCP over HTTP**: on loopback with a bearer token made fresh each launch. Where it is, and the token, are in `~/.config/fts/studio-mcp.json` (mode `0600`). Never print or commit the token.
- **The control socket**: `$XDG_RUNTIME_DIR/fts/studio.sock`, this user only.
- **Luau**: `SCRIPT_RUN` runs a script over every action as one step.

Examples:
- `STUDIO_SESSION_OPEN {"set": "Setlists/<name>.setlist", "song": "<title>", "play": true}`
- `STUDIO_ENGINE`
- `STUDIO_HOME`
- `EXT_LIST`
- `EXT_VIEWS`, the process views' frame times.

Agents test Studio through these actions, **never by moving the pointer or
typing on the desktop**: the person using the machine is working on it.

## Working on an extension

The arrangement, mixer, lyrics, navigator, Keyflow (the chart) and Ignition
(Lighting) are linked into Studio and run in its process at full speed. To
work on one, override it with a development copy **while Studio runs**.
This works in any build, release included:

```text
STUDIO_EXT_DEV      {"folder": "<checkout>/features/extension"}   # Keyflow
STUDIO_EXT_DEV      {"folder": "<checkout>/features/lighting/pane"} # Ignition
EXT_LOGS            {"id": "keyflow"}                               # its build
STUDIO_EXT_DEV_LIST
STUDIO_EXT_DEV_END  {"folder": "<the same folder>"}                 # back to the linked copy
```

**What happens:**
- Studio builds the crate's process program with `cargo build --bin <entry>` and swaps it in for the linked copy once the first build is good.
- Every save rebuilds it and swaps again, including a save to any local crate it builds from: its workspace's members and its path dependencies.
- A failed build keeps the last good copy, and `EXT_LOGS` shows the errors.
- A folder holding a built package (its `extension.toml` and program, no `Cargo.toml`) runs as it is and reloads when its files change.
- Overrides are kept in `~/.config/fts/studio-extensions/dev/overrides.json` and come back at every start until `STUDIO_EXT_DEV_END`.
- Builds go to `STUDIO_DEV_TARGET`, or to the overrides folder's `.target`.
- `STUDIO_DEV_EXTENSIONS` (`:`-separated folders) adds overrides for one run.

**Requirements and limits:**
- An extension can only be overridden if it ships a process copy:
  - `[entry]` and `[source]` in its `extension.toml`;
  - a `main.rs` serving the same `linked::Extension` with `Guest::extension(..).serve()`.

  Keyflow and Ignition have one. Studio's own extensions don't yet (#671).
- A process copy's Dioxus views need app-shell's remote views (FastTrackStudios/app-shell#35); until then its pane says it is drawn by its process.
- GPU process views (`kind = "process"`, like Ignition's visualizer) draw either way.

When the work is done, merge it in its repository, bump Studio's pin, and
end the override: the linked copy runs again, at full speed.

## Telemetry and crash reports

The app (`studio-app`), the engine (`studio-engine`) and a page in a
browser (`studio-web`) send their spans over OTLP, on plain `tracing` and
OpenTelemetry's crates (`crates/studio-core/util/studio-otel`). Every span
carries which build sent it: `service.version`, `studio.platform`
(`linux`, `macos`, `web`), `host.arch` and `deployment.environment`.

- **Where they go:** the process's `OTEL_EXPORTER_OTLP_ENDPOINT` (the
  local collector on 127.0.0.1:4318, read in OpenObserve on :5080) if it
  is set. Otherwise a release build sends to the collector it baked in:
  `desktop.yml` sets `STUDIO_OTLP_ENDPOINT` and `STUDIO_OTLP_TOKEN` from
  the organization's `TASK_OTLP_ENDPOINT` variable and `TASK_OTLP_TOKEN`
  secret. A plain build with neither sends nothing.
- **A crash** is a `crash` span (`crash.thread`, `crash.at`,
  `crash.message`, status error), flushed before the process goes.
  `Studio --crash-test` crashes on purpose once telemetry is up: run it
  with the local collector named and look for `operation_name = 'crash'`.
- **In a browser** the page posts its spans as OTLP/JSON to its node
  (`/api/otlp/v1/traces`), which passes them on to the collector the
  node's own `OTEL_EXPORTER_OTLP_*` names. A panic posts its crash span
  at once (`keepalive`); a failed call is a span with status error.

## Co-developing Studio and another repo

Studio takes Architect, app-shell, daw, session, Keyflow, Ignition, Drive
and Signal from GitHub by `rev`. To build Studio against local checkouts
of them, put `[patch]` tables with paths in `apps/studio/.cargo/config.toml`.

**That file is local:**
- never commit it, nor the `Cargo.lock` it produces (`git checkout Cargo.lock` when done);
- the paths are your machine's.

Patch every crate Studio builds from that repository, not only the one you
edit, or two copies whose types don't unify end up in the graph. The repo's
AGENTS.md lists the daw table; Architect needs both its `github.com` and
`www.github.com` spellings.

When the change is merged there, move Studio's `rev` (every line naming
that repository, in the root `Cargo.toml` and `apps/studio/Cargo.toml`),
check that `cargo check -p studio-app --no-default-features --features
native` passes without the config file, and commit the new lock.

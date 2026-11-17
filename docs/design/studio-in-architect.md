# Studio in Architect: one app

Status: phase 1. This doc, plus the first slice (`features/studio-daw`).
Owner direction: November 2026.

> "Studio desktop should just be entirely in Architect, using the
> Architect system. The DAW view is just the entered view of the thing."

Today there are two programs:

- **`studio-desktop`** (`apps/studio/desktop`). Its own Blitz window has:
  a welcome screen (the library), the DAW view (Session's shell), the rig
  view (Signal), dock panels, a command layer (⌘K, ⌘O, `?`), an app menu,
  and a link to a headless engine.
- **Studio Architect** (`apps/architect`). This is the Architect app with
  Studio's extensions compiled in: setlists, chart, lyrics, productions,
  studio-views, Lighting (`ignition`), and the island.

The target is one program: **Studio = Architect + Studio's extensions**.

- The library is Architect's vault and navigator, plus the setlists
  extension.
- Opening a setlist, a song or a session *enters* the DAW view. The DAW
  view is a linked extension view attached to the Studio engine.
- The rig view and Lighting are extension views too.
- Studio's actions join Architect's ⌘K, action list, CLI and MCP.
- At the end, `studio-desktop`'s window goes away. The engine binary and
  the `studio` CLI stay.

## Decisions

1. **The DAW view is a linked extension**: `features/studio-daw`, id
   `studio-daw`, with an `extension.toml`, a `[source]`, and a process
   copy (`src/main.rs`). This is architect-extensions Rule 4.
   - The view mounts `session_desktop::native::Embedded`, Session's
     shell as Studio's window mounts it today. It is reused, not
     rewritten.
   - It opens `placement = "zoomed"`, alone in the workspace. That is
     the "entered" view.
2. **Enter = resolve the note, open the set, zoom the view.**
   `STUDIO_DAW_ENTER {note}` works like this:
   - It reads the note. A setlist's `songs:` gives the running order.
   - It reads each song's `session:` property.
   - It writes the set as a `.setlist` file (one absolute path per line)
     in the cache, and hands that file to Session
     (`handoff::request(Entry::Path)`). The engine opens it as the set.
   - It opens the view zoomed.

   Entering runs from several places, all through the same action:
   - a setlist or song note's **DAW** favourite tab;
   - ⌘K;
   - a `.session` file in the navigator (an `[[openers]]` with
     `files = ["*.session"]`);
   - MCP or the CLI (`studio-architect actions run STUDIO_DAW_ENTER`).
3. **Song → session link: the song note's `session:` property.** It can
   be:
   - an absolute path;
   - a `~/` path;
   - a path relative to the song's note;
   - a wiki-link into the vault (`[[Libraries/…/Fire.session]]`).

   The rules are in `studio_daw::resolve::session_path`. A song without
   one is reported in the answer's `missing` and left out of the set. An
   arrangement's own session (`#Live`, `#Extended`) is later work; for
   now the DAW plays the song's session.
4. **Back is Session's own Back button.** `handoff::Host { on_leave }`
   runs `STUDIO_DAW_LEAVE`: it stops the transport, unzooms and closes
   the pane. You land in the workspace you came from, which is the
   library: the navigator and the setlist note. The engine keeps its set
   for the next entry.
5. **One engine per process, in another process.** This follows
   docs/design/one-engine.md and docs/design/engine-boundary.md.
   - Studio Desktop is itself the headless engine binary:
     `studio-architect --session-engine`, or `$FTS_STUDIO_BIN engine`.
   - At launch, before any thread, it **links** its engine:
     `studio_daw::engine::link_at_launch` (`link::choose`: attach to a
     running one, else spawn one in its own process group).
   - It sets `STUDIO_ENGINE_SOCKET`, so the setlist's playhead clock
     (`setlists::engine_clock`) and Lighting follow *this* engine. Without
     it they follow the newest socket in `$XDG_RUNTIME_DIR/fts`, which
     might belong to another Studio.
   - `FTS_ENGINE=spawn|attach|off` chooses how the engine is had.
6. **Studio Desktop is the server; Studio Web is the same app on the
   web.** Both are developed with `dx serve`; `devenv up` runs exactly
   these two processes.
   - **Studio Desktop**: `dx serve -p studio-architect --bin
     studio-architect --platform native --profile desktop-dev --target
     <host>`.
     - With `STUDIO_SERVE=1`, the window's process runs the
       distribution's node (`studio_architect::serve_beside`): vault,
       sync, share links, and the web app's vox. It reads the same
       `ARCHITECT_*` configuration as `serve`, and runs on a thread of its
       own. There is no separate node process in dev.
     - It spawns the engine as a separate process. A hot patch, a
       rebuild or a relaunch of the window re-attaches to it rather than
       restarting it. For that to hold across *rebuilds*, set
       `FTS_STUDIO_BIN` to a stable `studio` CLI. Without it, a window
       of a new build stops an engine of the old build and starts its own
       (`link::Plan::Replace`).
   - **Studio Web**: the same app's web build, proxied to Studio
     Desktop's node. For now this is Architect's `apps/web` (`cargo xtask
     dev run web`), until Studio has a web package (gap G7).
7. **The share-link rule holds.** A share page *is* the web app, opened
   as a guest: the same app, the same extensions, permissions off.
   Nothing is share-only.
   - The DAW view on the web is Session's shell drawn by the web build,
     which is the same component. Session already renders in a browser:
     `session_ui::web_host` replays a `Widget`'s `anyrender::Scene` on a
     canvas.
   - A guest sees what the share's permissions allow. Until the engine
     is reachable from the web (G6), the web shows the setlist, Chart,
     Lyrics and the island's transport, and the DAW tab is left out
     because its action is missing there. That is the favourites' own
     rule.
8. **iOS: the engine runs in-process.** iOS allows no child processes, so
   Architect's process extensions do not run there.
   - `session_desktop::native::prepare` already handles this: on iOS,
     `link_engine` starts the engine in the app's process and links the
     window over an in-memory link (`open::link_window_here`). The
     window's client is the same either way.
   - `studio-daw` is linked Rust, so it builds into the iOS app as it is.
     Only `link_at_launch` and `--session-engine` are desktop-only (they
     depend on `link`'s spawn).
   - The rig view does the same for the rig engine
     (`rig_engine::bootstrap_blocking`).
   - "One engine per process" on iOS means the app's process *is* the
     engine process. It does not outlive the app.

## Inventory: what `studio-desktop` does, and where it goes

| Today (file) | What it is | Goes to |
|---|---|---|
| `front.rs` `Studio`, `View::{Library, Daw, Rig}` | The window's root: welcome, DAW, rig | Architect's shell. `View::Daw` becomes the `studio-daw/daw` view, zoomed; `View::Rig` becomes a `studio-rig` view; `View::Library` becomes the workspace and navigator |
| `library_home.rs`, `features/library` (Task orgs → `library::task`) | The library: songs, setlists, sessions, parts, the ways in | **Gone**: Architect's vault and navigator plus setlists' providers (Setlists, Libraries). The ways in (new, join, demo, recent) become actions: `STUDIO_DAW_NEW`, `STUDIO_DAW_JOIN`, a Recent ⌘O source (`[[openers]] kind`). Task is dropped (memory: architect-replaces-task) |
| `front_state.rs`, `Preset` (`FTS_STUDIO_OPEN`) | What a launch opens | A launch argument that runs `STUDIO_DAW_ENTER` / `EXT_OPEN` (`studio-architect --open <note>`), via Architect's `architect://open` links |
| `CommandLayer`, `WindowMenuKeys` (front.rs) | ⌘K, ⌘O, `?` over every view; Session's Action List in ⌘K | Architect's palette. Session's Action List joins it as an `[[palette_sources]]` or as actions `studio-daw` registers from `handoff::actions()` (G3) |
| `mac_menu.rs`, `session_ui::app_menu` (Linux global menu) | The app menu | Architect's menu, with extension-contributed menu items (G4) |
| `session_desktop::native::{prepare_hosted, Embedded}` | Session's side of the engine; Session's shell | **Reused as they are** by `studio-daw` (this slice) |
| `session_engine::studio_engine::link` (spawn/attach, `Found`, `keep_linked`) | The engine in another process | Unchanged. Linked at launch by `studio_daw::engine` |
| `main.rs` `--session-engine`, `engine_main.rs` (`--engine`, the rig engine), `bin/studio` (the CLI) | Engine binaries | `studio-architect --session-engine` (this slice). The `studio` CLI stays. `--engine` (the rig engine) is folded into the one engine (one-engine.md) or kept as the rig view's process |
| `rig_view.rs`, `rigs.rs`, `keys_view.rs`, `ekit_view.rs`, `mobile_view.rs`, `space_view.rs`, `libraries_view.rs`, `rig_engine.rs`, `engines.rs`, `engine_watch.rs`, `pack_client.rs`, `web_packs.rs`, `engine_tone3000.rs` | The rig view (Signal) and its engine supervision | A `studio-rig` linked extension: views `rig`, `keys`, `ekit`, `libraries`; the rig engine as now. Packs and TONE3000 become its actions and settings |
| `front/rig_panels` (`HostPanels`, the rig's panels in Session's dock) | Rig panels inside the DAW | Stays in Session's dock (inside the DAW view) |
| `account.rs`, `WelcomeAccount*` (Task sign-in) | The account and org | Architect's accounts (`architect-accounts`). The FTS account becomes an Architect account; the org is the vault's org |
| `sync.rs`, `books.rs`, `peers.rs` (rig books over `crdt::docs`, the mesh) | Sync and peers | Architect's sync (`.architect/`, the node) and Architect p2p. The rig books become docs of the vault (memory: architect-replaces-task) |
| `remote.rs`, `web_base.rs`, `web_keys_*` | Dialing the engine; the browser keys rig | The web build's engine client (G6) |
| `menubar.rs` (macOS menu bar, every engine) | Engine processes at a glance | An Architect tray item contributed by `studio-daw` (Architect has `Icon::tray`) |
| `ios_audio.rs`, `ios_orientation.rs`, `ios_safari.rs`, `mac_activity.rs` | Platform glue | `features/app/shell` or Architect's mobile app hooks |
| `session_desktop::native::publish` (Publish to Task) | Ingest a session into the library | An action, `STUDIO_DAW_PUBLISH`: it writes the song note and its `session:` |
| `studio-views` toolbars (Arrange, Mixer, Editor refusing "needs Studio's engine") | Studio's views bar | They open Session's panels inside the DAW view (`STUDIO_VIEWS_ARRANGEMENT` → enter, then the screenset). Later, each panel becomes a view of its own |

What each kind of piece becomes:

- **Extensions**: `studio-daw`, `studio-rig`, plus the existing
  `setlists`, `studio-chart`, `studio-lyrics`, `productions`,
  `studio-views`, `ignition`.
- **Views**: `daw` (zoomed), `rig`, `keys`, `ekit`, `libraries`, `setlist`,
  `song`, `chart`, `lyrics`, `lighting`.
- **Settings**: the audio device and mode (`session_engine::open::launch_mode`)
  and the engine mode (`FTS_ENGINE`) become `[[settings]]` of
  `studio-daw`. The rig profile becomes `studio-rig`'s.
- **Actions**: everything in Session's Action List, prefixed (G3);
  `STUDIO_DAW_ENTER`, `LEAVE`, `NEW`, `JOIN`, `PUBLISH`.
- **Services**: the engine's vox services stay the engine's. Architect
  extensions reach them through `studio.daw` (`[provides]`) later.

## The engine in the app

| Event | Desktop | iOS |
|---|---|---|
| App launches | `link_at_launch`: attach, else spawn `<bin> --session-engine` (own process group); sets `STUDIO_ENGINE_SOCKET` | Nothing yet |
| A setlist or song is entered | `prepare_hosted` once (attach again, serve the Action List), `handoff::request`; the engine opens the set | `prepare` starts the engine in-process (`link_window_here`) |
| Window hot-patched (`dx serve`) | Engine untouched | n/a |
| Window rebuilt or relaunched | Re-attach (`FTS_STUDIO_BIN` set); else a new build replaces the engine | App relaunch restarts the engine |
| Back | Transport stops; the set stays open in the engine | Same |
| Quit | The engine keeps running (its own process group); the next launch attaches | The engine ends with the app |
| The engine dies | `link::keep_linked` starts another and re-attaches | n/a |

## What Architect lacks (gaps, with files)

- **G1. GPU limits for the window.**
  - Architect's window asks wgpu for default limits
    (`architect/features/workspace/src/native.rs` `window_renderer`:
    `with_features_and_limits(…, None)`).
  - Session needs `session_ui::gpu::window_limits()`: a long song's
    arrangement needs more than the 128 MiB storage buffer, and without it
    the window panics in `create_bind_group`.
  - Fix: `Distribution::gpu_limits(wgpu::Limits)`, read by
    `launch_with_fonts`.
  - Recital-sized songs work without it; Unbreakable (25 857 items) will
    not.
- **G2. Painted widgets in a linked view: works; in a process view: no.**
  - A linked view is in the app's document. Studio Architect links one
    `blitz-dom` (FTS blitz `db5b7ef`) and one `dioxus-native`, so
    Session's `blitz_dom::node::Widget`s (the arrangement, meters) paint
    into the window's scene as they do in Studio's window.
  - The process copy (`studio-daw` as a process extension in the stock
    Architect app) cannot carry them. A remote view sends DOM, not
    `Widget`s. A `paint` view could carry a scene, but Session's widgets
    are many, interactive, and tied to wheel and pointer state
    (`features/extensions/src/scene.rs`).
  - So the DAW is first-party, linked-only, until remote views can host
    `Widget`s.
- **G3. Keyboard focus and shortcut routing for a DAW.**
  - Session routes keys itself at the winit level: `session_ui::keys`,
    `use_window_event`, the overlay switch (`handoff::keys_taken`), and
    `typing()`.
  - Architect has its own keymap and palette
    (`architect/features/workspace/src/host/commands.rs`, the palette
    extension).
  - Both hear the same window events, so Space, arrows and letters can
    reach both. What is needed: a view-scoped *key capture*, so that while
    the zoomed DAW pane is focused, Architect's single-key bindings stand
    aside and ⌘K, ⌘O and ⌘W stay Architect's. In Architect terms, the
    view's manifest says `keys = "capture"`, and the workspace asks the
    focused view first.
  - Session's Action List should also appear in ⌘K. That needs either
    `[[palette_sources]]` backed by `handoff::actions()`, or dynamic
    action registration for a linked extension (today, actions are
    declared in the manifest).
- **G4. Menus.**
  - Session publishes a global app menu (`session_ui::app_menu::start()`,
    D-Bus on Linux) and `mac_menu.rs` builds AppKit menus. Architect has
    no contributed menu bar.
  - Today `prepare()` still starts Session's D-Bus menu inside Studio
    Architect, so it may show beside Architect's own.
  - Needed: a `[[menus]]` contribution, or the extension's actions
    grouped under a top menu.
- **G5. A full-window entered view.** Partly there: `placement =
  "zoomed"` (`architect/features/extensions/src/interface.rs`
  `open_in_workspace` → `WORKSPACE_ZOOM`) fills the workspace. Still
  missing:
  - The shell's own chrome (title row, docks, status bar) still shows
    around it.
  - Unzooming is the workspace's action, not the view's.
  - Session's shell has its own top bar, so there are two bars.

  Needed: a `zoomed` mode that also hides the shell chrome
  (`chrome = "none"`), with the view's Back returning to the zoom state
  that came before.
- **G6. The engine from the web.**
  - The web build has no engine. The window's link is a unix socket
    (`view_client` over `$XDG_RUNTIME_DIR/fts/engine-<pid>.sock`).
  - The engine can serve the same router over WebSocket
    (`engine_host --listen`, `Found.ws`).
  - Needed: Studio Desktop's node proxies a guest's or the owner's web
    app to the engine's `/vox`, gated by Architect permissions. The
    engine is ungated today (studio#362).
- **G7. A Studio web package.** Studio Web is Architect's `apps/web`,
  whose `Dioxus.toml` proxies to `127.0.0.1:7412`, Architect's node port.
  - It does not compile Studio's linked extensions, so the web shows
    Studio's fences only as the node draws them.
  - Needed: `apps/architect/web` (`studio-architect-web`), with Studio's
    linked extensions and its own `Dioxus.toml` and port. Then the
    desktop's node and Architect's own dev stack can run side by side.
- **G8. An in-process node in the stock app.** `architect-desktop`'s
  windowed `run` (`architect/apps/desktop/src/app.rs`) starts no node;
  `serve` is a separate verb. For now `studio_architect::serve_beside`
  calls `node::start_with_shell` itself.
  - Architect should own this: `Distribution::serve_beside()`, with a
    clean stop on quit.
  - Open question: the window's own vault (`ARCHITECT_VAULT`) and the
    node's (`ARCHITECT_VAULTS`) are two stores on one folder. Both write
    notes. Until the node serves the app's own store, dev points both at
    the same folder and accepts that the node sees the app's writes only
    as file changes.
- **G9. Single instance.** `architect_presence::single_instance` hands a
  second launch to the running one, for each `instance_id`. A dev Studio
  Desktop beside the user's must set its own config folder,
  `STUDIO_ARCHITECT_CONFIG_DIR`, which already changes the id
  (`Distribution::instance_id`).

### Found by the first isolated run (2026-11-15)

These showed up in a headless Loft session (`dawslice`) with a vault
copy, a silent engine and a private config folder.

- **G10. No Tokio runtime on Architect's UI thread.**
  - Session's shell awaits Tokio timers on the UI thread
    (`architect::platform::sleep` → `tokio::time::sleep`).
  - `dioxus_native::launch_cfg` (Studio's window) enters a runtime there;
    `architect_workspace::native::launch_with_fonts` doesn't. The window
    panicked with "there is no reactor running".
  - Worked around in `apps/architect/src/main.rs`: with `daw`, a
    multi-thread runtime is entered on the main thread for the window's
    life.
  - The right fix is in Architect: the launcher enters a runtime, as
    dioxus-native's does.
- **G11. Two rustls backends.** Linking Session into Architect turns on
  both `ring` and `aws-lc-rs`, so rustls refuses to pick a default, and
  Session's network threads panicked. Fixed in `main`: ring is installed
  as the process default first thing.
- **G12. A window resize panics in Stylo**
  (`ElementStyles::viewport_unit_usage` unwrapping `None` during
  viewport-unit invalidation, `blitz_dom::BaseDocument::set_stylist_device`).
  - Seen on fullscreen in the nested session, with and without the DAW
    pane.
  - This is a Blitz/Stylo bug (FTS blitz `db5b7ef`) to reproduce in
    Architect alone.
- **G5, confirmed.** `placement = "zoomed"` opens the pane, but
  `WORKSPACE_ZOOM` answers `changed: false` when the pane is already the
  only center group. The docks (navigator, the View dock) stay. "Entered"
  needs a mode that hides the docks.
- **G4, confirmed.** Session's D-Bus app menu (`session_ui::app_menu::start`
  in `prepare`) takes over the global menu bar from Architect's (File,
  Edit, View, Transport, Drums, Library, Set, Streaming).
- **A failed attach quit Architect.** `prepare_hosted` calls
  `process::exit(0)` when its "engine did not start" dialog has no
  answer, and a headless session has nobody to answer it.
  - Fixed: `session_desktop::native::prepare_in_host` returns `None`
    instead, and the DAW view says the engine did not start.
  - The trigger was a binary rebuilt under a running window: its build id
    changed, so `link` replaced the engine (`Plan::Replace`). Under `dx
    serve`, set `FTS_STUDIO_BIN`.
- **The spawned engine dies with a systemd unit.** `process_group(0)`
  doesn't leave the unit's cgroup. An engine meant to outlive the window
  is its own unit (as `devenv up` runs it), or is spawned with
  `systemd-run --scope`.
- **`FTS_ENGINE_SILENT=1` still tried ALSA.** It failed only because the
  isolated session has no audio device ("Host is down").
  `session_engine::open` sets `AUDIBLE` from more than the flag (lines
  ~174/196). Fix before any test runs where a device exists.
- **Open: the shell does not paint in the pane yet.**
  - Entering resolves the set (18 of 18 sessions) and opens the pane.
  - On the first run (before G10's fix), the window attached and the
    engine opened the set: it loaded the songs' media.
  - With the runtime entered on the UI thread, the window prints
    "Connecting to …engine.sock", but the engine sees no RPCs, and the
    pane (`APP_SHOT`) is empty: not even Session's start screen.
  - Next suspects: the runtime enter changing how `view_client`/`daw`'s
    `block_on` behave on the UI thread; Session's root sizing in a pane
    (`height: 100%` with no sized parent).

## Migration, in shippable steps

1. **This slice.**
   - `studio-daw`: enter a setlist or a song, transport, Back.
   - `--session-engine`, engine link at launch, `STUDIO_ENGINE_SOCKET`,
     `STUDIO_SERVE`, and the `dx serve` Dioxus.toml.
   - `session:` on song notes.
2. **`session:` written by the tools.**
   - `studio productions` and the setlist import write each song's
     `session:` when they split a show.
   - `STUDIO_DAW_PUBLISH` writes it for a session ingested from Session.
3. **Architect: G1 (GPU limits) and G9 (instance per config).** These
   are small PRs in architect. Bump the tag.
4. **Keys and actions (G3).**
   - Architect: view key capture.
   - `studio-daw`: Session's Action List in ⌘K and MCP, through a palette
     source and the app's action list, with the `SESSION_*` ids kept.
5. **Entered view chrome (G5)** and **menus (G4)**. The DAW looks like
   Studio's window, and Session's D-Bus menu goes.
6. **The rig view as `studio-rig`.**
   - `rig_view`, keys, ekit and libraries become views, with the rig
     engine supervised as now.
   - Account and peers move onto Architect's accounts and p2p.
7. **Studio Web (G7) and the engine from the web (G6).**
   - The `studio-architect-web` package. The node proxies the engine's
     `/vox`, behind permissions.
   - The DAW view on the web, for the owner and, read-only, for guests.
     It is the same component; nothing is share-only.
8. **iOS.** Studio Architect's iOS build with `studio-daw` linked (the
   in-process engine through `prepare`). It replaces the Studio iOS app
   in TestFlight.
9. **Retire `studio-desktop`'s window.**
   - `apps/studio/desktop` keeps the `studio` CLI and the engine binary.
   - `front.rs`, `library_home.rs`, `features/library`'s Task reader and
     the welcome screen go.
   - `studio-architect` is renamed **Studio**: binary `studio-desktop`,
     bundle id `app.fasttrackstudio.studio`, adopting Studio's prefs
     folder (`Distribution::adopt_folder`).

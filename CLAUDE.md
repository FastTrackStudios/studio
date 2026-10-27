# studio — Repo Instructions

**studio is FastTrackStudio's audio/music product in one repo**: the DAW
(`daw`) and everything built on it — **Studio, the one app** (Signal, the
live rig, and Session, the full DAW, as views of one project behind one
welcome screen, the library), Patchbay
(studio routing), the CLAP/VST3 plugins, and frame (the design /
control-surface engine). It is the merger, in October
2026, of the former session, daw, processor, signal, patchbay and frame
repos. One Cargo workspace, one lockfile, one `target/`, one flake.

## Layout

```
apps/       studio/desktop                     Studio, THE app (desktop + iOS, one
                                               TestFlight upload): its welcome screen
                                               (the library) opens the DAW view or
                                               the rig view; the rig app
                                               crate (`studio-desktop`, and its
                                               headless `--engine`)
            session/{desktop,daw,web,daw-web}  Session: its app as a library Studio
                                               mounts (+ its own binary and CLIs), the
                                               DAW view, the site, the DAW in a browser
            signal/{mobile,web}                Signal: Android, the site
            patchbay/                          fts-patchbay: app, agent CLI, browser remote
            plugins/*                          the CLAP/VST3 plugins (`<name>-plugin`),
                                               verify/ (bundle load checks)
            themer/
crates/     daw/*      the DAW alone: the domain core (`daw`), proto, control,
                       the standalone engine (`daw-standalone`), audio-graph,
                       allocator, transport-sync, actions, module, utils
            studio/    the umbrella over daw: REAPER adapter, project files,
                       UI, CSI, the daw CLI, the integration-test harness —
                       all cargo features of `studio`
            frame/     the design / control-surface engine (+ tools/: cli,
                       desktop, mcp, library-server)
features/   one crate per feature, sub-folders allowed. Name a feature for what
            it is; anything host-specific goes under adapters/<host>.
            signal/      Signal's domain: rigs, presets, scenes, its browser,
                         storage, controller, grid, macros (facade at the root)
            rigs/        the instruments — guitar, keys, drums, ekit, synth,
                         orchestra — plus host/ (rigs on the one engine) and
                         proto/ (the shared RigCore service)
            dsp/*        the effects and their engines (+ nam, analyzer, …)
            editor/      the editor: piano roll, drum lanes, curves
            timeline/    the editing engine the arrangement and the editor
                         share (docs/design/timeline.md)
            editing/     editing a recorded performance — quantize, groove,
                         leveling — with a profile per material, drums
                         first (docs/design/editing.md)
            session/, song/, sampler/, plugin-host/, audio/, midi/, input/,
            sync/, chords/, formats/, patchbay/, control-surfaces/,
            dynamic-template/, patch-list/, preset-browser/, …
            ui/*         shared UI: daw-ui, dock, plugin-core, plugin-faces, …
            app/         shell (log ring, crash log, update check), launcher
            adapters/    reaper/ (everything REAPER), web/ (the AudioWorklet)
vendor/     patched third-party crates (each with UPSTREAM.md or FTS-PATCH.md)
xtask/      devtools, frame (Figma parity), plugins (the bundler), session-reaper
docs/       design/ (one-engine.md), per-domain notes (daw, signal, frame,
            patchbay, …), agents/, guides, specs
nix/        flake modules (toolchain, shells, packages), reaper-config,
            vendored flakes
scripts/    helper scripts the Justfile calls
```

Some features carry their own `AGENTS.md` / `README.md`
(`features/dynamic-template`, `features/editor`, …) — read it before
working there.

## The model

- **daw is the DAW alone; `studio` is the umbrella.** `crates/daw/*` knows
  nothing of REAPER or of any product. REAPER, the project formats, the
  UI, control surfaces and the daw CLI are features of `crates/studio`.
- **One engine.** A process runs ONE daw engine: one `Standalone`, one
  project, one realtime audio engine on one device. A rig is a folder of
  tracks in that project (`signal_rig_host::engine::{Engine, Rig}`), not an
  engine of its own. Signal and Session both just run the DAW: Signal is
  the isolated live rig, Session the full DAW, and rigs live in Session
  as tracks. In Studio the window opens on the welcome screen (the
  library, `apps/studio/desktop/src/front.rs`) and goes into the DAW view
  or the rig view of what is opened; Session's Action List is served from
  launch. More engines means more processes of the same engine. Read
  **`docs/design/one-engine.md`** before touching rigs, engines, meters or
  MIDI.
- **The apps share their shell**: the log ring / crash log / flight log,
  the update check and the iOS scene delegate (`app_shell::ios_scene`)
  are `features/app/shell`; prefs are `utils::prefs`. Session and Signal
  do not keep copies.
- **Faces are frame documents.** Control surfaces and plugin faces are
  `.frame` documents rendered by `crates/frame`. `just faces` generates the
  plugin faces into `features/ui/plugin-faces/generated` (not kept in git; the apps
  read them at runtime).

## Crate rules

- **One crate per feature, with cargo features** — not `-proto` / `-ui` /
  `-dsp` sibling crates. A feature's wire types, UI and engine are features
  of its one crate (`features/dsp/eq`: `native`, `ui`, `profiles`, …;
  `features/patchbay`: `server`, `ui`, `vox`).
- **Path deps inside the repo, declared once** in the root
  `[workspace.dependencies]` and consumed as `x.workspace = true`.
  `default-features = false` cannot be set on a workspace-inherited dep —
  put it on the `[workspace.dependencies]` entry.
- **Git deps across repos, pinned to a tag** (architect, keyflow, editor,
  music-convention, neural-amp-modeler-rs, the `vendor` repo). The branch
  pins that remain say why in Cargo.toml (nice-plug's `fts-baseview-03`,
  reaper-rs, task's `editor`).
- **The root `[patch]` tables**, all live: `crates-io` (the vendor repo's
  vox-phon / phon / phon-jit / styx-format / facet-core / baseview /
  anyrender_* / winit-uikit / wgpu-hal / raw-window-metal forks, the FTS
  blitz tag, pipewire-rs past 0.10.0); `DioxusLabs/dioxus` (the vendor
  repo's dioxus-native{,-dom} and dioxus-core);
  `FastTrackStudios/daw` (hands keyflow's git dep the in-tree `daw`,
  `dawfile-reaper` and `fts-sample` — goes when keyflow takes them from
  studio). There is no architect `[patch]`: studio, keyflow and task@main
  pin the same architect tag, so `Cargo.lock` holds one architect source;
  bump them together. A `[patch]` **cannot rename a crate**.
- **Co-developing across repos**: override with a local checkout —
  `[patch."https://github.com/FastTrackStudios/architect"] architect =
  { path = "../architect/architect" }` — and never commit it; the paths are
  machine-specific.
- **`include_str!` / `@import` across a repo boundary does not work** (a
  git dep has no stable path on disk). Export the bytes from the owning
  crate (`architect_ui::THEME_CSS`), or resolve the path with
  `cargo metadata` the way `just _lumen-link` and `just _web-tw-link` do.
- **Architect idiom**: services are `#[architect::rpc]` traits (at most 4
  params per method — a Facet constraint); live updates are `#[subscribe]`
  streams from `architect::PubSub` hubs, never Tx-parameter subscriptions;
  transports are `architect::axum_ws` + `LayerRouter` server-side and
  `vox_core::initiator_on(link).establish::<Client>()` client-side.
- **Async**: `tokio::sync::*` for locks and channels;
  `architect::platform::{spawn, sleep, timeout}` for tasks and timers
  (tokio on native, browser timers on wasm).

## Build and test

Dev shell: `nix develop` (or direnv `use flake`) — the Rust 1.94 pin with
wasm (plus iOS on darwin, Android on Linux), `dx`, wasm-bindgen,
tailwindcss, tracey, mold, and the native headers (alsa, pipewire, jack).
It sources `.env` (gitignored) on entry. `nix develop .#ci` is what CI
runs in, `.#reaper-test` the REAPER suites, `.#stems` stem separation.

```bash
just check                 # cargo check --workspace
just test                  # the core test set: cargo nextest run -p <each of the Justfile's test_packages>
just test-doc              # doctests (nextest cannot run them)
just app [PROJECT]         # Session (session-desktop, release-fast)
just guitar / just keys    # the rig view (studio-desktop --guitar/--keys skip the welcome screen; FTS_STUDIO_OPEN=<path|setlist|rig> too)
just signal-engine         # studio-desktop --engine: the headless rig engine (ws://:4040/vox)
just reaper-test [FILTER]  # Session's #[reaper_test] suites in a real REAPER
just reaper daw-test       # daw-reaper's suites (module: features/adapters/reaper/reaper.just)
just plugins-bundle [names] && just plugins-link   # .clap/.vst3 into target/bundled, linked into ~/.clap, ~/.vst3
just faces                 # generate the plugin faces
just assets                # link the big binaries (see Assets)
```

The suite is a core set (cut on 2026-10-24 from 11 317 tests to ~650): the
crates in the Justfile's `test_packages` — the DSP math, the session
format, the engine, the timeline, the wire types, every effect's parameter
round-trip. `just test`, local CI and CI build and run exactly those. A
crate gains a place there when it gains tests worth running on every change.

**Before you push: `just install-hooks`, once per clone.** It installs the
prek hooks (`prek.toml`): rustfmt, shellcheck and actionlint on the staged
files at commit, and **local CI** at push (`scripts/ci-local-hook.sh` ->
`just ci-local`): the static guards (fmt, change files, rail config, the
generated workflows + actionlint), then the tests, `cargo
check --workspace` and the wasm check for what `cargo rail plan` says the
change affects, on your warm target dir. Green, it posts the **`Local CI`**
commit status ("passed in 6m12s on host: checks 21s, plan 8s, tests …") on
the pushed commit — and on every open layer of a `gh stack` the tree
contains, so one run on the top covers the stack. Run `just ci-local`
before pushing a long change: the pass it records makes the push instant.
`SKIP=local-ci git push` skips it; the PR can merge without it, since
`Local CI` is not required. `just rail-evidence` on a clean `main` lets later runs narrow a docs or
CI change; `just ci-churn` checks a warm build recompiles nothing after
the CI prep.

**What GitHub runs.** A PR into `main` gets one free job, **`PR checks`**
(`pr.yml`: fmt, change files, the lockfile, the rail config — compiles
nothing). `main` requires only `PR checks`; `Local CI` (the pre-push
hook's status) is an indicator, not a gate. **`CI`**
(`checks.yml`: `Gates`, `Rail plan`, `Check` + advisory clippy, `Wasm`,
`Tests`, aggregated as **`CI passed`**) runs after every merge to `main`,
NARROWED by `cargo rail plan` to what the merge reaches since the newest
green commit with rail evidence (`Check` records that evidence on the pool
as it builds; no base, the full set). Hotfix PRs into `release/**` and a
run by hand get the full set. `main` may go red:
releases are cut by hand, and `just release` tags only a commit whose
`CI passed` is green. Plan, gates, the macOS release tooling and the
aggregator run on GitHub's free runners. The builds — `Check`, `Tests` (the
core test set) and `Wasm` — run side by side on THEBATTLESHIP, in a pool of
three runners sharing one warm cache (`Codys-Wright/nix-config`
hosts/THEBATTLESHIP/github-runners.nix): `studio-main` for every check,
a hotfix PR's included, so the check target dirs are built once;
`studio-release` (and airlock's `studio-release-mac`) build only release
artifacts from a tag: the studio-web image, the AppImage, the `.dmg`,
TestFlight. A newer merge queues behind the running run (only the newest waits),
so every run that starts finishes and records rail's evidence; a release job is never
cancelled. Each pool's cache
is capped at 200 GB. No WarpBuild, no snapshots, no schedule. No tailwind step anywhere
(committed sheets; dx builds session-web's); `just ci-boundaries` is
manual, not CI.

**`just ci`** runs everything a merge to `main` runs, in the same order,
and stops at the first failure: lockfile drift, `cargo fmt --check`, the
generated workflows (`just workflows-check`), the rail config, `cargo check
--workspace`, the wasm checks, then (after `just assets`) the core test set.
`just ci <step>` starts from a step (`lockfile`, `fmt`, `workflows`, `rail`,
`check`, `wasm`, `nextest`). The workflow-spec checklist (`just daw-flows`)
is a report you run by hand, not a gate.

**The workflows are generated.** Every file in `.github/workflows/` is
written by `xtask/workflows` (`just workflows`); edit the Rust, never the
YAML — `Gates` and `just ci-local` fail when they differ. The jobs run
without nix (rustup + apt + pinned tool binaries). The release builds
(`Images`, `desktop`, `studio-ios`) keep their own runners. This repo
deploys nothing: Argo CD rolls out the tag's image from gitops
(Codys-Wright/starcommand).
`just ci` refuses to start with less than 25 GB free: a full disk shows up as
mold dying with a bus error, which reads as a broken toolchain
(`just sweep-incremental` is the always-safe reclaim). It says nothing
about `studio-ios` (Xcode, signing) or the other tag builds, which
have their own runners and reasons to be red. The REAPER integration
suites (`just reaper-test`, `just reaper daw-test`) are not in CI at all:
they need a licensed REAPER, so run them locally.

**Build performance / disk** — read the `build-performance` skill before
touching a `[profile.*]` knob, benchmarking a build, or when the disk
fills up. The short version:

- Dev debuginfo is `line-tables-only`; need a debugger? `--profile dev-dbg`.
- mold is the Linux linker — `direnv reload` after pulling, or links fail
  with `cannot find -fuse-ld=mold`. `target.<triple>.rustflags` REPLACES
  `build.rustflags`, so a new global flag goes in both.
- Dependencies build at opt-level 1, with an allowlist back at 3 for
  audio-thread crates. A dev run of a rig must never xrun: extend the
  allowlist, never raise `package."*"`.
- `release-fast` is the incremental release profile the app recipes
  share; build an app with one feature set, or cargo rebuilds everything.
- Cargo never GCs `target/`: `just disk`, `just sweep`, `just sweep-all`,
  `just sweep-incremental`.
- There is no `RUSTC_WRAPPER` (sccache breaks `dx`). Check `uptime` /
  `pgrep -c rustc` before benchmarking.

## Assets

Big binaries (test fixtures over 1 MB, research captures, the rigs' NAM
captures and IRs) are not in git. They live in the private repos
`studio-assets` and `rig-models`; `assets.txt` lists each file or
directory and where it goes, and `just assets` clones those repos into
`.assets/` and symlinks everything into place. A new large file goes into
one of those repos plus a line in `assets.txt` (and `.gitignore`), never
into this repo.

## Licence

**GPL-3.0-or-later.** The workspace `license` field, every FTS-owned crate
and `LICENSE` at the root say so; a new crate inherits it with
`license.workspace = true` and does not declare its own.

Vendored code keeps the licence of the code it vendors, because it is not
ours to change: everything under `vendor/` (dioxus-test, facet-swift —
MIT and/or Apache-2.0) and in FastTrackStudios/vendor (the dioxus crates,
baseview, anyrender_*, wgpu-hal, winit-uikit, raw-window-metal — MIT
and/or Apache-2.0; `world`, the WORLD vocoder — BSD-3-Clause), and
`features/patchbay/driver` (forked from MARS mars-hal:
`GPL-3.0-or-later AND MIT`). Incoming code must be GPL-compatible (MIT,
BSD, Apache-2.0 — keep the notice and licence text); code cannot go the
other way.

## Signal, plugin and DSP rules

Signal is the live-rig domain: `features/rigs/*` (the domain core in
`features/signal/*` — proto, live, storage, controller, rig-host, grid,
tone3000, …; docs in `features/signal/docs/`), the effects in
`features/dsp/*`, `features/adapters/reaper/signal-*`, `apps/signal/*` and
Studio's Signal side (`apps/studio/desktop`).
`docs/signal/domain-model.md` explains the rig model.

**Detachable GUI (strict)**: the rig engine is headless; every GUI —
desktop, phone, browser, plugin editor — is a vox client of it.

**GUI rendering** — signal and plugin UI renders identically standalone,
as a CLAP/VST3 editor and embedded in REAPER, through one pipeline:
`nice-plug-dioxus` / `dioxus_native` → Blitz (Vello + wgpu):

- Never `dioxus::desktop::LaunchBuilder` for this UI (WebKit/WRY breaks
  plugin parity; `studio-desktop --features webview` is an escape hatch
  that nothing painted can mount under).
- **Inline styles only** — Blitz does not load external stylesheets.
  Inline `style`, or a static string via `document::Style { {CSS} }`;
  never `document::Stylesheet { href }`, no Tailwind `asset!()` —
  `include_str!` the compiled sheet (`just tailwind`, `just signal-tailwind`,
  `just tailwind-eq`, …). Components must lay out correctly without
  Tailwind; classes are additive only.
- Root `App` components take no props (context via
  `use_context_provider`), so one component serves standalone and as a
  plugin editor.
- Blitz's gaps: no `position: fixed`/`sticky` (absolute positions against
  the immediate parent; mount overlays at the root with `inset: 0`), no
  `overflow: auto` (use `overflow-y: scroll`, or `hidden` to clip), no
  `text-overflow`, no CSS `fill`/`stroke` on SVG (use presentation
  attributes), no `vertical-align`, `text-shadow`, blend modes, 3D
  transforms, multi-column or container queries. Inside a plugin
  `use_effect` never runs, `autofocus` is ignored unless the node is
  already in the document, `MountedData::set_focus` panics from an event
  handler, and there is no `placeholder`. Load the **`blitz-design`
  skill** before building or refactoring this UI.

**Processing cores** (`crates/daw/audio-graph`, the engines in
`features/dsp/*`, the sampler engine) build for native, WASM/AudioWorklet
and embedded `no_std`: `#![no_std]` + `alloc` (gate `std` behind an
additive feature); no heap allocation on the hot path (pre-allocate in
`reset()`; `process()` never calls `Vec::push`/`Box::new`); no threads and
no task spawning; no platform I/O (cpal, web-sys and MIDI drivers live in
adapter crates); keep the `AudioNode: Send` bound.

**Effects do not depend on products**: nothing in `features/dsp/*` may
depend on `features/rigs`, `features/session` or an app. If an effect
needs something from a product, the dependency points the wrong way — the
product adapts to the effect (`builtin` makes the effects daw engine
blocks, a contract both products already speak).

**Parameters must parse the string they print** — a host's parameter list
is text in, text out; each effect's `tests/param_round_trip.rs` holds that
line. Tempo-synced times use `dsp_core::time` (note values, dotted and
triplet); do not grow a second table.

**Realtime lints**: `features/{dsp,rigs,audio}/clippy.toml` add the
audio-thread rules (`disallowed-methods` for locks, `env::var`, sleeps;
`too-large-for-stack`). Clippy reads the nearest `clippy.toml`, so they hold
under those trees and nowhere else — keep the three copies identical, and
keep the shared settings in step with the root one.

**Realtime lints**: `features/{dsp,rigs,audio}/clippy.toml` add the
audio-thread rules (`disallowed-methods` for locks, `env::var`, sleeps;
`too-large-for-stack`). Clippy reads the nearest `clippy.toml`, so they hold
under those trees and nowhere else — keep the three copies identical, and
keep the shared settings in step with the root one.

## Logging & tracing — wide events, ALWAYS

Before writing ANY log or debug output, load the `logging-best-practices`
skill (`.claude/skills/logging-best-practices/` — read `rules/fts-rust.md`
first). The rules are not optional:

- **The span IS the wide event.** `architect` opens one span per vox RPC,
  `tower_http` one per HTTP request. Enrich it with
  `architect_telemetry::wide::set("namespace.field", value)` — one
  context-rich event per request, never scattered log lines.
- **Never `println!`/`eprintln!`/`dbg!` in server or library code** — not
  in committed code, and not as debug scaffolding either. To chase a bug,
  reproduce it in a failing unit test or query the span fields; if you
  must watch a live process, use `tracing` with structured fields behind
  `RUST_LOG` and delete it before committing.
- Follow the established field names (`org.slug`, `auth.*`, `perm.*`,
  `media.*`, `share.*`); record the **shape**, never the secret (no
  tokens, no passwords, no raw note paths/URIs in fields).
- Denials/refusals get ONE `tracing::warn!` line (alertable); allowed
  outcomes ride the span only.
- New surface = new fields: any new HTTP route or RPC service sets its
  outcome fields on the span (`rules/fts-rust.md` lists the field
  conventions, e.g. `auth.outcome`).

## Agent skills and process

- **Branches, PRs and releases: tag-from-main.** Load the
  `release-workflow` skill before branching, opening or merging a PR, or
  releasing; studio's specifics are in `docs/agents/release-workflow.md`,
  which wins where they differ (studio has no release PRs).
  Branch from `origin/main` (`feat/sta-123-slug`), open the PR into `main`
  with a conventional title and `.github/pull_request_template.md` filled
  in, add a change file for every crate you change (`cargo rail change add
  <crates> --bump <none|patch|minor|major> -m "<note> (signal|session)"`),
  squash-merge. There is no `dev` branch. Releases are CalVer tags pushed
  by `just release` (main's newest commit with a green `CI passed`, or
  `just release <sha>`); the tag alone starts the release builds, each of
  which stamps the tag's version into its checkout, so the version
  committed on main is not the released one. The notes come from the
  change files added since the previous tag. Never edit the workspace
  version; `CHANGELOG.md` ends at 26.10.3.
- Issues: GitHub Issues on `FastTrackStudios/studio` via `gh` — see
  `docs/agents/issue-tracker.md`; the triage labels (`needs-triage`,
  `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`) in
  `docs/agents/triage-labels.md`; domain docs in `docs/agents/domain.md`.
- **Shipping a change: the `ship-stack` skill** — one concern per layer of
  a `gh stack`, commit through the prek hooks, `just ci-local` on the top,
  `gh stack submit --auto`, every layer `Local CI: success`, merge, then
  `CI passed` on main.
- Project skills live in `.claude/skills/` (build-performance,
  logging-best-practices, blitz-design, rust-testing, reaper-testing, architect-actions,
  signalpack, reaper-theme-*, …) — load the matching one before the work
  it covers.

# The monorepo migration: state and the fix-up work

Branch `monorepo`, worktree `/run/media/Development/t3-worktrees/studio/monorepo`.
Plan: `fts-home/docs/monorepo-plan.md`. Scripts and logs:
`/run/media/Development/monorepo-import/`.

## Done (2026-11-25)

1. **Imported with history**, from the branches that actually run (stack-dev's
   manifest and `fasttrackstudio/handoff/README.md`), including uncommitted
   work copied in as `wip:` commits:

   | Repo | Source |
   |---|---|
   | studio | `dev/live` + `feat/studio-instance` |
   | daw | `feat/clap-state-reload` |
   | signal | `feat/fts-blocks` + `docs/drum-mixing-vault` |
   | dsp | `fix/reset-like-fresh` |
   | plugins | `feat/fts-blocks` |
   | keyflow | `fix/stable-typed-ids` (includes `dev/local` and `feat/chart-from-session`) |
   | rhythm | `feat/edit-plan` + `work/hybrid-drums` WIP |
   | pitch | `feat/analyze` |
   | Ignition | `dev/local` + `stack/studio-distribution` + WIP (venue data, tests) |
   | session | `main` + `feat/view-kinds` |
   | daw-view-kit, arrangement-view, mixer-view, midi-editor | `feat/extract` |
   | stem-seperation | `main` + `bench/open-model-pilots` |
   | studio-adapters, stretch, music-convention, frame, 4 theme repos | `main` |

   Every source repo carries a local tag `pre-monorepo` (and `pre-monorepo-<branch>`
   for merged siblings). `git log -- <new path>` follows the history back through
   the `import <repo>` merges.

2. **Restructured** into `apps/studio` and `crates/studio-{core,connect,engine}`
   (`git mv` only). Each imported repo's README, AGENTS, docs, CI, nix and
   workspace `Cargo.toml` are in `docs/imported/<repo>/` as reference.

3. **One workspace that resolves** (`cargo metadata`): 234 members, one
   `Cargo.lock`, every internal dependency a path, one Dioxus, Blitz, daw and
   architect. Clashes and leftovers: `docs/imported/MANIFEST-REPORT.md`.

**Not carried over:** Ignition's untracked `data/effects` (131 MB), the two
`fts-rhythm` binaries, devenv state and machine-specific lockfiles. The four
patches in `fasttrackstudio/handoff/patches/` are not applied yet.

## First `cargo check --workspace` (2026-11-25)

5 min 49 s with kache, 2,247 crates compiled; **9 crates, 119 errors** (log:
`monorepo-import/check.jsonl`). Crates downstream of these were skipped, so
more may follow.

| Crate | Errors | Cause |
|---|---|---|
| `cbx` (git dep, chatterbox TTS, via studio-session's `tts`) | 52 | written against `ort` rc.10; the workspace is on rc.15 (rhythm-hits needs it; `ort` links natively, so only one version can exist) |
| `keybinds` | 44 | `include_str!` paths into the REAPER adapter's keymaps, broken by the move |
| `trigger`, `family-clap`, `effects` | 15 | nice-plug API drift between the plugins and signal branches |
| `dynamic-template` | 4 | `monarchy::Group` field renamed in the in-tree music-convention |
| `rhythm` (extension) | 2 | `rhythm_editing` gained `crossfade` / `drawn_from` |
| `frame-mcp`, `frame-view` | 2 | an include path; an architect API change |

## The old Session app goes

"Session" is now only the file format (`<Title>.session`, a Loro document)
that every part of Studio reads and writes. It lives in `studio-core/session`.
Everything left from the old Session app has been removed or renamed
(2026-11-25):

- **The REAPER `session-extension` is removed** (owner's decision), with its
  `xtask`, its workspace and `boundaries.toml` entries, and what existed only
  for it in the old crate: `host` (the router, `DawModule` list and action
  registration it mounted), `daw_module` (`SessionDaw`) and `rpc_services`
  (`SessionModeServiceImpl`, served only through `layer_control_surfaces`).
- **The engine crates are renamed:** `session-engine` -> `studio-engine`,
  `session-runtime` -> `engine-runtime`, `session-host` (package
  `session-desktop`) -> `engine-host`, `session-engine-client` ->
  `engine-client`. engine-host's own in-process engine module is
  `live_engine`. Kept on purpose: the `--session-engine` flag the window
  spawns its engine with, engine-host's `session` / `session-domain` cargo
  features, and the app's bundle identifier.
- **`studio-session` (key `session`) is dissolved.** Each module went here:

  | Module(s) | Now | Group |
  |---|---|---|
  | `proto` (setlist, song and mode services; the action traits) | `studio-proto` (`studio-core/proto/studio-proto`) | core |
  | `sync`: `align`, `crop`, `diff`, `doc`, `hold`, `model`, `slug`, `transport`, `drift`, `relay`, `net`, `clock`; `presence` | `session-sync` (`net` and `clock` behind feature `net`; presence is the primitive's). Its old copies of the same files are gone. | core |
  | `library` (+ `publish`), `project_sync` | gone with Task (studio#727): `library-sync` was the song library in Task (`project_sync` went in #728) | — |
  | `sync`: `bridge`, `direct`, `engine`, `whole` | `engine-sync` (`studio-engine/engine/engine-sync`; re-exports `session-sync`) | engine |
  | `setlist`, `song`, `keyflow`, `guide` (+ `tts`), `key`, `key_actions`, `key_change`, `modes`, `mode_phases`, `playback`, `routing`, `track_manager`, `guitar_grow`, `comping`, `count_in`, `reference`, `review`, `cache`, `event_bus`, `color`, `lyrics`, `load_selection`, `vault_sync`, `SessionServices` / `daw_services` / `register_all_actions` | `engine-domain` (`studio-engine/engine/engine-domain`; re-exports `studio-proto` as `proto` and `engine-sync` as `sync`) | engine |
  | `ui` (components, layouts, signals, shell), `strip_width` | `studio-ui` (`studio-connect/kit/studio-ui`, features `web`, `desktop`, `chart-editor`) | connect |
  | `section_kinds`, `mix_phases` (re-exports) | callers name `fts_session::…` | core |

  Edges across a group line that the move made visible (they were all inside
  the one crate before) and that the untangle step should remove:
  - connect crates (`panels`, `engine-client`, `daw-ui`, `link`, the
    mixer/arrangement extensions, `daw-view-tools`, `apps/studio`) still
    depend on `engine-domain` for `modes::Mode`, `mode_phases`,
    `keyflow::actions` / `from_chart` / `time_signature`, `key`, `lyrics` and
    `load_selection`. `Mode` and its current-mode state should move to
    `studio-proto`, with the REAPER layout as a hook that `engine-domain` sets.
  - `studio-ui`'s mixer uses `dynamic-template` (an engine adapter) for its
    track grouping; `studio-engine` and `engine-host` use `studio-ui`.
  - `session-sync` (core) depends on `daw-transport-sync` for `Position` and
    the clock estimator. That crate has no dependencies and belongs in core.
  - `engine-domain` is still one large crate (~25k lines). `guide` (with its
    TTS) and `song` are the next to split out, `guide` beside the guide
    plugin; `setlist` and `keyflow` depend on each other, so they move
    together.
  - `engine-domain`'s `README.md` and `AGENTS.md` are the old app's, moved
    as they were; rewrite them with the AGENTS/README pass.
- **TTS stays** (owner's decision) and is **on again** (`9d829b241`):
  `bearcove/cbx` is vendored as `crates/studio-engine/vendor/cbx` (its
  licences and a NOTICE; ported to ort 2.0.0-rc.15, its CHANGELOG says how,
  and `tts_smoke` synthesized with it once). It is on in engine-host's
  `session` feature and daw-extension's default. ort rc.15 allows one ONNX
  Runtime environment per process, so `onnx-env`
  (`crates/studio-engine/analysis/onnx-env`) owns it and rhythm-hits and cbx
  both make their sessions in it.

### Next: consolidate the format crates (planned, not done)

Five crates in `studio-core/session` overlap. Merge them into the format and
its file I/O:

| Crate | What it is | Used by |
|---|---|---|
| `session` (key `fts-session`) | the primitive: structure, timeline, per-extension data, transport, presence, sync | 10 crates |
| `session-format` | the daw's project as a Loro doc (round-trips DAWproject and RPP) and its Turso store | 12 crates |
| `song-file` | `<Title>.session` on disk: the store holding the project, timing, charts, cues and branches; the old-pair converter (and a bin) | daw-extension |
| `song` | the stored Song/Arrangement schema (vault folder) | none |
| `session-sync` | the live document, its diff, relay and network lanes | 5 crates |

1. **Remove `song`**: nothing depends on it. If library code still wants
   its vault-folder schema, fold it into what holds the library then.
2. **Fold `song-file` into `session-format`** as `session_format::file`
   (`open`, `new`, `convert`, `media`), with its bin as an example or an
   xtask. It already depends on `session-format` and has one user.
3. **Fold `session-sync` into the primitive** as `session::doc` (the engine
   project's document next to the session's own) behind a `live` feature,
   with `net` / `clock` behind `net`. `engine-sync` then re-exports
   `session::live`. Check first that the primitive's `sync` module and
   session-sync's `transport` / `presence` don't define the same things
   twice; keep one of each.
4. **Rename** `session-format` -> `session-file` (the format on disk) once
   `song-file` is in it, so the crates are the primitive (`session`) and its
   file (`session-file`). `keyflow-session` and `ignition-session` stay as
   lanes, or become `session` modules in their own crates.
5. Each step is one commit that keeps `cargo check --workspace` passing.

## The fix-up work

Work in this worktree's branch, or in worktrees branched from `monorepo` under
`/run/media/Development/t3-worktrees/studio/`. **Use the dev shell for every
cargo command** (`devenv shell -- …`): the `cargo` on the default PATH is
1.98.1, and its kache keys are never reused.

1. **Make it compile.** `cargo check --workspace --keep-going --profile desktop-dev`.
   Fix by group: `studio-core`, then `studio-engine`, then `studio-connect`,
   then `apps/studio`. Expect:
   - paths baked in source (`include_str!`, `include_bytes!`, `CARGO_MANIFEST_DIR`
     joins, test fixtures): the keyflow tests that join `../..` to find
     `examples/` now find `studio-core/music/keyflow-examples`
   - the `dsp` alias: studio and view crates that meant `plugin-ui` now name it
     with `package = "plugin-ui"`
   - `session` is the primitive (key `fts-session`, or `package = "session"`);
     the old app's crate (`studio-session`) is dissolved, see above
   - `ort` moved to 2.0.0-rc.15 in keyflow-lang and the guide's TTS
   - music-convention moved from edition 2021 to 2024
   - imported crates now get the workspace's lints and `[profile]`s
2. **Apply or discard** each patch in `fasttrackstudio/handoff/patches/`, as that
   README says.
3. **Untangle the engine from the UI**: the table in the plan
   ("What has to be untangled"), crate by crate, with each crate's
   `cargo tree` check turned on as it passes.
4. **Rename the duplicate and misleading names**: `studio-shell` (package
   `app-shell`, clashes with the app-shell repo), `expression-editor` (package
   `editor`, clashes with the editor repo).
5. **Two binaries in `apps/studio`**: the UI and the engine (`engine-host`
   becomes the engine binary). Fold `lighting/ignition` and
   `lighting/ignition-engine` into it.
6. **`boundaries.toml`** for the three groups; the Justfile, xtask workflows,
   AGENTS.md and README for the new layout; prune the unused architect patches.
7. **Run the real thing**: `just studio`, open a song, play, render, MCP.
   Then the force-merge into `main` (plan, phase 4).
8. ~~TTS~~: done (`9d829b241`); see above.

## Splitting studio-engine (next round, 2026-11-25)

`crates/studio-engine/engine/studio-engine` is 87.5k lines in ~120 modules:
every engine edit recompiles all of it (8.5 min measured), and it is on
`boundaries.toml`'s allow-list. It holds almost no UI code; what breaks the
rule is dependencies.

**Step 1: no UI dependencies** (branch `engine-split/core`)
- `engine-client` (a connect crate) in 23 files, for the UI↔engine wire
  types (`body`, `actions`, `setlist`, `studio_engine`): move those types to
  `studio-core/proto` (a `studio-proto` module or a new `engine-proto`
  crate); both sides use them from there.
- `app_shell::version` → `fts-version`; `dock::DockIntent` → proto or gone.
- the `web` feature's browser glue (`web_engine.rs`, `dioxus/web`) → its
  own small crate.
- Then `just boundaries tighten`: studio-engine leaves the allow-list.

**Step 1b: actions register themselves.** `actions.rs` (5.5k lines) reaches
into 72 modules; every action is registered there. Each domain registers its
own actions (a `register(&mut Registry)` per domain crate, collected at the
top), so no hub depends on everything.

**Step 2: engine-core** (same branch): `studio_engine`, `open`/`open_core`,
`project_file`, `prepare`, `song`, `autosave`, `backup`, `history_marks`
(~12k): what the rest depend on (fan-in: open 31, project_file 21).

**Step 3: the domains, in parallel** (one worktree and branch each, each
depending only on engine-core and studio-core, never on another domain):

| Crate | Modules | ≈ lines |
|---|---|---|
| engine-listen | listen, mix_measure, render_worker, signal_offline, off_loop, audio_health | 11k |
| engine-edit | hits, hits_midi, hit_place, drum_*, comp*, take_rank, edit_*, grow, fundamental, vocal_level | 15k |
| engine-mix | mix_state, mix_snapshot, mix_variants, mix_chains, mix_sends, fx_actions, automation_actions, signal_kit, clap_fx, clap_host | 12k |
| engine-project | ingest, projects, project_new, instrument_tracks, track_regions, stream_*, song_stream, show | 9k |
| engine-collab | collab, agent, task_*, live_host | 5k |

`studio-engine` keeps only the assembly (collecting the registries).

**Step 4: measure.** `cargo build --timings` of the engine before and after,
and the edit-to-run time of a change in one domain (target: seconds).

### Done on `engine-split/core` (2026-11-26)

- **Step 1:** studio-engine depends on no UI crate. engine-client's
  modules (engine-side state a window shares in-process, one cycle) moved
  to `engine_runtime::client`; engine-client re-exports them and keeps
  only what needs Dioxus. The dock's model is core's `dock-model`,
  `FolderState` is `daw_view::folders`, `ItemEnvelopes` is
  `dsp::level::stored`, the version is `utils::version`, the lyrics model
  is core's `lyrics`, `fts-account` is in `studio-core/util`, and the
  in-process FX build without their editors (`gui` feature). studio-engine
  is off both allow-lists.
- **Step 1b:** `actions/` has a file per domain, each with one
  `register(list)`; `actions::register_all` only collects them.
- **Step 2:** `engine-core` (9.1k lines). See its `lib.rs` for the list.
- Timings: `docs/engine-split-timings.md`.

### Handoff: the domain crates (step 3)

**The five crates exist already**, empty and wired, so no agent edits a
line another does:

- `crates/studio-engine/engine/engine-{listen,edit,mix,project,collab}`:
  each depends on `engine-core` and `architect` (action-list), has the
  features `native`, `signal`, `web` and `show` forwarded from
  studio-engine, and a `pub fn register(list: &ActionList) ->
  Result<(), ActionError>` that registers nothing yet.
- Root `Cargo.toml`: in `members` and `[workspace.dependencies]`
  (`default-features = false`). studio-engine depends on all five, and its
  `native`/`signal`/`web`/`show` features forward to them. Don't touch
  these lines.
- `boundaries.toml`: nothing to add; the groups go by folder, so each crate
  is in the engine group already. Never add an allow-list entry.
- The collector, `studio-engine/src/actions/mod.rs` `register_all`, has
  two lines per domain: `engine_<d>::register(list)?;` (your crate's) and
  `<d>::register(list)?;` (what is still in `actions/<d>.rs`). When
  `actions/<d>.rs` has moved into your crate, delete its line and its
  `mod <d>;`. Nothing else in that function is yours.

**The registry is a plain compiled-in one** (the owner's decision,
"Decision: two binaries, everything compiled in"). The `Registry` is
architect's `ActionList`, which registers through a shared reference:
`register(list: &ActionList)`. The engine binary's crate calls
`studio_engine::install()`, which fills the list through
`actions::register_all`. Nothing assumes actions arrive through an
extension package: keep it that way. The extension crates
(`crates/studio-engine/extensions/*`, `daw-extension` among them) are
becoming ordinary library crates that the engine binary links. A later
task folds `daw-extension` and `engine-host` into `apps/studio`'s
`studio-engine` binary, so don't build anything on their packaging.

**How to move a module** (the way engine-core was cut):

1. `git mv` it from `studio-engine/src/` to your crate's `src/`, and
   declare it in your `lib.rs` with the `#[cfg]` it had in
   studio-engine's `lib.rs`.
2. In studio-engine's `lib.rs`, replace its `pub mod x;` line with
   `pub use engine_<d>::x;` (keep the `#[cfg]`), so every `crate::x::…`
   in studio-engine and every `studio_engine::x::…` outside still
   resolve.
3. In your crate, `crate::` paths reach the core's modules once your
   `lib.rs` has `pub use engine_core::*;` (engine-core re-exports
   `engine_runtime::client::*` the same way). Your own modules shadow the
   glob.
4. Move `actions/<d>.rs` last: its body becomes your `register`. Its
   `use super::*;` reached `actions/mod.rs`'s shared helpers, which are
   `engine_core::actions::{song, resolve, resolve_in, meta, list}` now,
   plus `also_in_the_window` and `NoArg` (still in studio-engine: copy
   neither, move them to `engine_core::actions` under your heading if you
   need them).
5. `pub(crate)` items used from studio-engine become `pub`.
6. Copy the dependencies your modules use from studio-engine's
   `[dependencies]` (the same `workspace = true` lines and features). Keep
   studio-engine's own lines: other modules there may still use them.

**Shared, don't duplicate:** `engine_core::{actions, assembly, open,
project_file, prepare, song, history, history_marks, edit_trace,
safety_net, telemetry, studio_engine::set, audio_device, render_ahead}`.
Use them from the core; never copy one.

**The shared files, and how not to conflict in them:**

| File | Your lines |
|---|---|
| `engine-core/src/lib.rs` | under `// ── from engine-<d> ──` only |
| `engine-core/src/assembly.rs` (`Assembly`) | under `// ── engine-<d>'s parts ──` only |
| `studio-engine/src/body.rs` (`ASSEMBLY`) | under `// ── engine-<d>'s parts ──` only |
| `studio-engine/src/lib.rs` | your modules' `pub mod` → `pub use` lines only |
| `studio-engine/src/actions/mod.rs` | your `<d>::register` and `mod <d>;` lines only |
| `Cargo.lock` | regenerate on a conflict (`devenv shell -- cargo metadata --format-version 1 >/dev/null`); never hand-merge |

Rebase onto `monorepo` often; each of these is a different line per
domain.

**The modules as they stand** (lines include tests; `track_regions` is
in edit, not project: its dependencies and nearly all its users are
edit's):

| Crate | Modules | ≈ lines | Also |
|---|---|---|---|
| engine-listen | listen, mix_measure, render_worker, signal_offline, off_loop, audio_health, perf_meter | 14.4k | `actions/listen.rs` |
| engine-edit | hits, hits_midi, hit_place, hit_flags, hit_index, hit_ops, drum_apply, drum_bench, drum_group, drum_lanes, drum_level, drum_map_set, drum_plan, drum_reference, drum_samples, comp, comp_keep, comp_rank, comp_song, take_rank, edit_actions, edit_list, edit_reads, edit_revert, grow, fundamental, vocal_level, seams, trigger_from_mics, sample_align, key_change, item_actions, track_regions | 26.4k | `actions/edit.rs` (3.6k: drum editing) |
| engine-mix | mix_state, mix_snapshot, mix_variants, mix_chains, mix_sends, mix_export, fx_actions, automation_actions, signal_kit, clap_fx, clap_host, bass_rig, rig_actions, algorithms | 15.8k | `actions/mix.rs` |
| engine-project | ingest, projects, project_new, instrument_tracks, stream_set, stream_in, song_stream, show, library_actions, organize_edit | 7.7k | `actions/project.rs`, `studio_engine/set_open.rs` (the set's openers, under `crate::studio_engine::set`) |
| engine-collab | collab, agent, task_account, task_set, live_host | 4.2k | `actions/collab.rs` |

**Staying in studio-engine** (the assembly; it may use every domain):
`actions/{mod,base}.rs`, `body` (the `Assembly` it fills), `open` (the
wrappers that install first), `studio_engine/{headless,mcp,live,
view_host,set_actions}`, `engine_host`, `setlist`, `song_open`,
`shared_actions`, `page`, `perf`, `record`, `poi`, `timeline_actions`,
`audio_window`, `reference_play`, `web_engine`, `web_actions`, the bins.
A domain module that needs one of these needs it moved down to the core
first (or the call inverted), never a dependency on studio-engine.

**The domains still reach each other** (`crate::<module>::<item>` from
one domain's files into another's, today). Each is resolved before that
domain's crate compiles on its own, by one of: moving the item down into
engine-core (the owning domain's agent does it, first, in its own commit,
under its heading), or calling it through `engine_core::assembly` (a
field under the owner's heading, filled in `body.rs`). **Never a
dependency between two domain crates.**

| From | Into | Items |
|---|---|---|
| listen | collab `agent` | `By`, `InAction`, `InFlight`, `watch` |
| listen | edit `hits`, `vocal_level` | `kit`, `playback`; `Knobs`, `kept`, `level`, `level_with_signal`, `playing` |
| listen | mix `clap_fx`, `clap_host`, `fx_actions`, `mix_sends`, `mix_snapshot`, `signal_kit` | `add`, `display_name`, `loadable`; `hand_copy`, `plugin_call`, `pump`, `signal`; `moment`, `project_guid`, `target`; `add`, `list`, `paths`, `set`; `Mix`, `current`, `put`; `instances`, `is_signal_bundle`, `kit`, `private_session` |
| edit | collab `collab` | `chart_context`, `local_chart` |
| edit | listen `signal_offline` | `AnalyzeArg`, `analyze` |
| edit | mix `algorithms`, `clap_host`, `fx_actions`, `mix_chains` | `place`, `plan`; `signal`; `moment`; `drum_returns` |
| mix | edit `fundamental`, `hits`, `hits_midi`, `vocal_level` | `KICK`, `SNARE`, `TOM`, `find`; `kit`, `playback`; `hit_notes`, `note_for`; `has_lanes`, `lane_shown`, `playing`, `show_lane` |
| mix | listen `mix_measure`, `signal_offline` | `Bands`, `RATE`, `Span`, `analyse`, `render_span`; `after_step` |
| project | collab `collab`, `live_host`, `task_account`, `task_set` | `TaskSet`, `env_set`, `join_task_in_background`; `opened_from_library`; `AccountLibrary`, `remember_org`, `remembered_org`, `sign_out`, `signed_in`; `join`, `until_ok` |
| collab | listen `off_loop` | `blocking`, `run_by` |

Some of these are clearly the core's and nobody's domain: `hits::{kit,
playback}` (read by three domains), `fx_actions::{moment, project_guid,
target}`, `off_loop::{blocking, run_by}`, `agent`'s in-flight tracking,
`task_account` and `task_set` (Task's account, read by project and
collab). Their owners move them down first. Listen and mix reach each
other the most (measuring reads the mix's FX, sends and Signal; the mix
reads measurement): if moving those down makes the core a second mix,
that is the owner's call (one permitted edge, listen → mix, or one
crate), not a domain agent's.

**Checks for each domain:** `devenv shell -- cargo check --workspace`,
`just boundaries`, `cargo tree -p engine-<d> -e normal` free of
studio-engine and every other domain, `just test engine`, and the
timings of `docs/engine-split-timings.md` for an edit in one of your
modules (step 4).

## Decision: two binaries, everything compiled in (owner, 2026-11-25)

- **Two binaries in `apps/studio`, one app and one bundle:** `studio`, the
  connect app (UI: studio-core + studio-connect), and `studio-engine`, the
  engine (studio-core + studio-engine). Separate processes: the engine
  iterates without the UI, the UI joins any engine by address (local,
  remote, several instances), and a plugin crash takes the engine, not
  the window.
- **Every extension is compiled straight in by default**: on the engine
  side `daw-extension`, rhythm, pitch, keyflow, tempo-map and Signal (no
  longer a CLAP in Studio); on the UI side the view extensions
  (arrangement, mixer, lyrics, navigator, link, chart). No extension
  packaging (`extension.toml`), no install copy into
  `~/.config/fts/studio-extensions-*`, no extensions folder at run time:
  each binary runs what it was built with.
- **Dev overrides stay, opt-in only**: loading an external build of one
  extension (`STUDIO_EXT_DEV` or its successor) for rapid iteration, never
  the default path.

## Queued (2026-11-25)

- **The two binaries, every extension compiled in** (after the engine
  split lands): fold `daw-extension` and `engine-host` into `apps/studio`'s
  `studio-engine` binary; compile the engine extensions and the view
  extensions in; drop the extension install/copy path; Studio launches the
  engine binary from its own build (bundle) output.

- **Signal compiled into the engine** (landed, 2026-11-26): the plugin is
  `signal-clap` (`studio-engine/signal/signal-clap`: signal-core as a
  nice-plug plugin, its `signal/1` extension and its state, no editor);
  the bundle (`plugins/signal/signal`) is that plus its Dioxus editor.
  The engine hosts it in-process through its own CLAP entry
  (`signal_clap::entry::ENTRY`, `daw_standalone::plugin::BuiltinClap`,
  registered in `studio_engine::install`) wherever a song names it
  (`Signal Dev`, `FTS Signal`, a bundle's path from any machine), through
  the same CLAP host as a bundle: nothing installed, found or loaded, and
  always the built version. The bundle stays for other hosts. Still
  through the CLAP state stream; restoring the state without it (a native
  instance over signal-core, the engine's Signal calls as a Rust trait
  instead of `signal/1`) is the next step if `fx.load_states` stays slow.
- **Unbreakable integration test** (landed): the open/play/listen/render
  sequence on the real song, timed from the engine's spans against budgets
  that only shrink; `just test-unbreakable`. It lives in `daw-extension`'s
  `tests/unbreakable` and drives the binary only (`CARGO_BIN_EXE_…`, the MCP
  door): when the engine binary moves to `apps/studio`, the test folder
  and the recipe's `-p` move with it.
- **Song open toward REAPER's ~3 s:** cache the imported store by the
  `.session`'s hash (`store.import_snapshot` 7.9 s every open), per-plugin
  spans inside `fx.load_states`, then `bridge.read_doc` (2.8 s).
- **Render worker stale sync:** fixed (`963c7dcd7`). TRACK_SET made no
  undo step, so its edit never reached the song's doc, which the worker
  syncs from; it is one step now, and
  `the_worker_renders_what_the_engine_renders…` passes. Renders still
  default to the engine (`7aac60047`): moving them back to the worker is
  the owner's call. Any other action that edits without an undo block has
  the same gap.

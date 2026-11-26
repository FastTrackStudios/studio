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
  | `library` (+ `publish`), `project_sync` | `library-sync` (`studio-core/session/library-sync`, features `publish`, `project-sync`) | core |
  | `sync`: `bridge`, `direct`, `engine`, `whole` | `engine-sync` (`studio-engine/engine/engine-sync`; re-exports `session-sync`) | engine |
  | `setlist`, `song`, `keyflow`, `guide` (+ `tts`), `key`, `key_actions`, `key_change`, `modes`, `mode_phases`, `playback`, `routing`, `track_manager`, `guitar_grow`, `comping`, `count_in`, `reference`, `review`, `cache`, `event_bus`, `color`, `lyrics`, `load_selection`, `vault_sync`, `SessionServices` / `daw_services` / `register_all_actions` | `engine-domain` (`studio-engine/engine/engine-domain`; re-exports `studio-proto` as `proto` and `engine-sync` as `sync`) | engine |
  | `ui` (components, layouts, signals, shell), `open_link`, `strip_width` | `studio-ui` (`studio-connect/kit/studio-ui`, features `web`, `desktop`, `chart-editor`) | connect |
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
   its vault-folder schema, fold it into `library-sync`.
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

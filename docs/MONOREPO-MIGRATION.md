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
**Everything left from the old Session app is removed:**

- **`crates/studio-connect/features/session`** (package `studio-session`, key
  `session`), the old app's crate, is dissolved. 16 crates depend on it
  (the engine, the guide plugin, `apps/studio`, panels, views) through about
  450 `session::…` references. Its stub binary and two tools are already
  deleted. Move each module to where it belongs, then delete the crate:

  | Module(s) | Goes to |
  |---|---|
  | `sync`, `proto`, `keyflow` | the format: `studio-core/session` (`session-sync`, `session`, `keyflow-session`) |
  | `modes`, `mode_phases`, `playback`, `routing`, `track_manager`, `comping`, `count_in`, `key*`, `guitar_grow`, `reference`, `review`, `cache`, `daw_module`, `event_bus`, `rpc_services`, `host`/`services` | the engine: `studio-engine/engine` |
  | `guide` | `studio-engine` (with the guide plugin) |
  | `library`, `setlist`, `song`, `vault_sync`, `project_sync`, `load_selection`, `open_link` | `studio-connect/features/library` (or the format, where it is document code) |
  | `ui`, `lyrics`, `color`, `strip_width` | `studio-connect` (kit or views) |
  | `tts` (`cbx`) | decide: keep (move cbx to `ort` rc.15) or drop |

- **Names from the old app** go too. Rename to what each is now:
  - `studio-engine` → `studio-engine`
  - `engine-runtime` → `engine-runtime`
  - `session-host` (package `engine-host`) → `engine-host`, then the engine binary in `apps/studio`
  - `engine-client` → `engine-client`
  - the REAPER `session-extension` → decide: keep as `reaper-bridge`, or drop with REAPER hosting

  Studio's crate names move with them.
- **Overlapping format crates** in `studio-core/session` merge down to the
  format and its file I/O: `session` (the primitive), `session-format`
  (`Song.session`, Loro + Turso), `song-file` (`<Title>.session` on disk),
  `song` (stored Song/Arrangement) and `session-sync` (live document). Keyflow's
  and Ignition's lanes stay as `keyflow-session` / `ignition-session`, or fold
  into their crates as `session` modules.

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
   - `session`: studio's crate is package `studio-session` (key `session`);
     imported crates that meant the primitive use `package = "session"`
   - `ort` moved to 2.0.0-rc.15 in keyflow-lang and studio-session
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
5. **Two binaries in `apps/studio`**: the UI and the engine (`session-host`
   becomes the engine binary). Fold `lighting/ignition` and
   `lighting/ignition-engine` into it.
6. **`boundaries.toml`** for the three groups; the Justfile, xtask workflows,
   AGENTS.md and README for the new layout; prune the unused architect patches.
7. **Run the real thing**: `just studio`, open a song, play, render, MCP.
   Then the force-merge into `main` (plan, phase 4).

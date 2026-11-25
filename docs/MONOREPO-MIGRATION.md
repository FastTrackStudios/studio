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

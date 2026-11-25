# studio: repo instructions

**One repository, one app, one release.** Studio (`apps/studio`) is the
app, for web, desktop and iOS. Under it are three groups of small crates:
`studio-core` (shared data and contracts), `studio-connect` (the UI) and
`studio-engine` (the DAW engine). In November 2026 it absorbed the daw,
signal, dsp, plugins, keyflow, frame, music-convention, rhythm, pitch,
stretch, stem-separation, Ignition, session and studio-adapters repos, the
view kit and the arrangement, mixer and MIDI views, and the theme packs,
with their history. Their own READMEs, AGENTS, docs and CI are in
`docs/imported/<repo>/`, kept as reference. The migration's state and what
is still being untangled: `docs/MONOREPO-MIGRATION.md`.

"Session" now means only the `.session` file format (`studio-core/session`).
The old Session app is gone; its engine crates are `studio-engine`,
`engine-runtime`, `engine-host` and `engine-client`.

## The dev shell

Run every cargo, dx and just command in this checkout's dev shell
(`devenv.nix`): direnv loads it, or prefix a command with
`devenv shell --`. The default PATH has carried cargo 1.98.1. Its builds
compile everything again and share no kache keys with anyone. The pin is
1.99.1 (`rust-toolchain.toml`). Check with `cargo --version` before a long
build. Every `just` recipe enters the shell itself (`scripts/dev-shell.sh`).
Commit and push from the shell too: the prek hooks' shellcheck and
actionlint come from it.

## Layout and the boundary rule

| Group | Path | May depend on |
|---|---|---|
| core | `crates/studio-core/{proto,session,music,frame,util}` | core |
| connect | `crates/studio-connect/{client,theme,kit,views,plugin-ui,features,mcp,tools}` | core, connect |
| engine | `crates/studio-engine/{engine,daw,dsp,signal,analysis,lighting,adapters,extensions,plugins,web,vendor}` | core, engine |
| app | `apps/studio` | everything |
| tools | `xtask/`, `vendor/` | anything |

- **The exception:** `crates/studio-engine/plugins/*` (CLAP/VST3 bundles
  with their own GUI) may depend on `crates/studio-connect/plugin-ui/*`.
  Nothing depends on a bundle.
- **The promise:** no core or engine crate (the bundles aside) resolves
  Dioxus, Blitz, app-shell or a studio-connect crate. The engine builds,
  runs and is tested without the UI; the UI joins an engine, or opens a
  session with none running.
- `boundaries.toml` holds the rules, and `scripts/boundaries.py` checks
  them (`just boundaries`; it runs in `just check`, local CI and CI). Every
  declared path dependency counts, optional and dev ones included.
- Today's violations are the allow-list at the end of `boundaries.toml`.
  It **only shrinks**: a new violation fails, and so does an entry that no
  longer occurs (`just boundaries tighten` removes those). Fix a new
  violation in the code (move the type down to studio-core, or the UI out
  to studio-connect). Never add an entry.
- **Before working in a group, read its `AGENTS.md`**
  (`crates/studio-core/AGENTS.md`, `crates/studio-connect/AGENTS.md`,
  `crates/studio-engine/AGENTS.md`): the group's own rules.

## Build and test

```bash
just studio        # the desktop app under dx serve (Blitz); builds the engine first
just engine        # the engine headless (daw-extension --engine); `status`, `stop`
just web           # the web app under dx serve
just check         # cargo check --workspace + just boundaries
just check-web     # the browser builds for wasm32 (studio-app --features web, Keyflow's editor)
just test <group>  # core | connect | engine | app | tools | all; extra args go to nextest
just ci-local      # the PR gate on this machine, for what `cargo rail plan` says you touched
just ci            # what CI runs after a merge, in its order
```

- **Use the recipes, not your own `cargo -p` sets.** Cargo unifies
  features per invocation, so a different package set compiles the shared
  crates another way, under another kache key. `just test <group>` always
  builds the same set: the group's members, minus `[test] skip` in
  `boundaries.toml`.
- **One build for working:** dx's `desktop-dev` profile for the host
  triple, into `target/<host>/desktop-dev`. The window, the engine and the
  recipes share it; `release-fast` and `--release` are for shipping and
  measuring.
- Developing the app itself (the engine it finds, its actions, overriding
  an extension while it runs): `apps/studio/docs/development.md`.
- Your own target dir in a worktree, named `target` (`CARGO_TARGET_DIR`
  set to `…/target`); kache keys stay stable only that way. Keep worktrees
  outside the checkout. Load the `kache` skill when a build misses, and the
  `build-performance` skill before touching a `[profile.*]`.
- Cargo never collects `target/`: `just disk`, `just sweep`.

## The workspace

- One workspace, one `Cargo.lock`, one toolchain, one set of profiles.
  Every internal crate is a path dependency declared once in the root
  `[workspace.dependencies]`, consumed as `x.workspace = true`.
  `default-features = false` goes on that entry, never on a consumer.
- External pins (drive, architect, app-shell, editor, the forks) live in
  one block of the root `Cargo.toml`, by `rev` or tag. One Dioxus line,
  through `[patch.crates-io]`. A `[patch]` cannot rename a crate.
- Two packages are named `app-shell`: the workspace's
  (`crates/studio-connect/kit/studio-shell`) and FastTrackStudios/app-shell.
  Spell `-p` with a path or a version when you mean one of them.
- **No `include_str!`/`include_bytes!` outside the crate's own folder**,
  and test fixtures live inside the crate that reads them. A path that
  climbs out of a crate broke on every move so far. `env!("CARGO_MANIFEST_DIR")`
  only in tests (`utils::paths::checkout_path` otherwise).
- Async: `tokio::sync::*` for locks and channels;
  `architect::platform::{spawn, sleep, timeout}` for tasks and timers (it
  works on wasm too).
- The workflows in `.github/workflows/` are generated by `xtask/workflows`
  (`just workflows`). Edit the Rust, never the YAML; CI fails on drift.

## Assets

Big binaries (fixtures over 1 MB, research captures, the rigs' captures
and IRs) live in the private `studio-assets`, `rig-models` and
`mic-models` repos. `assets.txt` says where each goes, and `just assets`
links them into place. A new worktree gets them from the post-checkout
hook (`just worktree-setup` by hand). A new large file goes into one of
those repos and `assets.txt`, never into git here.

## Logging

Load the `logging-best-practices` skill (`.claude/skills/`) before writing
any log or debug output. The span is the wide event: enrich it with
`architect_telemetry::wide::set`. Never commit `println!`, `eprintln!` or
`dbg!` in library or server code. Reproduce a bug in a failing test
instead.

## Process

- **Others work in this checkout at the same time.** Stage only the paths
  you changed (`git add <paths>`, never `-A`), and commit them with
  `git commit -- <paths>`, which leaves other people's staged work alone.
  If `.git/index.lock` exists, wait and retry.
- **Branches, PRs and releases:** load the `release-workflow` skill;
  studio's specifics are `docs/agents/release-workflow.md`. Every PR that
  changes a crate carries a change file (`cargo rail change add <crates>
  --bump <none|patch|minor|major> -m "<note>"`).
- **One release for the repo:** `just release` tags `studio-vYY.M.MICRO` on
  main's newest green commit, and the tag ships. The `vYY.M.MICRO` tags are
  Studio's releases from before the monorepo; they still count for the
  next version and the notes.
- **CI:** a PR gets `PR checks` (the gates that compile nothing) and
  `Affected crates` (`cargo rail plan` against the base: check, tests,
  wasm). `CI` (`checks.yml`) runs after every merge to main, narrowed by
  rail; `just release` requires its `CI passed`. Mark a draft PR ready to
  run its checks. `just install-hooks` once per clone: local CI on push.
- Issues: GitHub Issues on `FastTrackStudios/studio` (`docs/agents/issue-tracker.md`).
- Project skills are in `.claude/skills/`: load the matching one before
  the work it covers (`blitz-design` for any Blitz UI, `rust-testing`,
  `reaper-testing`, `drum-mixing`, `ignition-domain`, the `song-*` ones).

## Licence

GPL-3.0-or-later: a new crate inherits `license.workspace = true`.
Vendored code keeps its own licence (FastTrackStudios/vendor, `vendor/`).
Incoming code must be GPL-compatible (MIT, BSD, Apache-2.0, with its
notice). Model weights under a non-commercial licence ship only as an
optional download, never in git.

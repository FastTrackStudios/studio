# studio-adapters — Repo Instructions

Studio's adapters, out of FastTrackStudios/studio with their history
(studio#510). `README.md` has the layout. Studio takes every crate here by
git at one revision; nothing below (daw, fts-fx, signal, Frame, Architect)
may depend on anything here, and nothing here may depend on Studio.

## Rules (they came with the code from Studio's AGENTS.md)

- **daw is the DAW alone.** REAPER, the project formats, control-surface
  wiring and the daw CLI are adapters: here, behind features of `studio`
  where an app needs them.
- **REAPER work**: load Studio's `reaper-testing` and `architect-actions`
  skills; the integration suites need a licensed REAPER and do not run in
  CI (`just reaper daw-test`).
- **No `include_str!` / `include_bytes!` across a repo boundary.** Export
  the bytes from the crate that owns them (`keybinds::embedded`).
- **No `env!("CARGO_MANIFEST_DIR")` outside tests** (kache keys), no
  absolute paths in build-script output.
- **GUI rendering**: Blitz (Vello + wgpu), inline styles only (Studio's
  `blitz-design` skill).
- **Logging**: wide events; never `println!`/`eprintln!`/`dbg!` in library
  code.
- **Async**: `tokio::sync::*`; `architect::platform::{spawn, sleep,
  timeout}`.
- **Crates**: one crate per feature with cargo features; path deps
  declared once in `[workspace.dependencies]`; git deps pinned to a tag or
  a `rev` (the daw repo, fts-fx and signal at one revision each, Studio's).
  `license.workspace = true` (GPL-3.0-or-later).
- **Large files** (test fixtures over 1 MB) live in the private
  studio-assets repo, listed in `assets.txt` and linked by `just assets`;
  never in git here.

## Pins

The external pins and the `[patch]` tables are a copy of Studio's, and
`Cargo.lock` was seeded from Studio's: move a pin here and in Studio
together, or Studio builds two of a crate.

## Process

- Dev shell: `devenv shell` (devenv-fleet's rust module: rust 1.99.1, the
  wasm target, kache, prek).
- CI (`.github/workflows/checks.yml`) runs on PRs that are not drafts:
  fmt, `cargo check --workspace --all-targets`, `cargo test --workspace`,
  and the wasm32 check for what Studio builds for the browser. Mark a PR
  ready to get checks.
- A change file per PR (`.changes/README`).

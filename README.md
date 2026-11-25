# studio

FastTrackStudio's music software in one repository: **Studio**, one app for
the desktop, the browser and iOS, and the DAW engine it runs.

[![CI](https://github.com/FastTrackStudios/studio/actions/workflows/checks.yml/badge.svg?branch=main)](https://github.com/FastTrackStudios/studio/actions/workflows/checks.yml?query=branch%3Amain)
Rust 1.99.1 (`rust-toolchain.toml`) · GPL-3.0-or-later · releases are
`studio-vYY.M.MICRO` tags ([Releases](https://github.com/FastTrackStudios/studio/releases/latest))

```bash
devenv shell        # or direnv: the dev shell, with the pinned toolchain
just studio         # the desktop app, rebuilt as you edit (dx serve)
```

## What it gives you

- **Studio** (`apps/studio`): setlists and songs, charts in the Keyflow
  language, lyrics, lighting (Ignition), and a song's arrangement and mixer.
  Songs and setlists are plain files; Drive syncs them when it is installed.
- **The engine** (`crates/studio-engine`): the DAW itself, in a process of
  its own that the app starts and talks to. It records, edits, mixes and
  renders a song, and runs Signal's live rigs (guitar, bass, keys, drums,
  sampled instruments).
- **The FX plugins**: EQ, compressor, saturation, delay, reverb,
  modulation, gate, limiter, pitch, NAM amps and more, as CLAP and VST3.
- **The analysis**: tempo maps, drum hits, pitch and stem separation for a
  song's recordings.
- **Keyflow and Frame**: the chart language and its engraver, and the
  `.frame` documents the plugin faces and control surfaces are drawn from.

## Run it

Everything runs from the repository root, in the dev shell (`devenv.nix`).
The `cargo` on a default PATH may be an older toolchain; the dev shell's is
the pinned one, and every `just` recipe enters it for you.

```bash
just studio          # the desktop app (Blitz) under dx serve; builds the engine first
just engine          # the engine alone, headless (`just engine status`, `just engine stop`)
just web             # the web app under dx serve (talks to the Drive node on :8096)
just check           # cargo check --workspace, then the layout rules (boundaries.toml)
just test engine     # one group's tests: core, connect, engine, app, tools or all
just ci-local        # what a PR's CI checks, for what your change affects
just plugins-bundle  # the CLAP/VST3 plugins into target/bundled
```

`just` lists every recipe. Developing the app (the engine it starts,
driving it through its actions, the web build):
[apps/studio/docs/development.md](apps/studio/docs/development.md).

## Install

macOS (Apple silicon) and Linux (x86_64):

```sh
curl -fsSL https://raw.githubusercontent.com/FastTrackStudios/studio/main/scripts/install.sh | sh
```

It installs the newest release: on macOS the notarized Studio.app into
`/Applications`, on Linux the AppImage into `~/.local/share/studio` with
`studio` on your PATH. Every download is checked against the release's
`SHA256SUMS`. Run it again to update. `STUDIO_VERSION=X.Y.Z` installs a
given version; a release from before the monorepo is `STUDIO_VERSION=v26.11.5`.
iOS has no release build yet.

## Layout

The code is three groups of small crates, and one app on top. Each group
has its own `AGENTS.md`.

| Path | What it is | May depend on |
|---|---|---|
| `apps/studio` | the app (package `studio-app`, program `Studio`): web, desktop, iOS | everything |
| `crates/studio-core` | shared data and contracts: the UI↔engine protocol (`proto`), the `.session` format (`session`), Keyflow and the music model (`music`), Frame (`frame`). No Dioxus, no audio thread | core |
| `crates/studio-connect` | the UI: the engine client, theme, view kit, the views (arrangement, mixer, MIDI, chart, lyrics, lighting, rigs), plugin UIs. Talks to an engine; never links one | core, connect |
| `crates/studio-engine` | the DAW engine: `daw`, DSP, Signal, analysis, lighting, the host adapters, the plugin bundles. No Dioxus | core, engine |
| `xtask/` | the plugin bundler, the CI generator (`xtask/workflows`), dev CLIs | anything |
| `docs/` | design notes, decisions, the migration (`docs/MONOREPO-MIGRATION.md`); `docs/imported/` holds the merged repos' own docs | |

The one exception: the plugin bundles (`crates/studio-engine/plugins/*`)
ship their own GUI, so they may use `crates/studio-connect/plugin-ui/*`.
`just boundaries` checks these rules, and that no core or engine crate
pulls in Dioxus, Blitz, app-shell or a connect crate. Today's violations,
which the migration is removing, are an allow-list in `boundaries.toml`
that can only shrink.

## Assets

Large binaries (test fixtures, research captures, the rigs' amp captures
and impulse responses) live in private repositories. `just assets` fetches
them and links them into place (`assets.txt`). The tests that read them,
and the rigs' bundled models, need them.

## For coding agents

- `AGENTS.md`: the rules for the whole repo (the dev shell, the boundary
  rule, recipes, process), and `crates/studio-*/AGENTS.md` for each group.
- `.claude/skills/`: the project's skills (Blitz UI, logging, testing,
  the domain skills for drums, songs and lighting).

## Licence

GPL-3.0-or-later (`LICENSE`). Patched third-party crates live in
[FastTrackStudios/vendor](https://github.com/FastTrackStudios/vendor),
under their own licences.

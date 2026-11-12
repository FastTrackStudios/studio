# Studio Architect: Studio's distribution of Architect

**Studio Architect** (`apps/architect`, package and binary
`studio-architect`) is the Architect app with Studio's extensions
**compiled in**. Its headless node (`studio-architect serve`) is what a
hosted Architect runs, so that a setlist shared from the hosted node
draws its songs' charts.

Architect's guide to distributions is
`docs/content/guide/your-own-architect.md` in FastTrackStudios/architect
(at the tag Studio pins). This crate follows its shape: a crate whose
`main` is `architect_desktop::run(<its Distribution>)`, with `gui` on by
default and the node built without it.

## Why it exists

A node draws a fence on a share link or a Site only if a renderer for it
is compiled into the node (`Distribution::block`, `node::start_with`). The
setlists extension installed from the store is a process extension, and a
node cannot ask one to draw (FastTrackStudios/architect#547). So a stock
Architect node shows a song's chart as its Keyflow text, and this node
draws it as Keyflow's follow SVG: bars timed on the chart's clock, tied to
the song's recording, followed by the page's player (studio#406).
`apps/architect/tests/node.rs` shows both on real nodes.

The owner's decision for the hosted app (architect#528, D1) is Studio
"both compiled in and loadable at run time, with override". That is what
a distribution gives: Studio's extensions are linked in, everything else
stays installable from the store, and an installed or development copy of
a linked extension still overrides it.

## What it is

| | Studio Architect | the Architect app | Studio (`apps/studio/desktop`) |
| --- | --- | --- | --- |
| name | `Studio Architect` | `Architect` | `Studio` |
| binary / app name | `studio-architect` | `architect-desktop` | `studio-desktop` |
| config folder | `studio-architect` | `architect` | Studio's prefs |
| bundle id | `app.fasttrackstudio.studio-architect` | `app.fasttrackstudio.architect` | `app.fasttrackstudio.studio` |

It does not adopt the Architect app's folder (`named` drops
`architect()`'s `adopts`), so on a desk it starts with data of its own.

**Compiled in** (`studio_architect::distribution`):

- everything the Architect app compiles in (`architect_desktop::distribution::architect()`):
  the Project Management bundle (Projects, Tasks, Time Tracker), Calendar,
  Day Plans, Threads, Notify, Files;
- the `studio` bundle: Setlists (`setlists::ext::Setlists`, id
  `studio-setlists`), Productions (`productions::ext::Productions`, id
  `studio-productions`) and Studio's views (`studio_views::ext::StudioViews`,
  id `studio-views`: the Views toolbar at the bottom, studio#409,
  `features/studio-views/README.md`);
- Studio's block renderer (`setlists::share::blocks`): the `kf` chart
  fence as the follow SVG and the `studio-setlist` fence as a set's
  playable list, on share links and Sites.

**Architect features**: `architect-desktop` with `vox` (the node's
services), and `gui` only in the desktop build. The node is built with
`--no-default-features` and `--cfg no_workspace_hack`, as
`cargo xtask dist node` builds Architect's own node. It does not have
`node-wasi` (Wasmtime in the node, so installed WASI components draw on
share links): nothing Studio ships needs it yet.

`studio-architect distribution` prints all of this as JSON, which is also
the web bundle's `/app/distribution.json`.

## Building it

```sh
just studio-architect-node        # the node for this machine: prints node=<path>
just studio-architect-web OUT     # the web bundle into OUT
just studio-architect-check       # cargo check of the node, as the PR job runs it
just studio-architect-release v26.11.0 DIR   # the release's tarballs into DIR
cargo run -p studio-architect     # the desktop app (gui), if you want the window
```

The recipes run `scripts/studio-architect.sh`:

- **node**: `cargo build --profile release -p studio-architect
  --no-default-features --config 'target.<host>.rustflags=["--cfg","no_workspace_hack"]'`.
  Cargo joins that array to the repo's table for the host, so on x86_64
  Linux the node is linked with mold and built for **x86-64-v3** (AVX2,
  Haswell and later), as Architect's own release node is. The host it runs
  on needs those instructions; most VPS CPUs have them, but a QEMU guest
  on a baseline CPU model (`kvm64`) does not.
- **web**: Architect's web app (one wasm for every distribution) with this
  distribution's `/app/distribution.json`. Architect's
  `scripts/web-bundle.sh` runs from an Architect checkout at the commit
  `Cargo.lock` pins (`ARCHITECT_SRC`, or a clone cached under the target
  dir), in its own workspace and target dir.

`cargo xtask dist` in an Architect checkout builds a distribution crate
too, but it runs cargo from the Architect checkout, so Architect's
toolchain file and cargo config would apply to Studio's workspace. The
script builds from this checkout instead and keeps `dist`'s flags.

## Release artefacts

The `studio-architect` workflow (`xtask/workflows/src/studio_architect.rs`)
runs on every release tag `vYY.M.MICRO`. It builds on THEBATTLESHIP's
`thebattleship-release` pool (the AppImage's pool), in a size-capped target
dir of its own in the pool's shared cache, and attaches these to the
tag's one GitHub Release:

| asset | what |
| --- | --- |
| `studio-architect-node-<tag>-x86_64-linux.tar.gz` (+ `.sha256`) | `studio-architect-node-<tag>-x86_64-linux/studio-architect`, the node |
| `studio-architect-web-<tag>.tar.gz` (+ `.sha256`) | `studio-architect-web-<tag>/`: the landing page, `docs/`, `app/` with `distribution.json` |
| `SHA256SUMS` (+ `SHA256SUMS.minisig`) | every asset of the release, signed with the org key |

These follow Architect's own (`architect-node-<tag>-x86_64-linux.tar.gz`,
`architect-web-<version>.tar.gz`), so the infra repo can wrap either one.
The names differ so the two can't be confused. There is no image: the
hosted node runs as a NixOS service.

`SHA256SUMS` is written by the `sums` job (`xtask/workflows/src/sums.rs`),
which runs at the end of both `desktop` and `studio-architect`. It sums
every asset the release has at that point from GitHub's digests, and runs
in the `release-signing` environment (tags only) to sign with
`FTS_RELEASE_MINISIGN_KEY`, whose public half is
`RWRh9Gk9qaStQMhku4THMidEYrQsmAEsm2dDprUkZxShEtqfIcroM8Od`. One
concurrency group per tag means the last run covers everything. Verify a
download with
`minisign -V -P <that key> -m SHA256SUMS -x SHA256SUMS.minisig`, then
`sha256sum -c --ignore-missing SHA256SUMS`.

The build job's summary lists the node's `DT_NEEDED` and interpreter,
which is what `autoPatchelf` has to satisfy on NixOS. On the first build
(2026-11-11, x86_64, 103 MiB binary, a 36.9 MB tarball):

```
NEEDED   libstdc++.so.6  libgcc_s.so.1  libm.so.6  libc.so.6  ld-linux-x86-64.so.2
needs    GLIBC_2.39, GLIBCXX_3.4.29, GCC_12.0.0
interp   /nix/store/…-glibc-2.42-84/lib/ld-linux-x86-64.so.2
RUNPATH  /nix/store/…-devenv-shell-env/lib:/nix/store/…-glibc-2.42-84/lib:/nix/store/…-gcc-15.3.0-lib/lib
```

The interpreter and `RUNPATH` are the build shell's store paths: wrap it
with `autoPatchelfHook` and `stdenv.cc.cc.lib` (libstdc++, libgcc_s) and
glibc. `libstdc++` is Luau's (`mlua-sys`, through Architect's
`architect-scripting`).

A PR that reaches the node (a crate of this repo in its tree, the
manifest, the lockfile or the toolchain) gets the `studio-architect` job in
`PR`: `cargo check --all-targets` of the Linux node on a free hosted
runner, set up as the hosted Gates job is (`mold` and the CI cargo
config, no apt header set). It is a cold check (6m39s on its first run),
it is not required, and PRs that don't reach the node skip it after
`cargo tree`.

## How it relates to the rest of Studio

- **Studio's own app** (`apps/studio/desktop`, `Studio`) is the DAW and
  the rig. It has its own identity and does not run Architect's shell.
  This is Architect's app, with Studio's notes features.
- **The setlists extension in the store** (`setlists-extension`
  workflow, `studio-setlists`) is the same extension as a process, for the
  stock Architect app. It works there, but it can't draw on a node.
  Installed into Studio Architect, it overrides the compiled-in copy, as a
  development copy does.
- **Productions** (`studio-productions`) is compiled in beside it. A
  production's show set is a setlist.

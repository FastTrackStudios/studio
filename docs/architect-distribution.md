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
  id `studio-views`: the Views toolbar at the bottom with the owner's six views, studio#409,
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

## The island (#445)

A share link's now-playing view draws Session's own progress bar (at the top:
sections as played, the playhead, a tap on a section seeks there) and its
transport (under the chart: previous, play / pause, next) when the node
ships Studio's island: `apps/architect/island`, a WASI component built from
`features/ui/transport-ui`, the crate Session draws them from. Architect's
node runs it in the page's extension sandbox, a document of its own with
its own CSP, so the share page's policy is unchanged
(FastTrackStudios/architect#601, `architect_sites::island`). It is given
where the song is and the song's sections (`data-sections` on the chart's
now-playing view, Keyflow's section marks and colours on the chart's clock)
and may ask only to seek, play, pause, or go to the previous or next song.
Its theme variables come from the share page's own theme.

`just island` builds it (in the `wasi` devenv profile: `wasm32-wasip2` and
node) into `<target>/studio-island/` (`<target>` is cargo's target
directory, from `cargo metadata`): Architect's `build-web` folder and its
stylesheet (`apps/architect/island/island.css`, Tailwind's theme and
utilities for the components' classes, no preflight: a view's sheet may not
style `html` or `:host`). It runs `just island-check` first, the island's
clippy for `wasm32-wasip2` (on the host the crate is empty, so a host
clippy checks nothing), warnings denied. **It fetches npm packages at build
time**: `npm ci` in Architect's web tools (copied from the architect
checkout cargo resolved into `<target>/architect-web`) installs jco and its
transpiler, pinned by that folder's `package-lock.json`.

The component is built as `wasm-release` (opt-level `s`, fat LTO, one
codegen unit) with its name section stripped
(`CARGO_PROFILE_WASM_RELEASE_STRIP=true`): 1.20 MB, 314 KB gzipped (the core
module 806 KB / 259 KB, jco's glue 393 KB / 54 KB), down from 1.7 MB /
415 KB unstripped at `release`; plus Architect's web runtime, 48 KB
gzipped, and the sheet, 12 KB / 3 KB. The page loads it only when the view
first opens; Architect's own bar and controls show until its first frame
(about 0.3 s after the view opens on a local node; at 1.6 Mbit/s and 150 ms
it is ready at 8.7 s, with the chart, which is the longer wait, as the node
serves both uncompressed: FastTrackStudios/architect#602), and stay where it
cannot run (Safari, which has no JS Promise Integration yet).

The distribution reads it from `studio-island/` beside the node's own
executable (`studio_architect::island`, `Distribution::island`) and checks
it against its lock first (`studio_architect::island::at`). The lock is
written at build time and names every file a page loads with its digest
(sha256, base64), the stylesheet among them: `just island` writes
`island.css` into the folder and runs `build-web` again, which locks every
file in its folder. Every file the lock names must be there, a plain
name directly in the folder, with the digest the lock gives; the lock
must name the sheet; and the folder, the lock and every file it names
must be a regular file (the folder a directory) of its own, checked with
`symlink_metadata` and again on the opened file: a symlink is refused,
never followed, even to an identical copy, as is a directory or a special
file. A folder that does not match is refused whole, with one warning
(`island.outcome = "refused"`): the node serves no island, and the
now-playing view keeps Architect's own bar and controls, as on a node
shipped without the folder. Only the lock's files and the lock are
served. `just island` writes `BUILT_FROM` beside them (`git describe
--always --dirty` of the tree it was built from, taken before the build;
not locked, not served), and `scripts/studio-architect.sh release`
refuses to package a folder that is missing, was built from another tree,
or from a tree with uncommitted changes, and refuses to release from one.
The release job builds it just
before, in the same target dir (`xtask/workflows/src/studio_architect.rs`,
step "Studio's island": the `wasi` profile's `wasm32-wasip2` target and
node >= 26, checked first).

**Its sheet must pass Architect's `cleanSheet`** (v0.12.4: no `var()` in
an animation or a transition, animations that end; the page refuses the
whole sheet otherwise, and the island draws unstyled). `island.css` keeps
Tailwind inside that: `@theme inline` writes `animate-pulse` as a plain
animation of ten runs, and the `transition-*` utilities (a `var()`
duration) are not generated (`@source not inline`): `duration-*` sets a
plain `transition-duration`, and `transition-property` is `all` by
default, so the bar still eases. `scripts/island-sheet.sh` puts the
compiled sheet through the `cleanSheet` of the architect checkout
Cargo.lock pins, with the island's prefix, and fails on a refusal:
`just island` runs it on the sheet it built, and the PR workflow's
`studio-architect` job compiles and checks it whenever a PR touches the
sheet, `transport-ui`, the island or the lockfile.

What the island stays inside (Architect v0.12.4): at most 8 intents a
second in bursts of 16 (it seeks on a tap, never on a drag), animations
that end, a view at most 4096 px tall in a slot of its own height (the
progress bar's view is 64 px in its 73 px slot and the transport's 56 px
in 65 px, at 390 px wide, neither clipped), and it is stopped 30 s after
its view closes and loaded again on the next open (0.28 s to its first
frame again on a local node, Architect's own bar showing for 0.24 s of
it).

### Serving it from the hosted node

Install the node's tarball as it is laid out: the folder `studio-island/`
**next to the real `studio-architect` executable**. The node finds the
folder from its own executable's path (`std::env::current_exe()`, which
resolves symlinks), so with a Nix package:

```text
$out/bin/studio-architect            the ELF (autoPatchelfHook patches it)
$out/bin/studio-island/              the tarball's folder, copied as is:
    architect.json  architect.lock.json  island.css  BUILT_FROM
    studio_island.js  studio_island.core.wasm  studio_island.core2.wasm
    studio_island.core3.wasm  studio_island.conditional-host-import-{0,1,2}.wasm
```

A `makeWrapper` wrapper keeps the real ELF in `$out/bin` (as
`.studio-architect-wrapped`), so the folder stays beside it; a binary
moved to `libexec/` takes the folder with it. The folder holds no ELF, so
autoPatchelfHook leaves it alone, and nothing in it may be changed after
the build: a file that does not match the lock refuses the whole island,
and so does a symlink anywhere in it (copy the files; do not link them
into the store path).

To check it on the server, curl a share page of a setlist with recordings
and the island's lock:

```sh
curl -s https://<node>/s/<code>/setlists/<setlist>/ \
  | grep -o '<script type="application/json" id="site-islands">[^<]*'
# {"islands":[{"folder":"/_ext/island/studio.transport.<hash>/","id":"studio.transport",…}],…}
curl -sI https://<node>/_ext/island/studio.transport.<hash>/architect.lock.json
# HTTP/1.1 200 OK, content-type: application/json, cache-control: public, max-age=31536000, immutable
```

`"islands":[]` means the node has no island: no folder beside the binary,
or one that does not match its lock (the node's log then has one warning,
`island.outcome = "refused"`).

## Release artefacts

The `studio-architect` workflow (`xtask/workflows/src/studio_architect.rs`)
runs on every release tag `vYY.M.MICRO`. It builds on THEBATTLESHIP's
`thebattleship-release` pool (the AppImage's pool), in a size-capped target
dir of its own in the pool's shared cache, and attaches these to the
tag's one GitHub Release:

| asset | what |
| --- | --- |
| `studio-architect-node-<tag>-x86_64-linux.tar.gz` (+ `.sha256`) | `studio-architect-node-<tag>-x86_64-linux/`: `studio-architect`, the node, and `studio-island/` beside it, its island (`architect.json`, `architect.lock.json`, `studio_island.js`, the `studio_island.*.wasm` the lock names, `island.css`, `BUILT_FROM`). Install the folder next to the node's real binary (not a wrapper or a symlink's folder): the node finds it from its own executable's path |
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
  A song note opens as its workspace (Session's Overview: the progress
  bar, the chart's text over the lyrics, the chart;
  `features/setlists/README.md`) in both, drawn in the app's document when
  compiled in and remotely as a process.
- **Productions** (`studio-productions`) is compiled in beside it. A
  production's show set is a setlist.

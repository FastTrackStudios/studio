---
name: build-performance
description: "Diagnose and fix slow builds or a full disk in this monorepo — target/ growth across worktrees, link times, debuginfo, opt-level choices, cargo-sweep, and cargo rail. Use when builds feel slow, when the dev disk fills up, when deciding whether to change a profile knob in the root Cargo.toml, or before benchmarking any build-time change."
---

# Build performance in this tree

~160 workspace members, one `target/` per worktree, a dozen worktrees.
The two failure modes are **disk** (target dirs in the hundreds of GB)
and **link time**. This is what was measured, what was changed, and how
to measure the next thing without fooling yourself.

## Measure before you change anything

Everything below was established by measurement, not by reading blog
posts. Do the same — most "obvious" Rust build wins are worth nothing
here, and one of them (sccache across worktrees) was worth literally zero
and has since been removed.

```bash
just disk            # per-worktree target/ sizes, largest first
just timings -p foo  # cargo --timings → per-crate Gantt + link tail
du -sh target/debug/{deps,incremental,build}
size --format=sysv <binary> | awk '/^\.debug/{s+=$2} END{print s/1e9" GB"}'
```

That last one is the important one. **Check the debuginfo share of a
big artifact before blaming anything else.**

### The contention trap — read this before benchmarking

This machine has 32 cores and other agents build on it. A background
`cargo rail unify` or another agent's build will make a change look
*slower* than what it replaced. A real measurement was thrown away this
way (O1 "measured" at 316s vs O3 at 221s, purely because load was ~50; the
clean re-run on an idle box gave 220s vs 298s — O1 was 26% FASTER, the
opposite of what the contaminated run said).

Before an A/B: `uptime` and `pgrep -c rustc`. If load is high or rustc
is running, wait. Build each arm into its own `CARGO_TARGET_DIR` under
the scratchpad, sequentially, never in parallel.

Also: only one cargo command at a time per worktree — they share a
target-dir lock and will block each other.

## What is already configured, and why

Root `Cargo.toml`, `.cargo/config.toml`, `nix/modules/toolchain.nix`,
`nix/modules/shells/default.nix`.

### Debuginfo — the dominant cost

`profile.dev` is `debug = "line-tables-only"` + `split-debuginfo = "unpacked"`.

The finding that drove it: the `action_ids` test binary was 1.62 GB, of
which **1.59 GB (98%) was `.debug_*` sections**, and `ld.bfd` copies all
of it into every artifact on every relink. Result: **1.62 GB → 211 MB,
−87%.**

You keep backtraces with file:line, and samply/perf still symbolize.
You lose debugger variable inspection — for that use `--profile dev-dbg`
(full DWARF, otherwise identical to dev).

Platform notes:
- wasm is unaffected — `-Csplit-debuginfo` is unstable on wasm32 and
  cargo omits it automatically. Verified with `cargo build -v`.
- macOS/iOS: `unpacked` is already the platform default.

### Linker

mold, via `-C link-arg=-fuse-ld=mold` in `.cargo/config.toml`.

**Precedence trap:** `target.<triple>.rustflags` *replaces*
`build.rustflags` — cargo does not merge them. The linux table has to
repeat `force-frame-pointers`. If you add a global rustflag, add it in
both places or it silently vanishes on Linux.

mold comes from the devshell. After pulling these changes, `direnv
reload` — otherwise every link fails with `cannot find -fuse-ld=mold`.

Changing any rustflag invalidates **every** cached artifact in the
worktree. Expect one full rebuild, and prefer to `just sweep` first.

### opt-level

- `profile.dev` base: 1 (workspace members).
- `package."*"`: **1** — dependencies. Was 3, which meant full LLVM
  optimization on ~1500 registry crates for every cold build and every
  new worktree. Measured on an idle box, cold build of the `action_ids`
  test binary: **298s at O3 → 220s at O1, −26%.**
- An explicit allowlist back at **3** for audio-thread crates, both
  workspace (`audiocore-dsp`, `daw-audio-graph`, `signal-sampler`,
  the `*-dsp` crates, …) and third-party (`rubato`, `symphonia-*`,
  `rustfft`/`realfft` + `transpose`/`strength_reduce`/`primal-check`,
  `dasp_*`, `zstd-*`, `blake3`, `phon*`).
- `build-override`: opt-level 3, debug off — build scripts and proc
  macros. `package."*"` does **not** cover these, so syn/facet/
  architect/dioxus-rsx were building at O0 and then executing thousands
  of times per build.

**A dev run of the rig must never xrun.** That is why the allowlist
exists. If you hit xruns on a path it doesn't cover, run `--release`
(launch paths already do) or add the crate — do not raise `"*"` back
to 3 wholesale. Regenerate the list with `cargo tree` over the audio
crates; note `package."*"` matches dependencies only, never workspace
members.

## One build for working: dx's `desktop-dev`, for the host triple

Since 2026-10-31 everything run while working builds ONE way: the window
(`dx serve`), the engine (`studio engine`), the app recipes (`just app`,
`just guitar`, `blitz_shot`, `prepare`, …) and the tests (`just dev-test`).

- **Profile `desktop-dev`**, declared in the root `Cargo.toml` as `inherits
  = "dev"`: what `dx serve` builds the window as. The dev profile's package
  overrides (the audio allowlist at opt-level 3) carry over.
- **`--target <host>`**, as dx passes it: artifacts land in
  `target/<host>/desktop-dev`. Without `--target` the same profile builds
  into `target/<profile>` and shares nothing with dx.
- **`target/desktop-dev` is live too.** With `--target`, cargo builds the
  HOST half of the graph — build scripts and proc-macros — into
  `target/<profile>`, not under the triple. Its timestamps can look stale;
  delete it and every crate that uses a proc-macro rebuilds (done once by
  mistake on 2026-10-31). Clear it only together with the triple's dir.
- **Features `cli,signal-keys-rig`** on studio-desktop. `just dev-test`
  names studio-desktop with them so the crates under test unify their
  features with the app's, and filters to the crates asked for.

Measured: with dx's build warm, building the engine (`--bin studio`) this
way compiled only the 102 workspace crates the CLI adds; every registry
crate came from dx's build. Before, dx (`desktop-dev`), the engine
(`release-fast`) and the agents' tests (`target/debug`, the test crate's own
features) each recompiled the graph their own way, and a test build with
nothing changed took 135 s.

The app must be fast in this build — if it is slow here, the code is doing
too much. `release-fast` (release, incremental, 256 units) and `--release`
are for shipping (`scripts/desktop/*`, `scripts/signal/mac-release.sh`) and
measuring performance on what ships, not for working.

## The build cache is kache (sccache stays gone)

Every cargo goes through **kache**, the fleet's `build.rustc-wrapper`
(`~/.cargo/config.toml`, from nix-config `modules/coding/lang/kache.nix`;
the shared store on THEBATTLESHIP is `/run/media/Development/.kache`). The
`kache` skill is the reference: how keys work, the rules that keep them
stable, and how to diagnose a miss. What it means in this tree:

- **Never set a `RUSTC_WRAPPER` here** — the fleet owns it.
- **dx:** the dev shell sets `DX_NO_RUSTC_WRAPPER=1`, which our patched dx
  (`nix/modules/dx-no-rustc-wrapper.patch`) reads to skip its own rustc
  workspace wrapper in plain builds; kache passes any other wrapper chain
  through uncached. Hot-patch builds stay uncached.
- **Path-free code:** find the checkout at run time with
  `utils::paths::checkout_path(rel)`, never `env!("CARGO_MANIFEST_DIR")`
  outside tests — kache keys that by its absolute path, so every worktree
  would recompile the crate and everything above it.
- **Build scripts** write generated `include_*!` paths relative to the
  generated file (`features/rigs/guitar/build.rs`) or copy into `OUT_DIR`
  (`apps/plugins/mic/build.rs`).
- **Target dirs are named `target`** (or live outside the checkout): kache
  keeps any other in-checkout name in the key.

Measured 2026-10-31: a fresh worktree builds the engine in 21 s, 1,709 of
1,709 crates restored (417 s cold). sccache, removed in August 2026, gave
0% across worktrees because the target dir was in its key, and refused
dx's wrapper outright.

## Disk: cargo never garbage-collects

There is no cargo GC on stable. A long-lived worktree accumulates a new
copy of every artifact each time a fingerprint changes — **56 stale
copies of a single crate** were measured, plus 77 G of
`debug/incremental` in one worktree.

```bash
just sweep              # this worktree, keep artifacts touched in 7d
just sweep-all          # every worktree from `git worktree list`
just sweep-incremental  # drop incremental caches (pure cache, always safe)
```

**Never delete another agent's worktree target dir** without asking —
most worktrees here belong to other agents mid-task. `just sweep-all`
is a time-based sweep, not a wipe, which is why it's safe.

## cargo rail — a hygiene tool, NOT a build-time tool here

`.config/rail.toml` is configured for unused-dep detection, dead-feature
pruning, and version unification. A full `unify --check` was run
2026-08-09. **Verdict: do not reach for it to speed up builds.**

What it found:

- **Zero unused dependencies.** Every candidate came back "preserved …
  a manifest feature edge references this declaration". The tree is
  already clean, so there is nothing to delete and no compile time to
  win. Same for features: "216 optional features (user-facing, not
  removed)".
- Its "changed: 43 workspace deps, 388 member edits across 192 crates"
  is version unification, workspace inheritance and key sorting — churn,
  not removal.
- **Applying it would loosen deliberate pins**: it converts `ort` and
  `winit` from `=x.y.z` to caret. `ort` is load-dynamic against a
  specific onnxruntime; do not let that float. Use `unify.exclude` or
  `exact_pin_handling = "skip"` first if you ever do apply it.
- One genuinely actionable dedup: **`dirs` is in the tree at both major
  6 and 5**, so it compiles twice. rail skips it rather than picking.

Cost: `unify --check` takes **well over an hour** on a cold
`compiler_diag_cache`, prints nothing while it works (it is compiling
the tree), and `--explain` took another 40+ min even with the cache
warm. Run it with `run_in_background`, give it an isolated
`CARGO_TARGET_DIR` so it doesn't take the worktree's target lock, and
don't benchmark anything while it runs (see the contention trap).

For a fast dedup signal instead: `cargo tree --duplicates --workspace`.
Baseline was **469 duplicated packages**, mostly transitive through the
Blitz/servo stack.

## Ideas already evaluated — don't redo these

- **Shared `CARGO_TARGET_DIR` across worktrees** — causes fingerprint
  clobbering with concurrent agent builds. Use worktree-local targets.
- **sccache, in any form** — 0% across worktrees, and it breaks `dx`. The cache is kache (above).
  Removed Aug 2026; see above.
- **`-Z threads=N` (parallel frontend), cranelift** — need nightly; the
  workspace is pinned to stable 1.94.

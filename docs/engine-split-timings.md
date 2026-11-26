# Splitting studio-engine: build timings

What the split (docs/MONOREPO-MIGRATION.md, "Splitting studio-engine") is
measured against. Every number is one build of the engine's program:

```sh
devenv shell -- cargo build --profile desktop-dev --target x86_64-unknown-linux-gnu \
  -p daw-extension --bin daw-extension --timings
```

on THEBATTLESHIP, in the `engine-split/core` worktree with its own target
dir (`monorepo-targets/engine-core/target`), cargo 1.99.1, kache on.

## Baseline: before the split (2026-11-26, at 5f8e83558)

| Build | Wall | `studio-engine` unit | `daw-extension` |
|---|---|---|---|
| Fresh target dir (dependencies from kache) | 2 m 41 s | 54.9 s | — |
| `touch src/listen.rs` | 3.4 s | kache hit (same content) | — |
| A one-line edit in `src/listen.rs` (a comment) | 61 s | 57.5 s | 4.3 s lib + 3.2 s bin |

- A touch costs nothing: kache keys on content, so only a real edit
  measures the edit loop. Measure step 4 with a real edit too.
- The edit loop is the `studio-engine` unit (87.5k lines in one crate):
  about 57 s of the 61 s.

Reports (in the target dir, not committed):

- fresh: `/run/media/Development/t3-worktrees/studio/monorepo-targets/engine-core/target/cargo-timings/cargo-timing-20261126T013512010Z-d5712a9e7ea61724.html`
- one-line edit: `/run/media/Development/t3-worktrees/studio/monorepo-targets/engine-core/target/cargo-timings/cargo-timing-20261126T013828516Z-d5712a9e7ea61724.html`

## After the split (2026-11-26, at the merge of the five domain crates)

The same build, in the merged worktree, load average about 30 (the
baseline host was busy too).

| Build | Wall | Units rebuilt |
|---|---|---|
| A one-line edit in `engine-listen/src/listen.rs` (a comment) | **29.7 s** (was 61 s) | engine-listen 12.1 s, studio-engine 13.3 s, daw-extension 9.2 s lib + 5.8 s bin |

- The domain crate itself is now 12 s, not 57 s. What's left is the
  layers above it: `studio-engine` (10.5k lines, the assembly) and
  `daw-extension` rebuild on every edit below them. Folding
  `daw-extension` into the engine binary (the two-binaries decision) and
  keeping `studio-engine` thin is where the next seconds are.
- The engine binary did not build on its own after the split:
  `daw-extension` uses `daw_ui::studio` and only got the `studio` feature
  through feature unification. It now asks for it; the song-timing model
  it reads should move to a core crate so the engine needs no UI kit.

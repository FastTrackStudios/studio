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

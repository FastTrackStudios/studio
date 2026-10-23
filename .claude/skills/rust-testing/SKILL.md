---
name: rust-testing
description: "Write, place, prune and speed up Rust tests in this workspace — where a test lives (unit vs one integration binary per crate), what a test must guard, what never to delete, how nextest is configured (slow-timeout overrides, test groups, profiles), and how to find and remove redundant or slow tests. Use before adding a tests/*.rs file or a #[test], before raising a slow-timeout in .config/nextest.toml, when a test is flaky or slow, when CI's test job is slow, and when auditing a crate's tests for duplicates."
---

# Rust tests in this tree

~11 000 tests, ~1460 crates and **814 test binaries** in CI's
`studio-tests` job (20 min on the warm Mac Pro). Most of that time is
**building and linking test binaries, not running tests**: every
`tests/*.rs` file is its own binary, and each one links its crate's whole
dependency tree again. So the cheapest test is one that adds no binary,
and the most expensive habit is a new file under `tests/` per behaviour.

CI already narrows which tests *run* (`cargo rail plan` →
`scripts/ci-rail-plan.py` → a nextest filterset of the affected
packages), but the test *build* stays `--workspace`. A new binary costs
every PR; a slow test costs every PR that touches its crate or anything
it depends on.

## Where a test lives

| What you are testing | Where it goes |
|---|---|
| Logic inside one crate, including private items | a unit test: `#[cfg(test)] mod tests;` in `foo.rs`, the tests in `foo/tests.rs` (a separate file, so editing a test does not recompile the library) |
| The crate's public API, end to end | **the crate's one integration binary**: `tests/it/main.rs` with a `mod` per area (`tests/it/routing.rs`, `tests/it/presets.rs`) |
| Something needing a real REAPER | `#[reaper_test]` — not in CI; see the `reaper-testing` skill |
| Something a human looks at (screenshots, renders for inspection) | `#[ignore]` + a `just` recipe that runs it with `--ignored` (precedent: `just ee-shots`) |
| A timing budget on real data | gated on an env var and skipped without it (precedent: `perf_slip`, `FTS_PERF_SESSION`) |

**Do not add a new top-level `tests/*.rs` file to a crate that already
has one.** Add a module to the existing binary instead; if the crate has
several, the new test goes into a `tests/it/` binary and the next person
to touch the old files moves them in too. Merging a crate's
`tests/*.rs` files into `tests/it/` is a mechanical, always-welcome
change: `git mv tests/foo.rs tests/it/foo.rs`, `mod foo;` in
`tests/it/main.rs`, shared helpers become `tests/it/common.rs` (one
`mod common;`, not one per file). Watch for two files that each define
the same helper or the same `#[tokio::test]` name — they now share a
namespace.

**The one exception: a test that installs a `#[global_allocator]`**
(`dsp_core::install_counting_allocator!()`, the realtime no-allocation
tests). A binary has one allocator, and a counting one would count every
other module's allocations too, so each of those files stays its own
`tests/<name>.rs` binary.

Doctests are binaries too (one per doctest). Internal crates should
carry examples as unit tests; a published crate keeps doctests and
`just test-doc` runs them (nextest cannot).

## What a test must guard

Before writing a test, name the invariant it holds and the change that
would break it. If you cannot, it is not a test yet. A test earns its
place when it:

- **asserts on the whole value** (`assert_eq!(cfg, Config { .. })`),
  not `is_ok()` or one field of five;
- **fails for one reason**, named in its title
  (`a_muted_child_changes_the_picture`, not `test_render_2`);
- **syncs on events, not time** — wait for the message, the state, the
  channel; `sleep` as synchronisation is a flake with a delay in front of
  it. If you must poll, poll a condition with a deadline, and keep the
  deadline generous: the box is often at load 80+ during a build;
- **is deterministic** — `BTreeMap`/sorted output wherever order reaches
  an assertion, fixed seeds for anything random;
- **is not already said** — before adding a case, grep the crate for a
  table-driven or round-trip test that already covers it, and add a row
  there instead of a new function.

## What never to delete

These look redundant to a quick read and are not:

- **`tests/param_round_trip.rs`** in every effect — the repo rule that a
  parameter parses the string it prints. Each effect has its own on
  purpose.
- **Golden / bit-exact vectors** (DSP output, committed fixtures, the
  golden session's scene renders) — they catch the change nobody meant.
- **Regression tests** whose name, doc comment or commit cites a bug,
  issue (`STA-…`, `#…`) or a specific knob combination. The reverb
  stability sweep is long *because* the bug only showed at certain
  settings — trimming it drops the coverage that found it.
- **Realtime / no-allocation tests** on processing cores.
- **The only test of a branch.** Delete a "duplicate" only after reading
  both tests and confirming they reach the same code with inputs that
  take the same path.

## What to delete

- Two tests asserting the same thing through the same path (copy-paste
  with a different literal that hits no new branch).
- A unit test fully contained in a broader one (a table row that already
  exists; an integration test whose assertions are a superset).
- Tests of a `derive` (`Debug`, `Clone`, `Default`, trivial serde),
  tests that assert a constant equals itself, tests that only check a
  constructor does not panic, tests that assert a mock returns what it
  was told to.
- `#[ignore]` tests with no recipe that runs them and no note on why —
  either give them a `just` recipe or delete them.
- Tests of code that is gone or deprecated.

When removing, put the reason per test in the commit body (`subsumed by
<path>::<test>`), so a reviewer can check each claim. Removing tests
needs no change file unless the crate's behaviour changes.

## Speed: in order of payoff

1. **Fewer binaries.** Merge `tests/*.rs` into `tests/it/` (above).
   Saves link time on every CI run and every `just test`, and target/
   disk (see `build-performance`).
2. **Things nobody asserts on stay out of the PR run.** A test that
   paints pictures for a human, with no assertion, is a `just` recipe,
   not CI: `#[ignore = "visual inspection, asserts nothing: just <x>-shots"]`
   and a `just` recipe that runs it with `--include-ignored` (precedent:
   `just ee-shots`, `just comp-shots` … `just daw-ui-shots`). A screenshot
   test that *does* assert (a panel opened, a picture matches a committed
   fixture) stays in the suite.
3. **Make a slow test cheaper before you raise its timeout.** Render
   fewer seconds, a lower sample rate, a smaller grid — when the shorter
   version still fails on the bug it guards. If it cannot be cheaper
   without losing that, raise the timeout (below) and say why.
4. **Contended resources go in a test group**, so they queue instead of
   starving each other and the rest of the suite into timeouts:

   ```toml
   [test-groups]
   gpu = { max-threads = 1 }

   [[profile.default.overrides]]
   filter = 'test(=every_scene_renders_to_its_committed_fixture) | test(=every_zoom_renders_to_its_committed_fixture)'
   test-group = 'gpu'
   ```

   `threads-required = N` on an override makes one test count as N
   threads, for a test that is itself parallel (the fill detector's
   album check analyses six songs at once).
5. **Delete the redundant** (above). Usually the smallest win per test,
   but it compounds and it makes the suite easier to read.

## `.config/nextest.toml`

The default `slow-timeout` is **30 s, terminate after one period**: a
test that takes longer is killed. Every override in that file follows
one rule, and a new one must too:

- **One override per test, filtered by its exact full name** — with one
  integration binary per crate that is `test(=<module>::<name>)`
  (`test(=fills_real::fills_land_where_a_drummer_puts_them)`), not a
  pattern that silently catches the next test someone adds.
- **A comment that says what the test does, the time measured (idle
  and under load if you have both), and why it cannot be cheaper.** "It
  was red" is not a reason; "it renders eleven GPU frames of a 233-track
  session" is.
- **Period well above the measured time**, `terminate-after` small: the
  box runs tests beside a workspace build, and a timeout that only just
  fits idle goes red under load — which teaches everyone to rerun a red
  suite instead of reading it. A hang must still be caught within
  minutes.

Changing `.config/nextest.toml` widens the next CI plan to the full set
(`.config/rail.toml [plan.work.*]`), so batch nextest config changes.

## Flaky tests

1. Find the race: the line where the test assumes an event arrived or a
   state changed without proof.
2. Replace the timing with a wait on that event; make the assertions
   order-independent (sort, compare sets).
3. Prove it:
   `cargo nextest run -p <crate> -E 'test(=<name>)' --stress-count 50 -j 2 --no-fail-fast`
   (run it while the machine is busy, too — CI is).
4. Say in the commit what the timing assumption was and what replaced
   it. Do not add `retries` to hide it.

## Running

```bash
just test                                   # the CI feature set; nextest, no doctests
just test-doc                               # doctests
cargo nextest run -p <crate>                # one crate
cargo nextest run -p <crate> -E 'test(/routing::/)'
cargo nextest run -p <crate> --test it      # the crate's integration tests
cargo nextest run -p <crate> -E 'test(/^stability::/)'   # one former test file
cargo test -p <crate> --test it -- screenshots:: --include-ignored   # an ignored harness, libtest-style
cargo nextest list -p <crate>               # what exists, without running
```

`just test`'s feature list matters: the app CLIs and the worklet rigs
sit behind features, and their tests never build without them. Never
run `cargo test --workspace` for the suite (no per-test processes, and
it rebuilds a different artifact set from nextest's).

Credit: the test-shape checklist borrows from
[pproenca/dot-skills `rust-write-tests`](https://github.com/pproenca/dot-skills/tree/main/skills/.curated/rust-write-tests)
(MIT); the binary-count argument is matklad's
[Delete Cargo Integration Tests](https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html).

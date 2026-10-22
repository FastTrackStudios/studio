# ADR 0002: a studio install becomes a Task device node by embedding LocalNode

- Status: accepted for the spike; enabling it in an app waits on the plugin-gated re-measure (C1 spike, STA-362; connect-anywhere spec §2 and C1 in §9)
- Date: 2026-10-22
- Code: `apps/session/node/` (crate `session-node`, feature `task-node`),
  `apps/session/node/tests/endpoint.rs` (the node on a temp root returns
  its endpoint id), `apps/session/node/src/bin/probe.rs` (the size probe).

## Question

Task's `offline.peers` makes every native install a node: it holds the orgs
it was admitted to, binds one iroh endpoint per org, keeps the admitted set,
and syncs documents peer to peer. Signal and Session have to join that mesh
as devices. How?

- **A. Embed Task's `LocalNode`** (`task_server::local_node`) behind a studio
  feature: the same backend Task's desktop and phone apps host
  (`task-node-host`).
- **B. Extract a thin device-node library** from task-server: account pass,
  endpoint per org, admitted set, doc lane, and nothing else.

The spec's default is A if its size cost is acceptable.

## Architect pin

Studio pins architect v0.9.9; task (main and dev) pins v0.9.6. The changes
v0.9.6..v0.9.9 are additive (the Action List, architect-mcp, telemetry
timing, mDNS LAN discovery, `crdt::peer` document exchange). Studio's existing
`[patch."https://github.com/FastTrackStudios/architect"]` table already lifts
task's `*-proto` crates onto v0.9.9; this change adds the four crates only
task-server reaches (`architect-auth`, `architect-permissions-proto`, `auth`,
`auth-proto`), so `Cargo.lock` keeps one architect source (boundaries
`single-source`). Task moving its own pin to v0.9.9 drops the need for the
patch rows but is not a blocker.

## Measurements

Measured on airlock (M4 mini, 10 cores, 16 GB) on 2026-10-22, in the
canonical studio checkout under the build lease. Studio at 8b14916 (this
branch, on dev 77d05552); task-server at task main 1da9c77c, with default
features. Logs are in `~/sta-362/` on airlock.

### Size

The probe is `session-node-probe` (`src/bin/probe.rs`), built with
`cargo build --release`. Without the feature it only starts a tokio
runtime. With `--features task-node` it also boots the node. The
stripped column is `strip -x`.

| target | baseline (raw / stripped) | with `task-node` (raw / stripped) | delta (raw / stripped) |
|---|---|---|---|
| aarch64-apple-darwin | 777,136 B / 658,744 B | 131,057,360 B / 110,066,728 B | +130.3 MB / +109.4 MB |
| aarch64-apple-ios | 788,268 B / 672,012 B | 131,080,656 B / 109,957,032 B | +130.3 MB / +109.3 MB |

For comparison, today's release iOS executables (dx, `target/dx/.../release/ios`)
are 175.6 MB for SessionDesktop and 185.6 MB for SignalDesktop. The delta is
an upper bound for an app: Session and Signal already link tokio, vox,
architect, loro and rustls, so in the real binary the shared code is not
counted twice. In the worst case the node adds about 70% to the Session iOS
executable.

### Build time

These are wall-clock times on a tree that was warm for studio's own crates
but cold for the ~235 crates task-server adds.

| step | darwin | iOS |
|---|---|---|
| `cargo build --release` probe, baseline | 6 s | 6 s |
| `cargo build --release` probe, `task-node` | 522 s | 495 s |
| `cargo check -p session-node --all-targets` (node in, cold) | 100 s | 149 s |
| `cargo test -p session-node --test endpoint` (debug build + run) | 369 s | n/a |
| `cargo check --workspace --all-targets` | 472 s | see below |

So the node costs about 8 to 9 minutes for one cold release build per
target. After that, incremental builds only recompile it when the task pin
moves.

### Gates

- `cargo check --workspace --all-targets` on aarch64-apple-darwin: rc 0.
- `cargo check -p session-node --all-targets --target aarch64-apple-ios`: rc 0.
- `cargo check --workspace --all-targets --target aarch64-apple-ios`: rc 101,
  but it fails the same way on dev without this change (8df11936, rc 101). The
  error is the `ort-sys` build script: `features/session`'s `tts` feature
  pulls `chatterbox-rs` → `ort`, which has no iOS onnxruntime. That is a
  base-tree problem, not one this change introduced.
- `tests/endpoint.rs`: passes. The node booted on a tempdir and returned
  endpoint id `af1e151d…770328f` for its personal org. The darwin release
  probe also booted and printed `3e82d7c6…4da77807f`.

### Found on the way

task main's `task-server --no-default-features` (any plugin subset) does not
compile: 97 errors. `apps/server/src/mcp.rs` and `notifier.rs` name
`scheduling_proto`, `email_proto`, `contacts_proto`, `agent_proto` and
`git_proto`, and `OrgAppState`'s `email`, `scheduling` and `contacts` fields,
without the plugin cfg. So today the node can only be embedded with every
plugin in it (mail, agents, git, finance, …). That is where most of the
size comes from.

## Decision

**A: embed Task's `LocalNode`**, behind `session-node/task-node` (off by
default). The probe proves the four things the spec asks of a device node,
all in-process, for both Apple targets: the account pass (`admit_account`),
an endpoint per org, the admitted set (`admit_peer`/`join`) and the doc lane.
Task's own mobile app already ships this exact node on iOS.

B would mean a second implementation of auth, the org root, iroh presence and
the device pull, which would then have to track task-server. The measured cost
of A is size, and most of that is plugins a studio node does not need. So the
answer to the size is to make task-server's plugin features real, not to fork it:

1. A Task fix to make `task-server --no-default-features --features
   plugin-fasttrackstudio` compile (gate the plugin uses in mcp.rs and
   notifier.rs).
2. Then this crate takes `default-features = false, features =
   ["plugin-fasttrackstudio"]`, and the probe is measured again.
3. Revisit B only if the plugin-free node still adds more than about 50 MB
   stripped on iOS.

Until then, no Session or Signal app enables `task-node`. Turning it on in a
shipping app is a separate decision, made with the re-measured number.

## Consequences

- `session-node` sits under `apps/` because the node hosts transports
  (architect `server` + `iroh`), which `boundaries.toml` allows only there.
- The feature is off by default. An app opts in with
  `session-node/task-node`; the crate's own tests turn it on through a
  self dev-dependency, so `cargo check --workspace --all-targets` compiles
  the embedded node on every PR.
- task-server comes in with default features (every plugin) until the Task
  fix above lands. The core lanes (auth, files, docs, the iroh presence and
  the device pull) are not features.
- iOS builds OpenSSL (`openssl/vendored`) and bzip2 (`bzip2/static`) from
  source, iOS only. This is the same recipe Task's `apps/mobile` uses.
- The task git deps move from 65559094 to 1da9c77c (task main tip), which
  also bumps the task protos studio already uses. The darwin workspace check
  is green with that bump.
- Every `cargo check --workspace --all-targets` now compiles the node
  through the crate's self dev-dependency.

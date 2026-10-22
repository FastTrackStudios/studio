# ADR 0002: a studio install becomes a Task device node by embedding LocalNode

- Status: proposed (C1 spike, STA-362; connect-anywhere spec §2 and C1 in §9)
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

MEASUREMENTS_PENDING

## Decision

DECISION_PENDING

## Consequences

- `session-node` sits under `apps/` because the node hosts transports
  (architect `server` + `iroh`), which `boundaries.toml` allows only there.
- The feature is off by default. An app opts in with
  `session-node/task-node`; the crate's own tests turn it on through a
  self dev-dependency, so `cargo check --workspace --all-targets` compiles
  the embedded node on every PR.
- task-server comes in with every plugin off but `plugin-fasttrackstudio`
  (songs, setlists, live). The core lanes (auth, files, docs, the iroh
  presence, the device pull) are not features.
- iOS builds OpenSSL from source (`openssl/vendored`, iOS only), the same
  recipe Task's `apps/mobile` uses for architect-auth's passkeys.

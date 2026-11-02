# Session — agent instructions

Session is FastTrackStudio's setlist, song, live-sync and song-library domain.

## Layout

`session` is **one crate**; its roles are cargo features, per architect's
crate-layout ADR (architect `docs/content/architecture/layout.md`). Read
that before adding a crate or a flag.

- Contract (entities, wire types, `#[architect::rpc]` traits): `src/proto/`.
- Roles: `ui` (Dioxus, `src/ui/`), `engine`, `net`, `collab`, `library`,
  `reaper`, `tts`, `vault-sync`, `web` / `desktop`. Flags compose one way:
  `ui` never implies a backend, a backend never implies `ui`.
- Apps (`apps/session/*`) depend on `session` with the flags they need.
- The REAPER extension is a separate binary, so it is its own crate:
  `features/adapters/reaper/session-extension`.

## RPC services

- Service traits use `#[architect::rpc]` (see `src/proto/services/`).
- Stream with `vox::Tx<T>` / `vox::Rx<T>`.

## Async

- Spawn and time with `architect::platform::{spawn, sleep, timeout}`: tokio
  on native, `spawn_local` on wasm.
- Locks and channels come from `tokio::sync`; release std sync primitives
  before any `.await`.

## Build & test

```bash
cargo check -p session                 # default features
cargo check -p session --features ui   # with the UI role
cargo test -p session                  # dev-deps turn on engine, net, tts, ui
```

## Before you push: `just ci` (at the repo root)

**Run `just ci` and get a green run before every push.** It runs exactly
what `.github/workflows/checks.yml` runs, in the same order — lockfile
drift, `cargo fmt --check`, the flow verification gate, the web tailwind
sheet, `cargo check --workspace`, `cargo nextest run --workspace` — and
stops at the first failure, because CI does too and a later step built
on a broken one tells you nothing.

It refuses to start with less than 25 GB free: a build that runs out of
room dies in the linker with a bus error that reads as a broken
toolchain, not as a full disk. `just sweep-incremental` reclaims the
most, `just disk` says where it went.

`just ci <step>` starts from a step when you have already passed the
cheap ones and are iterating: `lockfile`, `fmt`, `flows`, `tailwind`,
`check`, `nextest`.

A full run is a few minutes against a warm `target/`. A red CI round
trip is far longer, and on a shared self-hosted runner it blocks
everyone else's checks while it fails.

## Issue tracking

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

## btca — Source Code Search

Use **btca** to query the actual source code of key dependencies before implementing features or debugging.

```bash
btca ask -r <resource> -q "your question"
btca resources   # list all available resources
```

### Relevant Resources

| Resource | Repo | Description |
|----------|------|-------------|
| `facet` | facet-rs/facet | Rust reflection — shapes, derive macros, serialization |
| `roam` | bearcove/roam | RPC service framework — service traits, streaming, SHM |
| `moire` | bearcove/moire | Instrumentation — task spawning, sync primitives |

# Session — Claude Code Instructions

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

Run `just ci` at the repo root before pushing.

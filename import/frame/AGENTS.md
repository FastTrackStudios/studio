# frame: agent instructions

- The core crate, `frame` (`crates/frame`), has its layers behind features (`io`, `render`,
  `runtime`, `live`, `embed`, `figma`, `design`, `theme`, `nice-plug`, `host-scene`, …) and no
  architect, vox, Dioxus or figcraft dependency (`just boundary`, in CI). Anything that needs
  architect goes in `crates/frame-architect` (the `SurfaceEngine` vox service, the mock engine,
  `remote`, `library`). The tools are in `crates/frame/tools`, the plugin faces in
  `examples/plugin-faces`, the parity harness in `xtask`.
- Build inside `nix develop` and through the `just` recipes (`just check`, `just test`,
  `just parity`). Don't set `RUSTFLAGS` or `CARGO_TARGET_DIR`: the rustflags tables in
  `.cargo/config.toml` are Studio's on purpose, so kache shares builds across the fleet's repos.
- The pins (architect, Dioxus, Blitz, vox, the `[patch]` tables) follow architect's release tags;
  consumers repeat the same `[patch]` tables.
- Faces are generated (`just faces`), never committed.
- Until Studio depends on Frame by tag (FastTrackStudios/studio#373), Studio commits that touch
  Frame are replayed here with `just catch-up <studio checkout>`.
- figcraft comes from our fork by tag, only in `frame-doc` and `frame-editor` (`docs/figcraft-fork.md`, `NOTICE`).
- The plan: FastTrackStudios/frame#2.

# frame

A design and control-surface engine: a native document format (`.frame`
bundles and a lossless text markup), Figma import, a Vello renderer held to
pixel parity with Figma, and a runtime for fully mapped, AI-designable
control surfaces and plugin faces (see
[docs/control-surface-roadmap.md](docs/control-surface-roadmap.md)).

Frame's core, `frame`, depends on no architect, vox or Dioxus crate (CI
checks it), so a plugin GUI or any host can link it alone.
`frame-architect` connects it to [architect](https://github.com/FastTrackStudios/architect)
apps: the `SurfaceEngine` vox service, the mock engine, remote engines and
surface libraries.

```
crates/frame                 the core crate; its layers are features:
  src/document               model + ops (proto), Loro replica, .frame bundle, markup, io
  src/render                 Scene → paint → Vello / SVG; effects stage (WGSL)
  src/surface                catalog, controls, bindings, engine contract, runtime, kit
  src/motion, feed, theme    animation, typed feeds, host views drawn from documents
  src/design                 lint + the tool session (the AI authoring surface)
  src/figma                  Figma import
  src/live, embed            hosts: a texture on the host's wgpu device; strips
  render/tests/parity        the Figma parity scenarios
crates/frame-architect       on architect: SurfaceEngine over vox, the mock engine,
                             remote engines (feature remote), libraries (feature library)
  tools/cli                  frame-cli: render_surface, export_markup, parity/diagnostics
  tools/desktop              `frame`: the live surface player (hot reload, mock engine)
  tools/mcp                  frame-mcp: an MCP server to design surfaces with a model
  tools/library-server       serves a folder of surfaces as a live library
examples/plugin-faces        hardware-style plugin faces written with the surface kit
xtask                        cargo xtask: the parity harness and its scripts
docs                         design notes, parity specs and reports
```

## Develop

```sh
nix develop          # the toolchain (1.99.1), GUI libraries, Vulkan loader, mold
just check           # what CI checks (with fmt, clippy and `just boundary`)
just test
just parity          # the Figma parity suite (needs a GPU)
just faces           # generate the plugin faces into examples/plugin-faces/generated

cargo run --release -p frame-desktop -- crates/frame/examples/surfaces/01-stage.frame   # hot-reloads on save
```

The faces are generated, not kept in git (`just faces`, about 130 MB). The
parity scenarios, `regression.tough-1176-blue-face` included, are in
`crates/frame/render/tests/parity/scenarios`.

## Using Frame

Pin Frame by tag, repeat its `[patch]` tables (a dependency's patches don't
reach the app that uses it), and start a new app's `Cargo.lock` from Frame's:
a fresh resolve can pick crates newer than the pins build with. `examples/consumer` is this snippet as a
crate, and CI builds it against every commit (`just consumer`).

```toml
[dependencies]
frame = { git = "https://github.com/FastTrackStudios/frame", tag = "v0.1.0", features = ["io", "render", "runtime"] }
# On architect: the SurfaceEngine vox service, the mock engine, remote engines, libraries.
frame-architect = { git = "https://github.com/FastTrackStudios/frame", tag = "v0.1.0", features = ["remote"] }

# Frame's [patch.crates-io], copied from its Cargo.toml at that tag.
```

Frame's architect, vox and Dioxus pins match one architect tag (now
v0.10.0), so an app on that architect tag shares one version of each.

## Releases

Semver, like architect: `vX.Y.Z` tags on a green `main`, trunk-based, no
release branches or commits. Before 1.0, a minor bump may break the API;
the PR and the tag's notes say how. When architect cuts a tag, Frame moves
with the fleet: from an architect checkout,
`just fleet-bump vX.Y.Z ../frame` repoints Frame's architect crates, then
Frame copies that tag's other pins and `[patch]` tables in the same PR.

## History

Frame lived in [Studio](https://github.com/FastTrackStudios/studio) from
2026-10-12 to 2026-11 (`crates/frame`), where its 31 crates were folded into
one. Its history was cut out of Studio with `git filter-repo` and grafted
onto this repo's `2dae10b`, so `git log --follow` runs from March 2026
through Studio to now. Commits carried over from Studio end with a
`Studio-commit: FastTrackStudios/studio@<sha>` trailer. Until Studio depends
on Frame by tag, `just catch-up <studio checkout>` replays Studio commits
that touched Frame (`scripts/frame-catch-up.sh`).

## Licence

Frame is licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT), at your option. Unless you state otherwise,
any contribution you submit for inclusion in Frame is dual licensed as above,
without any additional terms or conditions.

# frame

A design and control-surface engine: a native document format, Figma import,
a vello renderer held to pixel parity with Figma, and, in progress, a runtime
for fully mapped, AI-designable control surfaces (see
[docs/control-surface-roadmap.md](docs/control-surface-roadmap.md)).

frame is an [architect](https://github.com/FastTrackStudios/architect)
consumer repo.

```
apps/frame/cli           frame-cli: render / diff / diagnostics binaries
crates/frame             facade crate: what apps and consumers depend on
features/document        the frame document model + file format (architect entity/service)
features/figma           Figma import only: figma-model (transitional), figma-rest, plugins/
features/render          renderer (paint primitives → vello, SVG export) + tests/parity
xtask                    cargo xtask: parity harness + scripts/
```

## Develop

```sh
nix develop          # GUI libs + Vulkan loader for the wgpu parity renders
just check
just test
just parity          # Figma parity suite (features/render/tests/parity)
just scaffold-feature <name>
```

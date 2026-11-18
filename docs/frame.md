# Frame in Studio

Frame, the design and control-surface engine, is its own repo
([FastTrackStudios/frame](https://github.com/FastTrackStudios/frame)).
Studio depends on its core crate, `frame`, as a git dependency pinned by
`rev`, declared once in the root `Cargo.toml` (`[workspace.dependencies]`).
Studio carried a copy at `crates/frame` from 2026-10-12 to 2026-11-17
(studio#510).

What stays here:

- `features/ui/plugin-faces`: the plugin and rig faces, written with Frame's
  surface kit; `just faces` generates them into
  `features/ui/plugin-faces/generated`.
- The hosts that draw them: `apps/session/ui` (`faces`, `fx_view`, the
  themed mixer in `skin`; `docs/frame-themes.md`) and `signal:features/rigs/guitar`
  (`ui/frame_surface`, `ui/tablet/routing_canvas`).

What is the frame repo's: the engine, its tools (`frame-cli`,
`frame-desktop`, `frame-mcp`, `frame-library-server`), the Figma parity
suite (`just parity` there), the face gallery (`just faces-gallery`) and the
design notes that were `docs/frame/` here (`docs/` there).

## Which crate, which features

Studio uses only `frame`, which has no architect, vox, Dioxus or Blitz
dependency, so it adds no second copy of any of them. Its rendering
dependencies (anyrender 0.14, anyrender_vello 0.15, vello 0.11, wgpu 30,
peniko 0.6, kurbo 0.13) are the versions Studio pins; a Frame bump that
moves one of them must wait for Studio to be on it.

| Studio crate | Features |
| --- | --- |
| `plugin-faces` | `io`, `render`, `runtime` (tests: `anyrender`, `design`) |
| `session-ui` | `host-scene`, `live`, `theme` |
| `guitar` | `host-scene`, `live` |

`host-scene` is the feature Studio's copy called `anyrender-blitz`: Frame
paints into the host's (Blitz's) anyrender scene. Module paths are the ones
Studio's copy had (`frame::document::proto`, `frame::surface::{kit, proto,
widgets}`, `frame::theme::runtime`, `frame::live`, …).

`frame-architect` (the `SurfaceEngine` vox service, the mock engine, remote
engines, surface libraries) is not a dependency: nothing in Studio uses it,
and it is on its own architect pin. Adding it means Frame first moves to
Studio's architect tag.

## Moving the pin

Change the `rev` of `frame` in the root `Cargo.toml`, then
`cargo update -p frame` and `just faces`. A new Frame may draw a face
differently: look at the faces before merging the bump. Compare `just
faces` output before and after (it is deterministic, byte for byte), then
render both sets with the frame repo's `render_surface` and compare
pixels, not files: the GPU effects stage is not byte-stable, and the same
binary flips a handful of pixels (a few dozen at most, up to ~100/255 on a
glow) from one run to the next. `just theme-parity` covers the themed
mixer.

## Working on Frame and Studio together

Put a local checkout under the build with a `[patch]` that is never
committed. Either pass it on the command line:

```sh
cargo check -p session-ui \
  --config 'patch."https://github.com/FastTrackStudios/frame".frame.path="../frame/crates/frame"'
```

or keep it in a file outside the repo and pass `--config /path/to/frame-local.toml`:

```toml
[patch."https://github.com/FastTrackStudios/frame"]
frame = { path = "/path/to/frame/crates/frame" }
```

Cargo writes the patched source into `Cargo.lock` while it builds: do not
commit that (`git checkout Cargo.lock`), and never put the `[patch]` in
`Cargo.toml` or `.cargo/config.toml`. `--locked` fails with a patch in
force, which is the point: CI builds the pinned `rev`. When the Frame change
is merged there, move the pin here.

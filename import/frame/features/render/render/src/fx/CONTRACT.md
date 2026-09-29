# Effect shader contract

A node's `shader={{"shader":"<id>","params":{…}}}` runs a WGSL effect over the
node's subtree on the GPU effects stage (`render::fx`, `Mode::Gpu`). `<id>` is
a built-in (`glass`, `crt`, `lighting`, `flicker`) or `asset:<asset id>`: the
UTF-8 WGSL stored in the document's asset table. Custom and built-in effects
use exactly the same contract.

## What the stage does

1. The node's subtree (its own paint and its descendants', with the enclosing
   groups, clips, layers and masks reapplied) renders alone into a
   transparent, frame-sized texture — the **source**.
2. A fullscreen pass runs over the frame. Inside the node's box (after its
   world transform — rotation/scale included) it calls your `effect`; outside
   the box the source passes through unchanged (so overflowing children still
   draw plainly).
3. The result is composited *over* what was painted before the node, in paint
   order; later siblings paint on top of it.

The effect sees only its own subtree, not what's behind the node. Effects
must be optional: renderers without the stage (SVG export, `--fx fallback`,
`--fx off`) draw the subtree plainly.

## What you write

The body of one function (plus any helpers of your own):

```wgsl
fn effect(uv: vec2<f32>) -> vec4<f32>
```

- `uv` — node-local, `0..1` across the node's box (`(0,0)` = its top-left).
- Return a **premultiplied** RGBA colour. The stage clamps `rgb` to `0..a`.
- Sample only with `textureSampleLevel` (or the helpers below): `effect` runs
  in non-uniform control flow.
- Don't declare bindings or entry points; the stage prepends the prelude
  below and appends `fx_vs_main`/`fx_fs_main`. Names starting with `fx_` are
  reserved.

## Prelude (in scope for your code)

```wgsl
struct FxUniforms {
    time_s: f32,          // seconds since the host started (animates `flicker`)
    value: f32,           // position 0..1 of the nearest control (this node or
                          // an ancestor); its default, else 0
    px_per_unit: f32,     // target px per node unit (render scale × transform)
    param_mask: u32,      // bit i set = param slot i given by the document
    resolution: vec2<f32>,  // target size, px
    node_size: vec2<f32>,   // the node's box, node units
    to_local_x: vec4<f32>,  // target px → node-local units (rows of a 2×3
    to_local_y: vec4<f32>,  //   affine: local = (dot(x.xyz, (px,1)), dot(y.xyz, (px,1))))
    to_px_x: vec4<f32>,     // node-local units → target px
    to_px_y: vec4<f32>,
    params: array<vec4<f32>, 4>,  // the 16 param slots, `params[i / 4][i % 4]`
}

@group(0) @binding(0) var<uniform> fx: FxUniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;  // the source, straight alpha
@group(0) @binding(2) var src_samp: sampler;          // linear, clamp-to-edge
```

Helpers:

| helper | returns |
|---|---|
| `sample_src(uv)` | source colour at node-local `uv`, premultiplied, bilinear |
| `sample_src_px(px)` | source colour at target pixel `px`, premultiplied |
| `uv_to_px(uv)`, `local_to_px(l)`, `px_to_local(px)` | coordinate mappings |
| `param(i)`, `param_set(i)`, `param_or(i, fallback)` | param slot `i` |
| `luminance(rgb)` | Rec. 709 luma |
| `hash11(x)` | cheap `0..1` hash |

## Params

`params` in the document are named; each name has a fixed slot (and a
`P_<NAME>` constant). Names not in this table are ignored.

| slot | name | constant |
|---:|---|---|
| 0 | `strength` | `P_STRENGTH` |
| 1 | `speed` | `P_SPEED` |
| 2 | `amount` | `P_AMOUNT` |
| 3 | `scale` | `P_SCALE` |
| 4 | `angle` | `P_ANGLE` |
| 5 | `elevation` | `P_ELEVATION` |
| 6 | `softness` | `P_SOFTNESS` |
| 7 | `radius` | `P_RADIUS` |
| 8–11 | `tint_r`, `tint_g`, `tint_b`, `tint_a` | `P_TINT_R` … `P_TINT_A` |
| 12–15 | `p0`, `p1`, `p2`, `p3` | `P_P0` … `P_P3` |

Use `param_or(P_STRENGTH, 0.6)` to give a param a default.

## Built-ins

| id | does | params (defaults) |
|---|---|---|
| `glass` | shallow lens refraction + diagonal specular sheen and top rim | `strength` (0.6) |
| `crt` | scanlines, slight barrel, phosphor bloom, vignette | `strength` (0.7), `scale` = scanline period in node units (3) |
| `lighting` | directional light over a height field from luminance (screen-space normals, so a turning cap keeps its light) | `strength` (0.8), `angle` (135° = from the top-left), `elevation` (45°), `scale` = bump depth (1) |
| `flicker` | tube-lamp brightness jitter from `time_s`; the node's (and its subtree's) `emission` flickers with it | `strength` = depth (0.5), `speed` Hz (10) |

## Failure

A shader that doesn't parse, validate or build a pipeline (or an `asset:` id
with no such asset) is reported once on stderr (`frame fx: shader … drawing
its subtree plainly`) and its subtree draws as if it had no shader. Compiled
pipelines are cached by id (asset ids are content hashes).

## Time

`time_s` is the host's clock. A scene with a `flicker`, or a custom shader
whose source mentions `time_s`, is *time-dependent*
(`render::fx::time_dependent`): hosts redraw it continuously.

## Example: invert

```wgsl
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = sample_src(uv);
    return vec4<f32>(vec3<f32>(c.a) - c.rgb, c.a);
}
```

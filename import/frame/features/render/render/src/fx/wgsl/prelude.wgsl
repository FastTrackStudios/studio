// ── frame effects contract (see CONTRACT.md) ─────────────────────────────
// Prepended to every effect (built-in or `asset:<id>`). An effect defines
//     fn effect(uv: vec2<f32>) -> vec4<f32>
// returning the premultiplied colour at node-local `uv` (0..1 across the
// node's box).

struct FxUniforms {
    time_s: f32,
    value: f32,
    px_per_unit: f32,
    param_mask: u32,
    resolution: vec2<f32>,
    node_size: vec2<f32>,
    to_local_x: vec4<f32>,
    to_local_y: vec4<f32>,
    to_px_x: vec4<f32>,
    to_px_y: vec4<f32>,
    params: array<vec4<f32>, 4>,
}

@group(0) @binding(0) var<uniform> fx: FxUniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;

const P_STRENGTH: u32 = 0u;
const P_SPEED: u32 = 1u;
const P_AMOUNT: u32 = 2u;
const P_SCALE: u32 = 3u;
const P_ANGLE: u32 = 4u;
const P_ELEVATION: u32 = 5u;
const P_SOFTNESS: u32 = 6u;
const P_RADIUS: u32 = 7u;
const P_TINT_R: u32 = 8u;
const P_TINT_G: u32 = 9u;
const P_TINT_B: u32 = 10u;
const P_TINT_A: u32 = 11u;
const P_P0: u32 = 12u;
const P_P1: u32 = 13u;
const P_P2: u32 = 14u;
const P_P3: u32 = 15u;

fn param(i: u32) -> f32 {
    return fx.params[i / 4u][i % 4u];
}

fn param_set(i: u32) -> bool {
    return ((fx.param_mask >> i) & 1u) != 0u;
}

fn param_or(i: u32, fallback: f32) -> f32 {
    if (param_set(i)) {
        return param(i);
    }
    return fallback;
}

fn px_to_local(px: vec2<f32>) -> vec2<f32> {
    let p = vec3<f32>(px, 1.0);
    return vec2<f32>(dot(fx.to_local_x.xyz, p), dot(fx.to_local_y.xyz, p));
}

fn local_to_px(l: vec2<f32>) -> vec2<f32> {
    let p = vec3<f32>(l, 1.0);
    return vec2<f32>(dot(fx.to_px_x.xyz, p), dot(fx.to_px_y.xyz, p));
}

fn uv_to_px(uv: vec2<f32>) -> vec2<f32> {
    return local_to_px(uv * fx.node_size);
}

// The subtree's colour at target pixel `px` (premultiplied, bilinear).
fn sample_src_px(px: vec2<f32>) -> vec4<f32> {
    let c = textureSampleLevel(src_tex, src_samp, px / fx.resolution, 0.0);
    return vec4<f32>(c.rgb * c.a, c.a);
}

// The subtree's colour at node-local `uv` (premultiplied).
fn sample_src(uv: vec2<f32>) -> vec4<f32> {
    return sample_src_px(uv_to_px(uv));
}

fn luminance(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn hash11(x: f32) -> f32 {
    return fract(sin(x * 12.9898) * 43758.5453);
}

// ── effect ───────────────────────────────────────────────────────────────

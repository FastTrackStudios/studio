//! The effects stage: optional light (`emission`) and shader (`shader`)
//! effects on top of the vector renderer.
//!
//! - [`Mode::Gpu`] — the scene renders as usual; emissive nodes render again in
//!   their emission colour at half resolution, are Gaussian-blurred (with
//!   downsampling for large radii) and added over the frame (bloom). Shader
//!   nodes render their subtree to its own texture, run a WGSL effect over it
//!   (built-ins in `wgsl/`, or `asset:<id>` custom WGSL — see `CONTRACT.md`)
//!   and composite back in paint order.
//! - [`Mode::Fallback`] — no GPU passes; emission draws as a soft glow in the
//!   vector path ([`EffectKind::Glow`](crate::anyrender_renderer::EffectKind)),
//!   shaders are skipped. The SVG exporter always does this.
//! - [`Mode::Off`] — neither.

mod passes;
mod stage;

use std::collections::BTreeMap;

pub use crate::anyrender_renderer::{flicker_level, EffectsPlan, EmissiveDraw, ShaderRegion};
pub use stage::{Frame, Stage};

/// How a renderer treats `emission`/`shader`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// GPU bloom + WGSL shaders.
    #[default]
    Gpu,
    /// Soft vector glows for emission; shaders skipped.
    Fallback,
    /// Neither.
    Off,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gpu" => Ok(Self::Gpu),
            "fallback" => Ok(Self::Fallback),
            "off" => Ok(Self::Off),
            other => Err(format!("unknown fx mode `{other}` (gpu|fallback|off)")),
        }
    }
}

/// The 16 named shader params, in uniform-slot order (`params[i / 4][i % 4]`
/// in WGSL, with `P_<NAME>` constants). Names not listed are ignored.
pub const PARAM_SLOTS: [&str; 16] = [
    "strength",
    "speed",
    "amount",
    "scale",
    "angle",
    "elevation",
    "softness",
    "radius",
    "tint_r",
    "tint_g",
    "tint_b",
    "tint_a",
    "p0",
    "p1",
    "p2",
    "p3",
];

/// The built-in effects, by id.
pub const BUILTINS: [&str; 4] = ["glass", "crt", "lighting", "flicker"];

/// The slot of a param name, if it has one.
#[must_use]
pub fn param_slot(name: &str) -> Option<usize> {
    PARAM_SLOTS.iter().position(|n| *n == name)
}

/// Pack a node's params into the uniform layout: 16 floats and a bit mask of
/// the slots the document set.
#[must_use]
pub fn pack_params(params: &BTreeMap<String, f64>) -> ([f32; 16], u32) {
    let mut values = [0.0_f32; 16];
    let mut mask = 0_u32;
    for (name, v) in params {
        if let Some(i) = param_slot(name) {
            values[i] = *v as f32;
            mask |= 1 << i;
        }
    }
    (values, mask)
}

/// The WGSL body of a built-in effect.
#[must_use]
pub fn builtin_source(id: &str) -> Option<&'static str> {
    Some(match id {
        "glass" => include_str!("wgsl/glass.wgsl"),
        "crt" => include_str!("wgsl/crt.wgsl"),
        "lighting" => include_str!("wgsl/lighting.wgsl"),
        "flicker" => include_str!("wgsl/flicker.wgsl"),
        _ => return None,
    })
}

/// The effect body for `id` in `doc`: a built-in, or the UTF-8 WGSL of the
/// asset an `asset:<id>` names.
#[must_use]
pub fn effect_source(doc: &document_proto::Scene, id: &str) -> Option<String> {
    if let Some(asset) = id.strip_prefix("asset:") {
        let a = doc.asset(&document_proto::AssetId(asset.to_string()))?;
        return String::from_utf8(a.bytes.clone()).ok();
    }
    builtin_source(id).map(str::to_string)
}

/// A complete effect module: the contract prelude, the effect, the entry
/// points.
#[must_use]
pub fn effect_module(body: &str) -> String {
    format!("{}\n{body}\n{}", include_str!("wgsl/prelude.wgsl"), include_str!("wgsl/entry.wgsl"))
}

/// Whether `doc` under `root` needs continuous redraws with effects on (a
/// time-dependent shader or flickering light).
#[must_use]
pub fn time_dependent(doc: &document_proto::Scene, root: document_proto::NodeId) -> bool {
    let (_, plan) = crate::build_scene_primitives_and_effects(doc, root);
    plan.time_dependent(|id| effect_source(doc, id))
}

#[cfg(test)]
mod tests;

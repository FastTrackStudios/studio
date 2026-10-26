//! **Multi** (`.mlt_omn`) parsing and mapping — 8 Parts + the master mixer.

use std::path::Path;

use super::model::{OmniPatch, parse_patch_node, rack_params, rack_types};
use super::tree::{fx_rack_from, patch_to_container};
use super::{SoundsourceIndex, parse_xml};
use sampler::rig_node::Container;

// ── Multis ───────────────────────────────────────────────────────────────────

/// A parsed `.mlt_omn` Multi: up to 8 Parts + their mixer strip.
#[derive(Debug, Clone, Default)]
pub struct OmniMulti {
    pub name: String,
    /// `(patch, level 0..1, muted, pan 0..1)` per part.
    pub parts: Vec<(OmniPatch, f32, bool, f32)>,
    /// Each part's sends to the four multi aux buses (`p{i}AuxSnd{k}`).
    pub sends: Vec<[f32; 4]>,
    /// The four aux racks (`AEffRack0..3`) and the master rack (`MEffRack`):
    /// slot types and params.
    pub aux_fx: Vec<(Vec<String>, Vec<[f32; 15]>)>,
    pub master_fx: (Vec<String>, Vec<[f32; 15]>),
}

/// Parse a `.mlt_omn` document: `SynthMaster` wraps 8 `SynthEngine` parts
/// plus `MasterEngineBaseParamBlock` mixer attrs (`pLevel0..7`, `pMute0..7`).
///
/// # Errors
///
/// Returns an error if the XML is invalid, does not contain the expected multi structure, or has no synth engines.
pub fn parse_multi(xml: &str) -> Result<OmniMulti, String> {
    let root = parse_xml(xml)?;
    let mut multi = OmniMulti::default();
    if let Some(descr) = root.child("ENTRYDESCR") {
        multi.name = descr.attr("name").unwrap_or("").to_string();
    }
    let mixer = root.find("MasterEngineBaseParamBlock");
    for (i, engine) in root.children_tagged("SynthSubEngine").enumerate() {
        let patch = parse_patch_node(engine)?;
        let level = mixer
            .and_then(|m| m.num(&format!("pLevel{i}")))
            .unwrap_or(0.75)
            .clamp(0.0, 1.0);
        let muted = mixer
            .and_then(|m| m.num(&format!("pMute{i}")))
            .unwrap_or(0.0)
            != 0.0;
        let pan = mixer
            .and_then(|m| m.num(&format!("pPan{i}")))
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        multi.parts.push((patch, level, muted, pan));
        multi.sends.push(std::array::from_fn(|k| {
            mixer
                .and_then(|m| m.num(&format!("p{i}AuxSnd{k}")))
                .unwrap_or(0.0)
                .clamp(0.0, 1.0)
        }));
    }
    let rack = |tag: &str| {
        root.find(tag)
            .map(|r| (rack_types(r), rack_params(r)))
            .unwrap_or_default()
    };
    multi.aux_fx = (0..4).map(|k| rack(&format!("AEffRack{k}"))).collect();
    multi.master_fx = rack("MEffRack");
    if multi.parts.is_empty() {
        return Err("no SynthEngine parts (not an Omnisphere multi?)".into());
    }
    Ok(multi)
}

/// Map a Multi onto one composition tree: Parts sum in parallel, each with
/// its mixer fader (measured: the layer level's taper, unity at 0.75 —
/// 1.0 +9.6 dB, 0.5 −7.1, 0.1 −35.1), pan and mute (bypass).
#[must_use]
pub fn multi_to_container(multi: &OmniMulti, index: &SoundsourceIndex) -> Container {
    let title = if multi.name.is_empty() {
        "Omnisphere Multi".to_string()
    } else {
        multi.name.clone()
    };
    let mut parts = Container::parallel("Parts");
    for (i, (patch, level, muted, pan)) in multi.parts.iter().enumerate() {
        // Skip empty default parts (no soundsource, no layers of note).
        let named = !patch.name.is_empty();
        let has_content = named || patch.layers.iter().any(|l| !l.soundsource.is_empty());
        if !has_content {
            continue;
        }
        let mut part = patch_to_container(patch, index);
        part.role = sampler::rig_node::Role::Engine;
        part.name = format!("Part {}: {}", i + 1, patch.name);
        part.output_db =
            (super::tree::layer_level_db(*level) - super::tree::layer_level_db(0.75)).max(-120.0);
        if (pan - 0.5).abs() > 1e-3 {
            part = part.add(
                sampler::RigBlock::of_type(signal_proto::block::BlockType::Volume)
                    .named("Part Pan")
                    .with_param("gain_db", "0")
                    .with_param("pan", format!("{:.3}", 2.0 * pan - 1.0)),
            );
        }
        part.bypassed = *muted;
        // Sends to the multi's aux buses: each sending part feeds its own
        // copy of the bus's rack (the units are linear, so the copies sum
        // as one bus would), at the part aux law (`send` −3 dB).
        let sends = multi.sends.get(i).copied().unwrap_or_default();
        if sends.iter().any(|&s| s > 0.0) {
            let mut split = Container::parallel("Aux Sends").add(Container::module("Main"));
            for (k, &send) in sends.iter().enumerate() {
                let Some((types, params)) = multi.aux_fx.get(k) else {
                    continue;
                };
                if send <= 0.0 || types.iter().all(|t| t.is_empty() || t == "No Effect") {
                    continue;
                }
                let bus = ["A", "B", "C", "D"][k];
                split = split.add(
                    Container::module(format!("Aux {bus} Send"))
                        .volume(20.0 * send.log10() - 3.0)
                        .add(fx_rack_from(&format!("Aux {bus}"), types, params)),
                );
            }
            part = part.add(split);
        }
        parts = parts.add(part);
    }
    let (types, params) = &multi.master_fx;
    Container::preset(title)
        .add(parts)
        .add(fx_rack_from("Master FX", types, params))
}

/// Convenience: read + parse + map a `.mlt_omn` file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the XML is invalid.
pub fn load_multi_file(path: &Path, index: &SoundsourceIndex) -> Result<Container, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let multi = parse_multi(&xml)?;
    Ok(multi_to_container(&multi, index))
}

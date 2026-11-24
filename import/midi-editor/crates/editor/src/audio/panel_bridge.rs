//! From the quantize panel's sliders to the engine, in one pass.
//!
//! The panel (`crate::ui::quantize_panel`) is deliberately
//! engine-blind: it holds plain config values and emits them, and the UI
//! crate does not depend on this one. This module is the other half of
//! that seam — panel-shaped config structs in, [`DetectConfig`] /
//! [`QuantizeConfig`] / [`SplitConfig`] and preview data out — so a host
//! (standalone or REAPER) wires the two with a couple of calls and no
//! translation code of its own.
//!
//! Detection follows the group rule: each trigger lane detects on its
//! members' **summed** signal and the lists are merged, union with
//! nearest-duplicate-collapses-to-the-louder — never per mic.
//! Spec: `drum-mode.md` r[drums.group.detection-source].

use crate::ops::quantize as tools;
pub use crate::ops::quantize::{HitPreview, hit_previews};

use crate::audio::detect::{DetectConfig, Filters, Transient};
use crate::audio::group_detect::{detect_summed, merge_hits};
use crate::audio::quantize::{Plan, QuantizeConfig, SplitConfig, plan};

/// The Detect section's values, as the panel holds them.
///
/// Field-for-field the shape of `quantize_panel::DetectSettings`; a
/// mirrored struct rather than a dependency because the arrow between
/// the crates points the other way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelDetect {
    pub threshold_db: f64,
    pub sensitivity: f64,
    pub crest_db: f64,
    pub high_pass_hz: Option<f64>,
    pub low_pass_hz: Option<f64>,
    pub gain: f64,
    pub retrigger_secs: f64,
    pub time_offset_secs: f64,
}

impl Default for PanelDetect {
    fn default() -> Self {
        Self {
            threshold_db: -60.0,
            sensitivity: 0.5,
            crest_db: 3.0,
            high_pass_hz: None,
            low_pass_hz: None,
            gain: 1.0,
            retrigger_secs: 0.050,
            time_offset_secs: 0.0,
        }
    }
}

/// The Target section's values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelTarget {
    /// Seconds between grid divisions — the host computes this from its
    /// tempo map and the panel's division/feel choice.
    pub grid_secs: f64,
    /// Where the grid starts, seconds from the take's start.
    pub grid_offset_secs: f64,
    /// Swing on the off-beat divisions, `0.0..=1.0`. Target placement
    /// only — the planner never sees it.
    pub swing: f64,
    /// Whether each division scans a window for its loudest hit.
    pub grid_scan: bool,
    /// The window half-width while `grid_scan` is on, seconds.
    pub tolerance_secs: f64,
    /// `0.0` leaves the take alone, `1.0` puts every hit on its
    /// division.
    pub strength: f64,
}

impl Default for PanelTarget {
    fn default() -> Self {
        Self {
            grid_secs: 0.125,
            grid_offset_secs: 0.0,
            swing: 0.0,
            grid_scan: true,
            tolerance_secs: 0.05,
            strength: 1.0,
        }
    }
}

/// The Write section's values. SPLIT is the kit default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelWrite {
    /// `true` cuts and moves pieces; `false` warps.
    pub split: bool,
    pub pad_secs: f64,
    pub crossfade_secs: f64,
}

impl Default for PanelWrite {
    fn default() -> Self {
        Self {
            split: true,
            pad_secs: 0.007,
            crossfade_secs: 0.007,
        }
    }
}

/// Panel detect values → the detector's own config.
// r[impl drums.quantize.panel]
pub fn detect_config(d: &PanelDetect) -> DetectConfig {
    let mut cfg = DetectConfig::default();
    cfg.gate.threshold_db = d.threshold_db;
    cfg.gate.crest_db = d.crest_db;
    cfg.gate.retrigger_secs = d.retrigger_secs;
    cfg.filters = Filters {
        high_pass_hz: d.high_pass_hz,
        low_pass_hz: d.low_pass_hz,
        gain: d.gain,
    };
    cfg.sensitivity = d.sensitivity;
    cfg.time_offset_secs = d.time_offset_secs;
    cfg
}

/// Panel target values → the planner's config.
// r[impl drums.quantize.panel]
pub fn quantize_config(t: &PanelTarget) -> QuantizeConfig {
    QuantizeConfig {
        grid_secs: t.grid_secs,
        grid_offset_secs: t.grid_offset_secs,
        tolerance_secs: t.grid_scan.then_some(t.tolerance_secs),
        strength: t.strength,
    }
}

/// Panel write values → SPLIT's config.
// r[impl drums.quantize.panel]
pub fn split_config(w: &PanelWrite) -> SplitConfig {
    SplitConfig {
        leading_pad_secs: w.pad_secs,
        crossfade_secs: w.crossfade_secs,
    }
}

/// Detect over the trigger lanes and merge, per the group rule.
///
/// `trigger_lanes` is one entry per armed lane (kick, snare, an armed
/// tom), each a list of that lane's member sample slices. Every lane
/// detects on its members' summed signal, and the lists merge with the
/// retrigger window as the duplicate window.
// r[impl drums.group.detection-source]
pub fn detect_group(
    trigger_lanes: &[Vec<&[f64]>],
    sample_rate: f64,
    detect: &PanelDetect,
) -> Vec<Transient> {
    let cfg = detect_config(detect);
    let per_lane: Vec<Vec<Transient>> = trigger_lanes
        .iter()
        .map(|members| detect_summed(members, sample_rate, cfg))
        .collect();
    let lists: Vec<&[Transient]> = per_lane.iter().map(|l| l.as_slice()).collect();
    merge_hits(&lists, detect.retrigger_secs)
}

/// The whole panel loop in one call: detect, merge, plan, preview.
///
/// Returns the merged hits (for the histogram), the plan (for Apply to
/// render as splits or a warp map), and the per-hit from→to list the
/// lanes and the panel's strip draw.
// r[impl drums.quantize.preview]
pub fn preview(
    trigger_lanes: &[Vec<&[f64]>],
    sample_rate: f64,
    detect: &PanelDetect,
    target: &PanelTarget,
) -> (Vec<Transient>, Plan, Vec<HitPreview>) {
    let hits = detect_group(trigger_lanes, sample_rate, detect);
    let (p, previews) = preview_hits(&hits, target);
    (hits, p, previews)
}

/// Plan + preview over a hit list the caller already holds — the same
/// pass as [`preview`], split out so a host that overlays hand-added or
/// hand-removed hits (r[drums.manual.add-remove]) plans over the list
/// the user actually sees.
pub fn preview_hits(hits: &[Transient], target: &PanelTarget) -> (Plan, Vec<HitPreview>) {
    let mut p = plan(hits, quantize_config(target));
    // The same swing pass the panel's own preview runs — one body, in
    // the tools crate, so the two can never disagree.
    tools::swing(&mut p.0, quantize_config(target).into(), target.swing);
    let previews = hit_previews(&p);
    (p, previews)
}

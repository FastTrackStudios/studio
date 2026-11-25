//! Per-hit editing and hybrid slip-vs-stretch planning.
//!
//! Users (dragging a hit marker in the UI) and agents (`RHYTHM_PLACE_HITS`)
//! express edits as target times on individual hits. From those target times
//! and the original recording, [`plan_hybrid`] deterministically produces a
//! phase-locked edit plan that keeps drum attacks 100% natural wherever
//! possible:
//!
//! 1. **Pure slip + crossfade (`SegmentKind::Slip`)** whenever the natural
//!    decay of the slice before and lead-in of the slice after overlap without
//!    touching any attack transient.
//! 2. **Transient-protected tail stretch (`SegmentKind::TailStretch`)** when a
//!    hit moved far enough later that natural audio leaves a gap: the attack
//!    plus `transient_protect_s` plays completely unstretched, and ONLY the
//!    decay tail after the transient is stretched to bridge the gap.
//! 3. **Full warp (`SegmentKind::Warp`)** when a hit (or the whole edit)
//!    explicitly requests stretch mode, or when stretching only the decay tail
//!    would exceed `max_tail_stretch_ratio`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::placement::{Cut, Hit, Piece, place};

/// Global editing strategy chosen by the user, agent, or material profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditStrategy {
    /// Prefer untouched natural slip + crossfade; stretch only post-transient
    /// tails when natural audio cannot close a gap.
    #[default]
    Hybrid,
    /// Never stretch audio; close seams strictly with slip + crossfade (leaving
    /// any unavoidable gap flagged in the audit).
    SlipOnly,
    /// Place stretch markers at every moved hit and warp audio continuously.
    StretchOnly,
}

/// Optional per-hit override when fine-tuning an individual hit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitEditMode {
    #[default]
    Auto,
    SlipOnly,
    Stretch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordedHit {
    pub id: String,
    pub piece: String,
    /// Attack start time in the original recording, seconds.
    pub at: f64,
    #[serde(default)]
    pub peak_db: f64,
    #[serde(default)]
    pub ghost: bool,
}

/// An explicit move of one hit (from UI drag, quantizer, or agent call).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitMove {
    pub id: String,
    /// Desired playback time on the timeline, seconds.
    pub target_time: f64,
    #[serde(default)]
    pub mode: HitEditMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HybridPolicy {
    #[serde(default)]
    pub strategy: EditStrategy,
    /// Pre-transient cut lead (seconds). Protects stick approach / soft kick onset.
    pub lead_s: f64,
    /// Post-transient guard (seconds). Next slice never reaches back over a hit.
    pub guard_s: f64,
    /// Crossfade length across slip seams (seconds).
    pub crossfade_s: f64,
    /// Minimum post-attack transient kept 100% unstretched before any tail stretch.
    pub transient_protect_s: f64,
    /// Gaps smaller than this are absorbed by crossfade overlap without stretching.
    pub max_natural_gap_s: f64,
    /// Maximum comfortable tail-stretch factor (output / input duration) before
    /// warning or falling back to a wider stretch region.
    pub max_tail_stretch_ratio: f64,
}

impl Default for HybridPolicy {
    fn default() -> Self {
        Self {
            strategy: EditStrategy::Hybrid,
            lead_s: 0.005,
            guard_s: 0.015,
            crossfade_s: 0.005,
            transient_protect_s: 0.030,
            max_natural_gap_s: 0.004,
            max_tail_stretch_ratio: 1.75,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentKind {
    /// Natural unstretched recording played at 1.0x speed.
    Slip,
    /// Only the post-transient decay tail of a slip slice is time-stretched to
    /// close a gap; the attack transient before `source_start_s` stays untouched.
    TailStretch {
        /// Output duration / source duration (> 1.0 = slower / longer tail).
        stretch_ratio: f64,
    },
    /// Continuous stretch-marker warp across the segment.
    Warp {
        /// Output duration / source duration.
        stretch_ratio: f64,
    },
}

/// One phase-locked timeline audio segment applied identically across every mic
/// of the edited phase group (e.g. the whole drum kit).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioSegment {
    pub timeline_start: f64,
    pub timeline_end: f64,
    pub source_start: f64,
    pub source_end: f64,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    pub kind: SegmentKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanAudit {
    pub total_segments: usize,
    pub slip_segments: usize,
    pub tail_stretch_segments: usize,
    pub warp_segments: usize,
    pub unclosed_gaps: usize,
    pub max_stretch_ratio: f64,
    pub attacks_protected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HybridPlan {
    pub strategy: EditStrategy,
    pub segments: Vec<AudioSegment>,
    pub audit: PlanAudit,
}

/// Deterministically plan phase-locked audio segments from the original
/// recording, `hits`, and individual `moves`.
///
/// Moving a hit and moving it back to `hit.at` yields the exact same uncut
/// single `Slip` segment as never having edited it.
#[must_use]
pub fn plan_hybrid(
    hits: &[RecordedHit],
    moves: &[HitMove],
    span: (f64, f64),
    policy: HybridPolicy,
) -> HybridPlan {
    let mut sorted_hits: Vec<&RecordedHit> = hits.iter().collect();
    sorted_hits.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.id.cmp(&b.id)));

    let move_map: HashMap<&str, &HitMove> = moves.iter().map(|m| (m.id.as_str(), m)).collect();

    if policy.strategy == EditStrategy::StretchOnly {
        return plan_stretch_only(&sorted_hits, &move_map, span);
    }

    let placement_hits: Vec<Hit> = sorted_hits
        .iter()
        .map(|h| {
            let shift = move_map
                .get(h.id.as_str())
                .map_or(0.0, |m| m.target_time - h.at);
            Hit { at: h.at, shift }
        })
        .collect();

    let cut = Cut {
        lead: policy.lead_s,
        guard: policy.guard_s,
    };
    let pieces = place(&placement_hits, span, cut);

    let mut segments = Vec::with_capacity(pieces.len() * 2);
    let mut unclosed_gaps = 0usize;
    let mut max_stretch_ratio = 1.0_f64;

    for (idx, p) in pieces.iter().enumerate() {
        // A piece that crossfades over the one before (the gap fill's
        // overlap) starts here where that one ends: the plan's segments
        // butt, each with its own fades.
        let p = &Piece {
            start: p.start + p.crossfade.unwrap_or(0.0),
            ..*p
        };
        let src_start = p.start - p.shift;
        let src_end = p.end - p.shift;
        let has_prev = idx > 0;
        let has_next = idx + 1 < pieces.len();

        let contains_mode = |mode| sorted_hits.iter().any(|hit| {
            hit.at >= src_start - 1e-6 && hit.at <= src_end + 1e-6
                && move_map.get(hit.id.as_str()).is_some_and(|m| m.mode == mode)
        });
        let piece_forced_stretch = contains_mode(HitEditMode::Stretch);
        let piece_strategy = if contains_mode(HitEditMode::SlipOnly) {
            EditStrategy::SlipOnly
        } else {
            policy.strategy
        };

        let gap_end = p.drawn_to.filter(|&to| to > p.end + policy.max_natural_gap_s);

        match (gap_end, piece_strategy) {
            (Some(target_end), EditStrategy::Hybrid) => {
                // Find the last recorded hit inside this piece so we keep its
                // attack + `transient_protect_s` 100% unstretched.
                let last_hit_in_piece = sorted_hits
                    .iter()
                    .filter(|h| h.at >= src_start - 1e-6 && h.at <= src_end + 1e-6)
                    .map(|h| h.at + p.shift)
                    .next_back();

                let protect_until = last_hit_in_piece
                    .map(|t| (t + policy.transient_protect_s).min(p.end))
                    .unwrap_or(p.start);

                // Available natural tail after transient protection to stretch over
                // the gap.
                let natural_tail = (p.end - protect_until).max(0.0);
                let needed_tail = (target_end - protect_until).max(0.0);

                if natural_tail > 0.008
                    && !piece_forced_stretch
                    && (needed_tail / natural_tail) <= policy.max_tail_stretch_ratio
                {
                    let ratio = needed_tail / natural_tail;
                    max_stretch_ratio = max_stretch_ratio.max(ratio);

                    // 1. Crisp, untouched attack + transient body.
                    if protect_until > p.start + 1e-6 {
                        segments.push(AudioSegment {
                            timeline_start: p.start,
                            timeline_end: protect_until,
                            source_start: src_start,
                            source_end: protect_until - p.shift,
                            fade_in_s: if has_prev { policy.crossfade_s } else { 0.0 },
                            fade_out_s: policy.crossfade_s * 0.5,
                            kind: SegmentKind::Slip,
                        });
                    }
                    // 2. Stretched post-transient decay tail bridging into the next slice.
                    segments.push(AudioSegment {
                        timeline_start: protect_until,
                        timeline_end: target_end,
                        source_start: protect_until - p.shift,
                        source_end: src_end,
                        fade_in_s: policy.crossfade_s * 0.5,
                        fade_out_s: if has_next { policy.crossfade_s } else { 0.0 },
                        kind: SegmentKind::TailStretch {
                            stretch_ratio: ratio,
                        },
                    });
                } else if natural_tail > 0.004 && !piece_forced_stretch {
                    // Still protect the transient even when the gap is large —
                    // clamp stretch region to the tail and record the higher ratio
                    // in the audit so the user/agent can inspect it.
                    let ratio = needed_tail / natural_tail;
                    max_stretch_ratio = max_stretch_ratio.max(ratio);
                    if protect_until > p.start + 1e-6 {
                        segments.push(AudioSegment {
                            timeline_start: p.start,
                            timeline_end: protect_until,
                            source_start: src_start,
                            source_end: protect_until - p.shift,
                            fade_in_s: if has_prev { policy.crossfade_s } else { 0.0 },
                            fade_out_s: policy.crossfade_s * 0.5,
                            kind: SegmentKind::Slip,
                        });
                    }
                    segments.push(AudioSegment {
                        timeline_start: protect_until,
                        timeline_end: target_end,
                        source_start: protect_until - p.shift,
                        source_end: src_end,
                        fade_in_s: policy.crossfade_s * 0.5,
                        fade_out_s: if has_next { policy.crossfade_s } else { 0.0 },
                        kind: SegmentKind::TailStretch {
                            stretch_ratio: ratio,
                        },
                    });
                } else {
                    // Explicit Stretch mode or zero natural tail: warp the whole piece.
                    let src_len = (src_end - src_start).max(1e-6);
                    let out_len = (target_end - p.start).max(1e-6);
                    let ratio = out_len / src_len;
                    max_stretch_ratio = max_stretch_ratio.max(ratio);
                    segments.push(AudioSegment {
                        timeline_start: p.start,
                        timeline_end: target_end,
                        source_start: src_start,
                        source_end: src_end,
                        fade_in_s: if has_prev { policy.crossfade_s } else { 0.0 },
                        fade_out_s: if has_next { policy.crossfade_s } else { 0.0 },
                        kind: SegmentKind::Warp {
                            stretch_ratio: ratio,
                        },
                    });
                }
            }
            (Some(_), EditStrategy::SlipOnly) => {
                unclosed_gaps += 1;
                segments.push(AudioSegment {
                    timeline_start: p.start,
                    timeline_end: p.end,
                    source_start: src_start,
                    source_end: src_end,
                    fade_in_s: if has_prev { policy.crossfade_s } else { 0.0 },
                    fade_out_s: if has_next { policy.crossfade_s } else { 0.0 },
                    kind: SegmentKind::Slip,
                });
            }
            (None, _) => {
                if piece_forced_stretch && (p.end - p.start) > 1e-6 && p.shift.abs() > 1e-6 {
                    segments.push(AudioSegment {
                        timeline_start: p.start,
                        timeline_end: p.end,
                        source_start: src_start,
                        source_end: src_end,
                        fade_in_s: if has_prev { policy.crossfade_s } else { 0.0 },
                        fade_out_s: if has_next { policy.crossfade_s } else { 0.0 },
                        kind: SegmentKind::Warp { stretch_ratio: 1.0 },
                    });
                } else {
                    // Natural audio covers the seam completely — pure slip + crossfade.
                    segments.push(AudioSegment {
                        timeline_start: p.start,
                        timeline_end: p.drawn_to.unwrap_or(p.end),
                        source_start: src_start,
                        source_end: p.drawn_to.unwrap_or(p.end) - p.shift,
                        fade_in_s: if has_prev { policy.crossfade_s } else { 0.0 },
                        fade_out_s: if has_next { policy.crossfade_s } else { 0.0 },
                        kind: SegmentKind::Slip,
                    });
                }
            }
            (Some(_), EditStrategy::StretchOnly) => unreachable!(),
        }
    }

    let audit = build_audit(&segments, unclosed_gaps, max_stretch_ratio);
    HybridPlan {
        strategy: policy.strategy,
        segments,
        audit,
    }
}

fn plan_stretch_only(
    sorted_hits: &[&RecordedHit],
    move_map: &HashMap<&str, &HitMove>,
    (start, end): (f64, f64),
) -> HybridPlan {
    // Anchors: (source_time, timeline_time)
    let mut anchors: Vec<(f64, f64)> = Vec::with_capacity(sorted_hits.len() + 2);
    anchors.push((start, start));
    for h in sorted_hits {
        if h.at <= start + 1e-6 || h.at >= end - 1e-6 {
            continue;
        }
        let target = move_map
            .get(h.id.as_str())
            .map_or(h.at, |m| m.target_time)
            .clamp(start + 1e-5, end - 1e-5);
        anchors.push((h.at, target));
    }
    anchors.push((end, end));

    let mut segments = Vec::with_capacity(anchors.len().saturating_sub(1));
    let mut max_ratio = 1.0_f64;
    for pair in anchors.windows(2) {
        let [(s0, t0), (s1, t1)] = *pair else {
            continue;
        };
        let src_len = (s1 - s0).max(1e-6);
        let out_len = (t1 - t0).max(1e-6);
        let ratio = out_len / src_len;
        max_ratio = max_ratio.max(ratio).max(1.0 / ratio);
        let kind = if (ratio - 1.0).abs() < 1e-5 {
            SegmentKind::Slip
        } else {
            SegmentKind::Warp {
                stretch_ratio: ratio,
            }
        };
        segments.push(AudioSegment {
            timeline_start: t0,
            timeline_end: t1,
            source_start: s0,
            source_end: s1,
            fade_in_s: 0.0,
            fade_out_s: 0.0,
            kind,
        });
    }

    let audit = build_audit(&segments, 0, max_ratio);
    HybridPlan {
        strategy: EditStrategy::StretchOnly,
        segments,
        audit,
    }
}

fn build_audit(
    segments: &[AudioSegment],
    unclosed_gaps: usize,
    max_stretch_ratio: f64,
) -> PlanAudit {
    let slip_segments = segments
        .iter()
        .filter(|s| matches!(s.kind, SegmentKind::Slip))
        .count();
    let tail_stretch_segments = segments
        .iter()
        .filter(|s| matches!(s.kind, SegmentKind::TailStretch { .. }))
        .count();
    let warp_segments = segments
        .iter()
        .filter(|s| matches!(s.kind, SegmentKind::Warp { .. }))
        .count();
    PlanAudit {
        total_segments: segments.len(),
        slip_segments,
        tail_stretch_segments,
        warp_segments,
        unclosed_gaps,
        max_stretch_ratio,
        attacks_protected: warp_segments == 0,
    }
}

/// Quantize `hits` onto a constant-tempo grid while preserving any manual
/// per-hit `pinned_moves` made by a user or agent.
#[must_use]
pub fn quantize_to_grid(
    hits: &[RecordedHit],
    bpm: f64,
    subdivision_per_beat: u32,
    strength: f64,
    tolerance_s: f64,
    pinned_moves: &[HitMove],
) -> Vec<HitMove> {
    let step = 60.0 / bpm.max(1.0) / f64::from(subdivision_per_beat.max(1));
    let strength = strength.clamp(0.0, 1.0);
    let pinned: HashMap<&str, &HitMove> = pinned_moves.iter().map(|m| (m.id.as_str(), m)).collect();

    hits.iter()
        .filter_map(|h| {
            if let Some(pin) = pinned.get(h.id.as_str()) {
                return Some((*pin).clone());
            }
            let nearest = (h.at / step).round() * step;
            let delta = nearest - h.at;
            if delta.abs() <= tolerance_s || delta.abs() < 1e-6 {
                return None;
            }
            Some(HitMove {
                id: h.id.clone(),
                target_time: h.at + delta * strength,
                mode: HitEditMode::Auto,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hits() -> Vec<RecordedHit> {
        vec![
            RecordedHit {
                id: "Kick#0".into(),
                piece: "Kick".into(),
                at: 0.500,
                peak_db: -6.0,
                ghost: false,
            },
            RecordedHit {
                id: "Snare#0".into(),
                piece: "Snare".into(),
                at: 1.020,
                peak_db: -4.0,
                ghost: false,
            },
            RecordedHit {
                id: "Kick#1".into(),
                piece: "Kick".into(),
                at: 1.500,
                peak_db: -6.0,
                ghost: false,
            },
        ]
    }

    #[test]
    fn moving_hit_back_restores_single_uncut_slip() {
        let hits = sample_hits();
        let moved_back = vec![HitMove {
            id: "Snare#0".into(),
            target_time: 1.020,
            mode: HitEditMode::Auto,
        }];
        let plan = plan_hybrid(&hits, &moved_back, (0.0, 2.0), HybridPolicy::default());
        assert_eq!(plan.segments.len(), 1);
        assert_eq!(plan.segments[0].kind, SegmentKind::Slip);
        assert_eq!(plan.audit.tail_stretch_segments, 0);
        assert!(plan.audit.attacks_protected);
    }

    #[test]
    fn small_individual_move_uses_pure_slip_and_crossfade() {
        let hits = sample_hits();
        // Pull snare 20ms earlier onto 1.000s: natural decay/lead-in covers both seams.
        let moves = vec![HitMove {
            id: "Snare#0".into(),
            target_time: 1.000,
            mode: HitEditMode::Auto,
        }];
        let plan = plan_hybrid(&hits, &moves, (0.0, 2.0), HybridPolicy::default());
        assert_eq!(plan.audit.slip_segments, plan.audit.total_segments);
        assert_eq!(plan.audit.tail_stretch_segments, 0);
        assert_eq!(plan.audit.unclosed_gaps, 0);
        assert!(plan.audit.attacks_protected);
    }

    #[test]
    fn large_gap_triggers_transient_protected_tail_stretch() {
        let hits = vec![
            RecordedHit {
                id: "Kick#0".into(),
                piece: "Kick".into(),
                at: 0.500,
                peak_db: -6.0,
                ghost: false,
            },
            RecordedHit {
                id: "Snare#0".into(),
                piece: "Snare".into(),
                at: 0.700,
                peak_db: -4.0,
                ghost: false,
            },
            RecordedHit {
                id: "Kick#1".into(),
                piece: "Kick".into(),
                at: 1.200,
                peak_db: -6.0,
                ghost: false,
            },
        ];
        // Move Snare#0 100ms later (from 0.700 -> 0.800): Kick#0 only has natural audio
        // up to 0.695s, and Snare#0 cannot reach back past Kick#0's guard (0.515 + 0.100 = 0.615)...
        // Wait: move Snare#0 250ms later (0.700 -> 0.950) so Snare#0's reach-back stops at 0.765s
        // while Kick#0 ends at 0.695s -> real 70ms gap!
        let moves = vec![HitMove {
            id: "Snare#0".into(),
            target_time: 0.950,
            mode: HitEditMode::Auto,
        }];
        let plan = plan_hybrid(&hits, &moves, (0.0, 1.5), HybridPolicy::default());
        assert!(
            plan.audit.tail_stretch_segments >= 1,
            "expected tail stretch to bridge 70ms gap, got {plan:#?}"
        );
        assert_eq!(plan.audit.warp_segments, 0);
        assert!(plan.audit.attacks_protected);
        assert_eq!(plan.audit.unclosed_gaps, 0);
    }

    #[test]
    fn quantize_preserves_manual_hit_pins() {
        let hits = sample_hits();
        let pins = vec![HitMove {
            id: "Snare#0".into(),
            target_time: 1.035, // human/agent intentionally laid back 35ms
            mode: HitEditMode::SlipOnly,
        }];
        let moves = quantize_to_grid(&hits, 120.0, 4, 1.0, 0.002, &pins);
        let snare = moves.iter().find(|m| m.id == "Snare#0").unwrap();
        assert!((snare.target_time - 1.035).abs() < 1e-9);
        assert_eq!(snare.mode, HitEditMode::SlipOnly);
    }
}

/// Position of a constant-tempo grid and the maximum spread of a grouped attack.
#[derive(Clone, Copy, Debug)]
pub struct QuantizeGrid {
    pub bpm: f64,
    pub subdivision: u32,
    pub origin_s: f64,
    pub strength: f64,
    pub tolerance_s: f64,
    pub group_window_s: f64,
}

/// Move nearby attacks by a shared offset so quantizing two drums played
/// together preserves their recorded timing difference.
pub fn quantize_grouped(
    hits: &[RecordedHit],
    grid: QuantizeGrid,
    pins: &[HitMove],
) -> Vec<HitMove> {
    let mut hits: Vec<_> = hits.iter().collect();
    hits.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.id.cmp(&b.id)));
    let pins: HashMap<_, _> = pins.iter().map(|pin| (pin.id.as_str(), pin)).collect();
    let step = 60.0 / grid.bpm / f64::from(grid.subdivision);
    let mut moves = Vec::new();
    let mut hits = hits.as_slice();
    while let Some(first) = hits.first() {
        let count = hits.partition_point(|hit| hit.at - first.at <= grid.group_window_s);
        let (group, later) = hits.split_at(count.max(1));
        let relative = first.at - grid.origin_s;
        let delta = (relative / step).round() * step - relative;
        let shift = group.iter()
            .find_map(|hit| pins.get(hit.id.as_str()).map(|pin| pin.target_time - hit.at))
            .unwrap_or_else(|| if delta.abs() <= grid.tolerance_s { 0.0 } else { delta * grid.strength });
        for hit in group {
            if let Some(pin) = pins.get(hit.id.as_str()) {
                moves.push((*pin).clone());
            } else if shift.abs() > 1e-6 {
                moves.push(HitMove { id: hit.id.clone(), target_time: hit.at + shift, mode: HitEditMode::Auto });
            }
        }
        hits = later;
    }
    moves
}

//! The leveler's four lanes as an item keeps them: the gate, de-breath,
//! ride and de-ess envelopes ([`super::envelope::Contributions`]) in
//! item-relative time, and the settings digest they came from. Written
//! with the item by the vocal leveler (`studio-engine`'s `vocal_level`)
//! and read by the expression editor (`editor::daw::state` re-exports
//! them).

use facet::Facet;

/// A generated envelope, as stored.
///
/// Sparse breakpoints with shapes, in **item-relative time** — seconds
/// from the item's start, not from the project's. That is possible
/// because this data is ours on both backends, and it is what makes
/// moving an item leave every curve aligned to its audio. It is also why
/// the four are not written as FX-parameter envelopes: those are *track*
/// envelopes in project time, so dragging the item would leave its gate
/// curve behind, and keeping them aligned would make item-move a write
/// path for envelope data — which nothing else in this design needs.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StoredEnvelope {
    pub points: Vec<StoredEnvPoint>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StoredEnvPoint {
    /// Seconds from the start of the item.
    pub t_s: f64,
    pub db: f64,
    pub hold: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StoredSpan {
    pub from_s: f64,
    pub to_s: f64,
    pub db: f64,
}

/// The four envelopes and the settings they came from, for one item.
///
/// The **composite is not here**: it is the take volume envelope, which
/// the DAW owns and plays. These four are ours, and they are what you
/// edit.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ItemEnvelopes {
    pub item_guid: String,
    pub gate: StoredEnvelope,
    pub breath: StoredEnvelope,
    pub sibilance: Vec<StoredSpan>,
    pub ride: StoredEnvelope,
    /// Which are switched off. Bypass is a mix decision, so it travels.
    pub bypass_gate: bool,
    pub bypass_breath: bool,
    pub bypass_sibilance: bool,
    pub bypass_ride: bool,
    /// The config digest these were generated from, so a reopened item
    /// knows whether they are still current without re-analysing.
    pub digest: u64,
}

impl ItemEnvelopes {
    pub fn capture(
        item_guid: impl Into<String>,
        curves: &super::envelope::Contributions,
        digest: u64,
    ) -> Self {
        let pts = |v: &[super::envelope::EnvPoint]| StoredEnvelope {
            points: v
                .iter()
                .map(|p| StoredEnvPoint {
                    t_s: p.t_s,
                    db: p.db,
                    hold: p.hold,
                })
                .collect(),
        };
        Self {
            item_guid: item_guid.into(),
            gate: pts(&curves.gate),
            breath: pts(&curves.breath),
            ride: pts(&curves.ride),
            sibilance: curves
                .sibilance
                .iter()
                .map(|s| StoredSpan {
                    from_s: s.from_s,
                    to_s: s.to_s,
                    db: s.db,
                })
                .collect(),
            bypass_gate: curves.bypass.gate,
            bypass_breath: curves.bypass.breath,
            bypass_sibilance: curves.bypass.sibilance,
            bypass_ride: curves.bypass.ride,
            digest,
        }
    }

    pub fn restore(&self) -> super::envelope::Contributions {
        use super::envelope::{Bypass, Contributions, EnvPoint, GainSpan};
        let pts = |e: &StoredEnvelope| {
            e.points
                .iter()
                .map(|p| EnvPoint {
                    t_s: p.t_s,
                    db: p.db,
                    hold: p.hold,
                })
                .collect::<Vec<_>>()
        };
        Contributions {
            gate: pts(&self.gate),
            breath: pts(&self.breath),
            ride: pts(&self.ride),
            sibilance: self
                .sibilance
                .iter()
                .map(|s| GainSpan {
                    from_s: s.from_s,
                    to_s: s.to_s,
                    db: s.db,
                })
                .collect(),
            bypass: Bypass {
                gate: self.bypass_gate,
                breath: self.bypass_breath,
                sibilance: self.bypass_sibilance,
                ride: self.bypass_ride,
            },
        }
    }
}

//! Dual-reverb composition — `BigSky` MX two-reverbs-per-preset routing.
//!
//! Two full [`ReverbChain`]s (each its own algorithm — including two
//! independent Convolution/Impulse slots — params, pan, mix) composed
//! per [`DualRouting`]:
//!
//! - `Series12` / `Series21`: one chain's full stereo output (its own
//!   dry/wet mix included) feeds the other — the walkthrough's
//!   "Magneto into Chamber" patch.
//! - `Parallel`: both chains process the same input; outputs sum with
//!   each chain's own mix law (`out = a + b − dry`, dry counted once),
//!   so the two mix knobs act like the hardware's independent
//!   per-reverb mixes. Combine with per-chain `pan` for the
//!   "two spaces offset left/right around a dry center" trick.
//! - `Split` / `SplitSwapped`: reverb 1 → left, reverb 2 → right, each
//!   mono-summed so the split is total.
//!
//! Scratch buffers are sized in `update()`; larger `process` calls are
//! chunked — no allocation on the audio thread.

use dsp_core::audio::{AudioConfig, Processor};

use crate::chain::ReverbChain;

/// Routing for the two reverbs in a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DualRouting {
    /// Only reverb A runs (single-reverb preset).
    #[default]
    Single,
    /// A feeds B.
    Series12,
    /// B feeds A.
    Series21,
    /// A and B run side by side, summed.
    Parallel,
    /// A → left channel, B → right channel.
    Split,
    /// A → right channel, B → left channel.
    SplitSwapped,
}

impl DualRouting {
    pub const COUNT: usize = 6;

    #[must_use]
    pub const fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Series12,
            2 => Self::Series21,
            3 => Self::Parallel,
            4 => Self::Split,
            5 => Self::SplitSwapped,
            _ => Self::Single,
        }
    }

    #[must_use]
    pub const fn to_index(self) -> usize {
        match self {
            Self::Single => 0,
            Self::Series12 => 1,
            Self::Series21 => 2,
            Self::Parallel => 3,
            Self::Split => 4,
            Self::SplitSwapped => 5,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Series12 => "Series 1>2",
            Self::Series21 => "Series 2>1",
            Self::Parallel => "Parallel",
            Self::Split => "Split",
            Self::SplitSwapped => "Split Swap",
        }
    }
}

/// Two reverb chains + routing (`BigSky` MX dual-reverb presets).
///
/// Chain B is built only when it is needed ([`b_mut`](Self::b_mut), or a
/// routing that plays it through [`set_routing`](Self::set_routing)): a
/// reverb is mostly `Single`, and a rig prepares one in every patch — B's
/// delay network was half of every reverb's memory, never heard.
pub struct DualReverb {
    pub a: ReverbChain,
    b: Option<Box<ReverbChain>>,
    /// What `update` last prepared for: a B built after it is prepared too.
    config: Option<AudioConfig>,
    /// Set through [`set_routing`](Self::set_routing) so B is there to play;
    /// a routing set here with no B plays A alone.
    pub routing: DualRouting,

    // Pre-allocated scratch: the dry input copy and B's working buffers.
    dry_l: Vec<f64>,
    dry_r: Vec<f64>,
    b_l: Vec<f64>,
    b_r: Vec<f64>,
}

impl DualReverb {
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: ReverbChain::new(),
            b: None,
            config: None,
            routing: DualRouting::Single,
            dry_l: vec![0.0; 512],
            dry_r: vec![0.0; 512],
            b_l: vec![0.0; 512],
            b_r: vec![0.0; 512],
        }
    }

    /// Copy the public parameter surface from one slot to the other
    /// (`BigSky` MX "copy settings"). Copies params/mix/pan/EQ/duck/trem —
    /// NOT the algorithm selection or its internal state, so you can
    /// carry one slot's settings onto a different engine. Call
    /// `update()` (or `update_params()` on the destination) afterwards
    /// so filters and smoothers pick the values up.
    pub fn copy_params(&mut self, from_a: bool) {
        if from_a {
            let surface = self.a.param_surface();
            self.b_mut().apply_surface(&surface);
        } else if let Some(b) = self.b.as_deref() {
            let surface = b.param_surface();
            self.a.apply_surface(&surface);
        }
    }

    /// Chain B, built (and prepared, if this is) if it was not — an
    /// allocation: a param write's or a preparation's, never the audio
    /// callback's.
    pub fn b_mut(&mut self) -> &mut ReverbChain {
        let config = self.config;
        self.b.get_or_insert_with(|| {
            let mut c = Box::new(ReverbChain::new());
            if let Some(cfg) = config {
                c.update(cfg);
            }
            c
        })
    }

    /// Chain B, if it has been built.
    #[must_use]
    pub fn b(&self) -> Option<&ReverbChain> {
        self.b.as_deref()
    }

    /// Play `routing`, building B if it plays it.
    pub fn set_routing(&mut self, routing: DualRouting) {
        self.routing = routing;
        if routing != DualRouting::Single {
            self.b_mut();
        }
    }

    /// Max samples per inner chunk (scratch capacity).
    const fn chunk_capacity(&self) -> usize {
        self.dry_l.len()
    }

    fn process_chunk(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len();
        // The caller chunks to `chunk_capacity()`, so the scratch buffers are
        // always long enough. Clamping rather than asserting keeps the render
        // callback panic-free if that ever stops holding.
        let n = n.min(self.chunk_capacity()).min(right.len());
        // No B (a routing set without `set_routing`): A alone — B is never
        // built here.
        let Some(b) = self.b.as_deref_mut() else {
            self.a.process(left, right);
            return;
        };
        match self.routing {
            DualRouting::Single => {
                self.a.process(left, right);
            }
            DualRouting::Series12 => {
                self.a.process(left, right);
                b.process(left, right);
            }
            DualRouting::Series21 => {
                b.process(left, right);
                self.a.process(left, right);
            }
            DualRouting::Parallel => {
                let (Some(dry_l), Some(dry_r)) = (self.dry_l.get_mut(..n), self.dry_r.get_mut(..n))
                else {
                    return;
                };
                let (Some(head_l), Some(head_r)) = (left.get(..n), right.get(..n)) else {
                    return;
                };
                dry_l.copy_from_slice(head_l);
                dry_r.copy_from_slice(head_r);
                let (Some(b_l), Some(b_r)) = (self.b_l.get_mut(..n), self.b_r.get_mut(..n)) else {
                    return;
                };
                b_l.copy_from_slice(head_l);
                b_r.copy_from_slice(head_r);

                self.a.process(left, right);
                b.process(b_l, b_r);

                // Sum of both chains' mix laws, dry counted once:
                // out = dry·(1 − mixA − mixB) + wetA·mixA + wetB·mixB.
                for (((l, r), (bl, br)), (dl, dr)) in left
                    .iter_mut()
                    .zip(right.iter_mut())
                    .zip(b_l.iter().zip(b_r.iter()))
                    .zip(self.dry_l.iter().zip(self.dry_r.iter()))
                    .take(n)
                {
                    *l += *bl - *dl;
                    *r += *br - *dr;
                }
            }
            DualRouting::Split | DualRouting::SplitSwapped => {
                let (Some(b_l), Some(b_r)) = (self.b_l.get_mut(..n), self.b_r.get_mut(..n)) else {
                    return;
                };
                let (Some(head_l), Some(head_r)) = (left.get(..n), right.get(..n)) else {
                    return;
                };
                b_l.copy_from_slice(head_l);
                b_r.copy_from_slice(head_r);

                self.a.process(left, right);
                b.process(b_l, b_r);

                let swapped = self.routing == DualRouting::SplitSwapped;
                for ((l, r), (bl, br)) in left
                    .iter_mut()
                    .zip(right.iter_mut())
                    .zip(b_l.iter().zip(b_r.iter()))
                    .take(n)
                {
                    let a_mono = (*l + *r) * 0.5;
                    let b_mono = (*bl + *br) * 0.5;
                    (*l, *r) = if swapped {
                        (b_mono, a_mono)
                    } else {
                        (a_mono, b_mono)
                    };
                }
            }
        }
    }
}

impl Default for DualReverb {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for DualReverb {
    fn reset(&mut self) {
        self.a.reset();
        if let Some(b) = self.b.as_deref_mut() {
            b.reset();
        }
    }

    fn update(&mut self, config: AudioConfig) {
        self.config = Some(config);
        self.a.update(config);
        if let Some(b) = self.b.as_deref_mut() {
            b.update(config);
        }

        let cap = config.max_buffer_size.max(64);
        if self.dry_l.len() < cap {
            self.dry_l.resize(cap, 0.0);
            self.dry_r.resize(cap, 0.0);
            self.b_l.resize(cap, 0.0);
            self.b_r.resize(cap, 0.0);
        }
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len().min(right.len());
        let cap = self.chunk_capacity();
        let mut pos = 0;
        while pos < n {
            let end = pos.saturating_add(cap).min(n);
            // `end <= n <= min(len)`, so both slices exist; bailing on the
            // impossible `None` keeps the render callback panic-free.
            let (Some(l), Some(r)) = (left.get_mut(pos..end), right.get_mut(pos..end)) else {
                return;
            };
            self.process_chunk(l, r);
            pos = end;
        }
    }
}

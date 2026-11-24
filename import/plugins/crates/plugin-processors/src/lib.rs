//! The plugins' effects, headless: each one's parameter table and its
//! `dsp` engines wired behind one [`BlockProcessor`] interface. No nice-plug, no
//! Dioxus, no allocation on the audio path, `no_std` + `alloc`; so the same
//! EQ runs as a CLAP here, as a block inside Signal (`signal-core`), and
//! compiled into Studio's engine on iOS and in the browser.
//!
//! The algorithms are `dsp`'s; this crate decides what an effect *is*:
//! which parameters, in what units and ranges, how they print and parse
//! ([`param`]), and how they reach the engine.
//!
//! | name     | feature    | engine                                  |
//! |----------|------------|-----------------------------------------|
//! | `gain`   | (always)   | level and constant-power pan            |
//! | `gate`   | `gate`     | `dsp::gate::DrumGate`                   |
//! | `drive`  | `saturate` | `dsp::saturate::preamp::ClassAPreamp`   |
//! | `eq`     | `eq`       | `dsp_core` biquads (a channel strip)    |
//! | `comp`   | `comp`     | `dsp::comp`, the generic model          |
//! | `delay`  | `delay`    | `dsp::delay::clean_delay`               |
//! | `reverb` | `reverb`   | `dsp::reverb::ReverbChain`              |
//!
//! The plugin apps (`apps/plugins/*`) still carry their own nice-plug
//! parameter trees; moving each onto its table here is the follow-up that
//! makes "the EQ block" and "the EQ plugin" one thing in every parameter.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use alloc::boxed::Box;
use alloc::string::String;
use core::fmt;

pub mod param;

#[cfg(feature = "comp")]
pub mod comp;
#[cfg(feature = "delay")]
pub mod delay;
#[cfg(feature = "eq")]
pub mod eq;
#[cfg(feature = "gate")]
pub mod gate;
#[cfg(feature = "reverb")]
pub mod reverb;
#[cfg(feature = "saturate")]
pub mod saturate;

pub use param::{ParamSpec, Unit, ValueText};

/// A modulation source a block exposes: an envelope, its gain reduction,
/// a detected pitch. Read after each [`BlockProcessor::process`]; Signal's
/// macro and modulation system routes it to any block's parameters. A
/// block's CLAP form publishes the same sources over Signal's extension
/// (`com.fasttrackstudio.signal/1`), so no route depends on a block being
/// compiled in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModSourceSpec {
    /// Stable id, what a route names.
    pub id: &'static str,
    pub name: &'static str,
    /// What its value means (gain reduction in dB, an envelope 0–1, …).
    pub unit: Unit,
}

/// A MIDI event for an instrument block, at a frame offset into the piece
/// handed to the next [`BlockProcessor::process`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Midi {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    Cc {
        channel: u8,
        cc: u8,
        value: u8,
    },
    /// −8192…8191.
    PitchBend {
        channel: u8,
        value: i16,
    },
}

/// One block, compiled in: the interface Signal's chain holds every block
/// through, and the one a block's own CLAP form wraps (nice-plug, in
/// `apps/plugins/*`), so both forms are the same processor.
///
/// - **Parameters** are addressed by their spec's stable `id`. The ids are
///   the same in the CLAP form, and every parameter is modulatable: Signal
///   modulates by computing the value itself ([`ParamSpec::modulated`]),
///   the CLAP form by `CLAP_EVENT_PARAM_MOD`, the same arithmetic.
/// - **Sample accuracy** is the caller's: it splits the block at each
///   parameter event and calls [`set_param`](Self::set_param) between
///   pieces, as nice-plug does with sample-accurate automation. A block
///   applies a value at once (no smoothing of its own outside the engine),
///   which is what lets the two forms null.
///
/// Built (and allocated) off the audio thread; everything below runs on it
/// and must not allocate, lock or block.
pub trait BlockProcessor: Send {
    /// Its parameters, in index order.
    fn params(&self) -> &'static [ParamSpec];

    /// Set parameter `index` to a plain value (already clamped to its spec;
    /// already modulated, when modulation is on).
    ///
    /// Idempotent: setting the value it already has changes nothing (no
    /// reset glide, no re-seeded smoother). A host may set a parameter
    /// more than once at a change point (a CLAP gets a value and a
    /// modulation event), and the null test holds a block to this.
    fn set_param(&mut self, index: usize, value: f32);

    /// Clear its state (filters, tails, envelopes), keeping its parameters.
    fn reset(&mut self);

    /// Process one piece in place: at most the `max_block` it was built for.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Frames of latency it adds.
    fn latency(&self) -> u32 {
        0
    }

    /// A MIDI event at the start of the next piece (instrument blocks: the
    /// sampler). Effects ignore it.
    fn midi(&mut self, event: Midi) {
        let _ = event;
    }

    /// The modulation sources it exposes, in index order.
    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        &[]
    }

    /// Source `index`'s value after the last piece.
    fn mod_source(&self, index: usize) -> f32 {
        let _ = index;
        0.0
    }
}

/// Why an effect couldn't be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildError(pub String);

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BuildError {}

/// Every effect this build has, by name.
pub const NAMES: &[&str] = &[
    "gain",
    #[cfg(feature = "gate")]
    "gate",
    #[cfg(feature = "saturate")]
    "drive",
    #[cfg(feature = "eq")]
    "eq",
    #[cfg(feature = "comp")]
    "comp",
    #[cfg(feature = "delay")]
    "delay",
    #[cfg(feature = "reverb")]
    "reverb",
];

/// Build effect `name` for `sample_rate` and blocks of up to `max_block`,
/// at its parameters' defaults. `None` for a name this build doesn't have.
///
/// # Errors
///
/// [`BuildError`] when the engine refuses the rate or block size.
pub fn build(
    name: &str,
    sample_rate: f32,
    max_block: usize,
) -> Option<Result<Box<dyn BlockProcessor>, BuildError>> {
    let _ = max_block;
    let built: Result<Box<dyn BlockProcessor>, BuildError> = match name {
        "gain" => Ok(Box::new(Gain::new())),
        #[cfg(feature = "gate")]
        "gate" => Ok(Box::new(gate::Gate::new(sample_rate))),
        #[cfg(feature = "saturate")]
        "drive" => Ok(Box::new(saturate::Drive::new(sample_rate))),
        #[cfg(feature = "eq")]
        "eq" => Ok(Box::new(eq::Eq::new(sample_rate))),
        #[cfg(feature = "comp")]
        "comp" => {
            comp::Comp::new(sample_rate, max_block).map(|c| Box::new(c) as Box<dyn BlockProcessor>)
        }
        #[cfg(feature = "delay")]
        "delay" => Ok(Box::new(delay::Delay::new(sample_rate))),
        #[cfg(feature = "reverb")]
        "reverb" => Ok(Box::new(reverb::Reverb::new(sample_rate, max_block))),
        _ => return None,
    };
    let _ = sample_rate;
    Some(built.map(|mut p| {
        for (i, spec) in p.params().iter().enumerate() {
            p.set_param(i, spec.default);
        }
        p.reset();
        p
    }))
}

/// Effect `name`'s parameter schema, without building it: what a CLAP
/// backend standing in for the compiled-in block runs (the same ids).
#[must_use]
pub fn params_of(name: &str) -> Option<&'static [ParamSpec]> {
    Some(match name {
        "gain" => GAIN_PARAMS,
        #[cfg(feature = "gate")]
        "gate" => gate::PARAMS,
        #[cfg(feature = "saturate")]
        "drive" => saturate::PARAMS,
        #[cfg(feature = "eq")]
        "eq" => eq::PARAMS,
        #[cfg(feature = "comp")]
        "comp" => comp::PARAMS,
        #[cfg(feature = "delay")]
        "delay" => delay::PARAMS,
        #[cfg(feature = "reverb")]
        "reverb" => reverb::PARAMS,
        _ => return None,
    })
}

/// Effect `name`'s modulation sources, without building it.
#[must_use]
pub fn sources_of(name: &str) -> &'static [ModSourceSpec] {
    match name {
        #[cfg(feature = "gate")]
        "gate" => gate::SOURCES,
        #[cfg(feature = "comp")]
        "comp" => comp::SOURCES,
        _ => &[],
    }
}

/// Decibels to a linear gain.
#[must_use]
pub fn db_to_gain(db: f32) -> f32 {
    libm::powf(10.0, db / 20.0)
}

/// Level and pan.
pub struct Gain {
    gain: f32,
    pan: f32,
}

pub const GAIN_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("gain", "Gain", Unit::Db, -60.0, 24.0, 0.0),
    ParamSpec::new("pan", "Pan", Unit::Plain(2), -1.0, 1.0, 0.0),
];

impl Gain {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            gain: 1.0,
            pan: 0.0,
        }
    }
}

impl Default for Gain {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockProcessor for Gain {
    fn params(&self) -> &'static [ParamSpec] {
        GAIN_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.gain = db_to_gain(value),
            1 => self.pan = value,
            _ => {}
        }
    }

    fn reset(&mut self) {}

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        // Constant-power pan, unity at centre.
        let angle = (self.pan + 1.0) * core::f32::consts::FRAC_PI_4;
        let gl = self.gain * libm::cosf(angle) * core::f32::consts::SQRT_2;
        let gr = self.gain * libm::sinf(angle) * core::f32::consts::SQRT_2;
        left.iter_mut().for_each(|s| *s *= gl);
        right.iter_mut().for_each(|s| *s *= gr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_builds_and_passes_a_block() {
        for name in NAMES {
            let Some(Ok(mut p)) = build(name, 48_000.0, 256) else {
                panic!("{name} did not build");
            };
            let mut l = alloc::vec![0.25_f32; 256];
            let mut r = alloc::vec![-0.25_f32; 256];
            p.process(&mut l, &mut r);
            assert!(
                l.iter().chain(&r).all(|s| s.is_finite()),
                "{name} made a non-finite sample"
            );
        }
        assert!(build("nope", 48_000.0, 256).is_none());
        for name in NAMES {
            let built = build(name, 48_000.0, 256).and_then(Result::ok);
            assert_eq!(built.map(|p| p.params()), params_of(name), "{name}");
        }
    }
}

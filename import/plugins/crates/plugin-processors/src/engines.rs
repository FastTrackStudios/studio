//! Engines: the compiled-in DSP every algorithm runs on.
//!
//! An engine is a [`BlockProcessor`] whose parameters are its **targets**:
//! a versioned schema (`eq.canonical@1`, `reverb.plate@1`) that algorithm
//! files map their controls onto. A target id never changes meaning within
//! a version; a new meaning is a new version beside the old one, so a
//! community algorithm written against `@1` keeps working.
//!
//! | engine | targets |
//! |---|---|
//! | `eq.canonical@1` | `band.{1..24}.{used,on,shape,freq,gain,q,slope}`, `gain_scale`, `output` |
//! | `dyn_eq.bands@1` | `band.{1..4}.{used,shape,freq,q,gain,range,source,threshold,attack,hold,release}`, `velocity`, `output` |
//! | `reverb.{room,hall,plate,spring,cloud,bloom,shimmer,chorale,magneto,nonlinear,swell,reflections,velvet,freeverb,random}@1` | the shared space controls (`decay`, `size`, `diffusion`, …) and the algorithm's own two (`extra_a`, `extra_b`) |
//! | `delay.{tape,clean,bbd,lofi,shimmer,reverse,pitch,rhythm,drum,oilcan,multitap,spectral,filter,reverb}@1` | `time`, `feedback`, `mix`, `high_cut`, `low_cut`, `width`, `drive`, `wow`, `flutter` |
//! | `comp.generic@1`, `saturate.preamp@1`, `gate.drum@1`, `gain@1` | the effect's own parameters |

use alloc::boxed::Box;

use crate::{BlockProcessor, BuildError, ParamSpec};

/// One engine this build has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineInfo {
    /// `eq.canonical@1`.
    pub id: &'static str,
    /// The block family it serves: `eq`, `reverb`, `delay`, …
    pub family: &'static str,
    /// What a person calls it: "Plate".
    pub name: &'static str,
}

/// Every engine in this build.
#[must_use]
pub fn all() -> alloc::vec::Vec<EngineInfo> {
    let mut out = alloc::vec![EngineInfo {
        id: "gain@1",
        family: "gain",
        name: "Gain"
    }];
    #[cfg(feature = "gate")]
    out.push(EngineInfo {
        id: "gate.drum@1",
        family: "gate",
        name: "Drum Gate",
    });
    #[cfg(feature = "saturate")]
    out.push(EngineInfo {
        id: "saturate.preamp@1",
        family: "drive",
        name: "Class-A Preamp",
    });
    #[cfg(feature = "comp")]
    out.push(EngineInfo {
        id: "comp.generic@1",
        family: "comp",
        name: "Compressor",
    });
    #[cfg(feature = "eq")]
    out.push(EngineInfo {
        id: "eq.canonical@1",
        family: "eq",
        name: "FTS EQ",
    });
    #[cfg(feature = "eq")]
    out.push(EngineInfo {
        id: "dyn_eq.bands@1",
        family: "dyn_eq",
        name: "Dynamic EQ",
    });
    #[cfg(feature = "reverb")]
    out.extend(reverb::ALGORITHMS.iter().map(|a| EngineInfo {
        id: a.engine_id,
        family: "reverb",
        name: a.name,
    }));
    #[cfg(feature = "delay")]
    out.extend(delay::STYLES.iter().map(|s| EngineInfo {
        id: s.engine_id,
        family: "delay",
        name: s.name,
    }));
    out
}

/// Engine `id`'s target schema, without building it.
#[must_use]
pub fn targets_of(id: &str) -> Option<&'static [ParamSpec]> {
    Some(match id {
        "gain@1" => crate::GAIN_PARAMS,
        #[cfg(feature = "gate")]
        "gate.drum@1" => crate::gate::PARAMS,
        #[cfg(feature = "saturate")]
        "saturate.preamp@1" => crate::saturate::PARAMS,
        #[cfg(feature = "comp")]
        "comp.generic@1" => crate::comp::PARAMS,
        #[cfg(feature = "eq")]
        "eq.canonical@1" => eq::targets(),
        #[cfg(feature = "eq")]
        "dyn_eq.bands@1" => crate::dyn_eq::targets(),
        #[cfg(feature = "reverb")]
        id if reverb::find(id).is_some() => reverb::TARGETS,
        #[cfg(feature = "delay")]
        id if delay::find(id).is_some() => delay::TARGETS,
        _ => return None,
    })
}

/// Build engine `id` for `sample_rate` and blocks of up to `max_block`, its
/// targets at their defaults. `None` for an id this build doesn't have.
///
/// # Errors
///
/// [`BuildError`] when the engine refuses the rate or block size.
pub fn build(
    id: &str,
    sample_rate: f32,
    max_block: usize,
) -> Option<Result<Box<dyn BlockProcessor>, BuildError>> {
    let built: Result<Box<dyn BlockProcessor>, BuildError> = match id {
        "gain@1" => Ok(Box::new(crate::Gain::new())),
        #[cfg(feature = "gate")]
        "gate.drum@1" => Ok(Box::new(crate::gate::Gate::with_max_block(
            sample_rate,
            max_block,
        ))),
        #[cfg(feature = "saturate")]
        "saturate.preamp@1" => Ok(Box::new(crate::saturate::Drive::new(sample_rate))),
        #[cfg(feature = "comp")]
        "comp.generic@1" => crate::comp::Comp::new(sample_rate, max_block)
            .map(|c| Box::new(c) as Box<dyn BlockProcessor>),
        #[cfg(feature = "eq")]
        "eq.canonical@1" => Ok(Box::new(eq::CanonicalEqEngine::new(sample_rate, max_block))),
        #[cfg(feature = "eq")]
        "dyn_eq.bands@1" => Ok(Box::new(crate::dyn_eq::DynEqEngine::new(
            sample_rate,
            max_block,
        ))),
        #[cfg(feature = "reverb")]
        id if reverb::find(id).is_some() => {
            let algo = reverb::find(id)?;
            Ok(Box::new(reverb::ReverbEngine::new(
                algo,
                sample_rate,
                max_block,
            )))
        }
        #[cfg(feature = "delay")]
        id if delay::find(id).is_some() => {
            let style = delay::find(id)?;
            Ok(Box::new(delay::DelayEngine::new(
                style,
                sample_rate,
                max_block,
            )))
        }
        _ => return None,
    };
    let _ = (sample_rate, max_block);
    Some(built.map(|mut p| {
        for (i, spec) in p.params().iter().enumerate() {
            p.set_param(i, spec.default);
        }
        p.reset();
        p
    }))
}

// ── eq.canonical@1 ──────────────────────────────────────────────────────

#[cfg(feature = "eq")]
pub mod eq {
    //! `eq.canonical@1`: the FTS EQ engine (`dsp::eq::host::CanonicalEq`),
    //! 24 bands. What the FTS EQ plugin runs; every EQ algorithm (the
    //! parametric face, an SSL face, a community one) maps onto these.

    use alloc::boxed::Box;
    use alloc::format;
    use alloc::vec;
    use alloc::vec::Vec;

    use dsp::eq::host::{CanonicalBandConfig, CanonicalBandDynamics, CanonicalEq};
    use dsp_core::num::narrow;

    use crate::{BlockProcessor, ParamSpec, Unit};

    /// Bands in the engine.
    pub const BANDS: usize = dsp::eq::engine::EQ_BANDS;

    /// The canonical shapes, by index (`dsp::eq::design::slope::FilterShape`).
    pub const SHAPES: &[&str] = &[
        "Bell",
        "Low Shelf",
        "High Shelf",
        "Low Cut",
        "High Cut",
        "Notch",
        "Band Pass",
        "Tilt Shelf",
        "Flat Tilt",
        "All Pass",
        "Band Shelf",
        "Shelf Alt",
        "Band Pass Alt",
    ];

    /// Per band, in target order.
    const FIELDS: usize = 7;

    /// The engine's targets: built once (24 bands of ids), then `'static`.
    #[must_use]
    pub fn targets() -> &'static [ParamSpec] {
        static TARGETS: std::sync::OnceLock<&'static [ParamSpec]> = std::sync::OnceLock::new();
        TARGETS.get_or_init(|| {
            let leak = |s: alloc::string::String| -> &'static str { Box::leak(s.into_boxed_str()) };
            let mut t: Vec<ParamSpec> = Vec::with_capacity(BANDS * FIELDS + 2);
            for b in 1..=BANDS {
                // Default frequencies spread log-evenly, 30 Hz – 16 kHz.
                let pos = crate::util_f32(b - 1) / crate::util_f32(BANDS - 1);
                let freq = 30.0 * libm::powf(16_000.0 / 30.0, pos);
                t.push(ParamSpec::new(
                    leak(format!("band.{b}.used")),
                    leak(format!("Band {b} Used")),
                    Unit::Toggle,
                    0.0,
                    1.0,
                    0.0,
                ));
                t.push(ParamSpec::new(
                    leak(format!("band.{b}.on")),
                    leak(format!("Band {b} On")),
                    Unit::Toggle,
                    0.0,
                    1.0,
                    1.0,
                ));
                t.push(ParamSpec::choice(
                    leak(format!("band.{b}.shape")),
                    leak(format!("Band {b} Shape")),
                    SHAPES,
                ));
                t.push(
                    ParamSpec::new(
                        leak(format!("band.{b}.freq")),
                        leak(format!("Band {b} Freq")),
                        Unit::Hz,
                        10.0,
                        30_000.0,
                        freq,
                    )
                    .skewed(0.25),
                );
                t.push(ParamSpec::new(
                    leak(format!("band.{b}.gain")),
                    leak(format!("Band {b} Gain")),
                    Unit::Db,
                    -30.0,
                    30.0,
                    0.0,
                ));
                t.push(
                    ParamSpec::new(
                        leak(format!("band.{b}.q")),
                        leak(format!("Band {b} Q")),
                        Unit::Plain(2),
                        0.025,
                        40.0,
                        0.707,
                    )
                    .skewed(0.35),
                );
                t.push(ParamSpec::new(
                    leak(format!("band.{b}.slope")),
                    leak(format!("Band {b} Slope")),
                    Unit::Plain(2),
                    0.5,
                    16.0,
                    2.0,
                ));
            }
            t.push(ParamSpec::new(
                "gain_scale",
                "Gain Scale",
                Unit::Percent,
                0.0,
                2.0,
                1.0,
            ));
            t.push(ParamSpec::new(
                "output",
                "Output",
                Unit::Db,
                -24.0,
                24.0,
                0.0,
            ));
            Box::leak(t.into_boxed_slice())
        })
    }

    pub struct CanonicalEqEngine {
        eq: Box<CanonicalEq>,
        bands: [CanonicalBandConfig; BANDS],
        output: f64,
        l: Vec<f64>,
        r: Vec<f64>,
    }

    impl CanonicalEqEngine {
        #[must_use]
        pub fn new(sample_rate: f32, max_block: usize) -> Self {
            let sr = f64::from(sample_rate);
            let mut eq = Box::new(CanonicalEq::new(sr));
            eq.prepare(sr, u32::try_from(max_block).unwrap_or(1024));
            Self {
                eq,
                bands: [CanonicalBandConfig::default(); BANDS],
                output: 1.0,
                l: vec![0.0; max_block],
                r: vec![0.0; max_block],
            }
        }
    }

    impl BlockProcessor for CanonicalEqEngine {
        fn params(&self) -> &'static [ParamSpec] {
            targets()
        }

        fn set_param(&mut self, index: usize, value: f32) {
            let globals = BANDS * FIELDS;
            if index >= globals {
                match index - globals {
                    0 => self.eq.set_gain_scale(f64::from(value)),
                    1 => self.output = f64::from(crate::db_to_gain(value)),
                    _ => {}
                }
                return;
            }
            let (band, field) = (index / FIELDS, index % FIELDS);
            let Some(cfg) = self.bands.get_mut(band) else {
                return;
            };
            let before = *cfg;
            let v = f64::from(value);
            match field {
                0 => cfg.used = value >= 0.5,
                1 => cfg.enabled = value >= 0.5,
                2 => cfg.shape = u32::try_from(dsp_core::num::f32_to_index(value)).unwrap_or(0),
                3 => cfg.freq_hz = v,
                4 => cfg.gain_db = v,
                5 => cfg.q = v,
                _ => cfg.slope = v,
            }
            // A repeated set changes nothing (the trait's rule), and a band
            // redesigns only when something about it moved.
            if *cfg != before {
                let cfg = *cfg;
                self.eq
                    .set_canonical_band(band, cfg, CanonicalBandDynamics::default());
            }
        }

        fn reset(&mut self) {
            self.eq.reset();
        }

        fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
            let n = left.len().min(right.len()).min(self.l.len());
            let (Some(l), Some(r)) = (self.l.get_mut(..n), self.r.get_mut(..n)) else {
                return;
            };
            for ((d, s), (e, t)) in l
                .iter_mut()
                .zip(left.iter())
                .zip(r.iter_mut().zip(right.iter()))
            {
                *d = f64::from(*s);
                *e = f64::from(*t);
            }
            self.eq.process(l, r);
            let g = self.output;
            for ((d, s), (e, t)) in left
                .iter_mut()
                .zip(l.iter())
                .zip(right.iter_mut().zip(r.iter()))
            {
                *d = narrow(*s * g);
                *e = narrow(*t * g);
            }
        }

        fn latency(&self) -> u32 {
            self.eq.latency()
        }
    }
}

// ── reverb.<algorithm>@1 ────────────────────────────────────────────────

#[cfg(feature = "reverb")]
pub mod reverb {
    //! One engine per reverb algorithm: each is its own DSP
    //! (`dsp::reverb::ReverbChain` on that algorithm), with its own look in
    //! the views and its own algorithm file.

    use alloc::vec;
    use alloc::vec::Vec;

    use dsp::reverb::{AlgorithmType, ReverbChain};
    use dsp_core::audio::{AudioConfig, Processor as _};
    use dsp_core::num::narrow;

    use crate::{BlockProcessor, ParamSpec, Unit};

    /// A reverb algorithm as an engine.
    #[derive(Clone, Copy, Debug)]
    pub struct Algorithm {
        pub engine_id: &'static str,
        pub name: &'static str,
        pub algorithm: AlgorithmType,
    }

    pub const ALGORITHMS: &[Algorithm] = &[
        Algorithm {
            engine_id: "reverb.room@1",
            name: "Room",
            algorithm: AlgorithmType::Room,
        },
        Algorithm {
            engine_id: "reverb.hall@1",
            name: "Hall",
            algorithm: AlgorithmType::Hall,
        },
        Algorithm {
            engine_id: "reverb.plate@1",
            name: "Plate",
            algorithm: AlgorithmType::Plate,
        },
        Algorithm {
            engine_id: "reverb.spring@1",
            name: "Spring",
            algorithm: AlgorithmType::Spring,
        },
        Algorithm {
            engine_id: "reverb.cloud@1",
            name: "Cloud",
            algorithm: AlgorithmType::Cloud,
        },
        Algorithm {
            engine_id: "reverb.bloom@1",
            name: "Bloom",
            algorithm: AlgorithmType::Bloom,
        },
        Algorithm {
            engine_id: "reverb.shimmer@1",
            name: "Shimmer",
            algorithm: AlgorithmType::Shimmer,
        },
        Algorithm {
            engine_id: "reverb.chorale@1",
            name: "Chorale",
            algorithm: AlgorithmType::Chorale,
        },
        Algorithm {
            engine_id: "reverb.magneto@1",
            name: "Magneto",
            algorithm: AlgorithmType::Magneto,
        },
        Algorithm {
            engine_id: "reverb.nonlinear@1",
            name: "Non-Linear",
            algorithm: AlgorithmType::NonLinear,
        },
        Algorithm {
            engine_id: "reverb.swell@1",
            name: "Swell",
            algorithm: AlgorithmType::Swell,
        },
        Algorithm {
            engine_id: "reverb.reflections@1",
            name: "Reflections",
            algorithm: AlgorithmType::Reflections,
        },
        Algorithm {
            engine_id: "reverb.velvet@1",
            name: "Velvet",
            algorithm: AlgorithmType::Velvet,
        },
        Algorithm {
            engine_id: "reverb.freeverb@1",
            name: "FreeVerb",
            algorithm: AlgorithmType::FreeVerb,
        },
        Algorithm {
            engine_id: "reverb.random@1",
            name: "Random Space",
            algorithm: AlgorithmType::Random,
        },
    ];

    #[must_use]
    pub fn find(engine_id: &str) -> Option<&'static Algorithm> {
        ALGORITHMS.iter().find(|a| a.engine_id == engine_id)
    }

    /// Every reverb engine's targets (the algorithm's own two are
    /// `extra_a` / `extra_b`; its algorithm file names them).
    pub const TARGETS: &[ParamSpec] = &[
        ParamSpec::new("decay", "Decay", Unit::Percent, 0.0, 1.0, 0.5),
        ParamSpec::new("size", "Size", Unit::Percent, 0.0, 1.0, 0.5),
        ParamSpec::new("diffusion", "Diffusion", Unit::Percent, 0.0, 1.0, 0.7),
        ParamSpec::new("damping", "Damping", Unit::Percent, 0.0, 1.0, 0.4),
        ParamSpec::new("modulation", "Modulation", Unit::Percent, 0.0, 1.0, 0.2),
        ParamSpec::new("tone", "Tone", Unit::Plain(2), -1.0, 1.0, 0.0),
        ParamSpec::new("predelay", "Pre-delay", Unit::Ms, 0.0, 250.0, 10.0).skewed(0.5),
        ParamSpec::new("mix", "Mix", Unit::Percent, 0.0, 1.0, 0.25),
        ParamSpec::new("width", "Width", Unit::Percent, 0.0, 1.0, 1.0),
        ParamSpec::new("low_cut", "Low Cut", Unit::Hz, 20.0, 1000.0, 20.0).skewed(0.25),
        ParamSpec::new("high_cut", "High Cut", Unit::Hz, 1000.0, 20_000.0, 20_000.0).skewed(0.25),
        ParamSpec::new("extra_a", "Character A", Unit::Percent, 0.0, 1.0, 0.5),
        ParamSpec::new("extra_b", "Character B", Unit::Percent, 0.0, 1.0, 0.5),
    ];

    pub struct ReverbEngine {
        chain: ReverbChain,
        l: Vec<f64>,
        r: Vec<f64>,
    }

    impl ReverbEngine {
        #[must_use]
        pub fn new(algo: &Algorithm, sample_rate: f32, max_block: usize) -> Self {
            let config = AudioConfig {
                sample_rate: f64::from(sample_rate),
                max_buffer_size: max_block,
            };
            let mut chain = ReverbChain::new();
            // The algorithm builds its engine here, off the audio thread.
            chain.set_algorithm(algo.algorithm);
            chain.update(config);
            Self {
                chain,
                l: vec![0.0; max_block],
                r: vec![0.0; max_block],
            }
        }
    }

    impl BlockProcessor for ReverbEngine {
        fn params(&self) -> &'static [ParamSpec] {
            TARGETS
        }

        fn set_param(&mut self, index: usize, value: f32) {
            let v = f64::from(value);
            let c = &mut self.chain;
            let slot: &mut f64 = match index {
                0 => &mut c.params.decay,
                1 => &mut c.params.size,
                2 => &mut c.params.diffusion,
                3 => &mut c.params.damping,
                4 => &mut c.params.modulation,
                5 => &mut c.params.tone,
                6 => &mut c.predelay_ms,
                7 => &mut c.mix,
                8 => &mut c.width,
                9 => &mut c.input_hp_freq,
                10 => &mut c.input_lp_freq,
                11 => &mut c.params.extra_a,
                12 => &mut c.params.extra_b,
                _ => return,
            };
            if slot.to_bits() == v.to_bits() {
                return;
            }
            *slot = v;
            c.update_params();
        }

        fn reset(&mut self) {
            self.chain.reset();
        }

        fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
            let n = left.len().min(right.len()).min(self.l.len());
            let (Some(l), Some(r)) = (self.l.get_mut(..n), self.r.get_mut(..n)) else {
                return;
            };
            for ((d, s), (e, t)) in l
                .iter_mut()
                .zip(left.iter())
                .zip(r.iter_mut().zip(right.iter()))
            {
                *d = f64::from(*s);
                *e = f64::from(*t);
            }
            self.chain.process(l, r);
            for ((d, s), (e, t)) in left
                .iter_mut()
                .zip(l.iter())
                .zip(right.iter_mut().zip(r.iter()))
            {
                *d = narrow(*s);
                *e = narrow(*t);
            }
        }
    }
}

// ── delay.<style>@1 ─────────────────────────────────────────────────────

#[cfg(feature = "delay")]
pub mod delay {
    //! One engine per delay style (`dsp::delay::chain::DelayChain` on that
    //! style): tape, BBD, lo-fi, shimmer, … each its own DSP.

    use alloc::vec;
    use alloc::vec::Vec;

    use dsp::delay::chain::DelayChain;
    use dsp::delay::engine::DelayStyle;
    use dsp_core::audio::{AudioConfig, Processor as _};
    use dsp_core::num::narrow;

    use crate::{BlockProcessor, ParamSpec, Unit};

    #[derive(Clone, Copy, Debug)]
    pub struct Style {
        pub engine_id: &'static str,
        pub name: &'static str,
        pub style: DelayStyle,
    }

    pub const STYLES: &[Style] = &[
        Style {
            engine_id: "delay.tape@1",
            name: "Tape",
            style: DelayStyle::Tape,
        },
        Style {
            engine_id: "delay.clean@1",
            name: "Clean",
            style: DelayStyle::Clean,
        },
        Style {
            engine_id: "delay.bbd@1",
            name: "BBD",
            style: DelayStyle::Bbd,
        },
        Style {
            engine_id: "delay.lofi@1",
            name: "Lo-Fi",
            style: DelayStyle::LoFi,
        },
        Style {
            engine_id: "delay.shimmer@1",
            name: "Shimmer",
            style: DelayStyle::Shimmer,
        },
        Style {
            engine_id: "delay.reverse@1",
            name: "Reverse",
            style: DelayStyle::Reverse,
        },
        Style {
            engine_id: "delay.pitch@1",
            name: "Pitch",
            style: DelayStyle::Pitch,
        },
        Style {
            engine_id: "delay.rhythm@1",
            name: "Rhythm",
            style: DelayStyle::Rhythm,
        },
        Style {
            engine_id: "delay.drum@1",
            name: "Drum",
            style: DelayStyle::Drum,
        },
        Style {
            engine_id: "delay.oilcan@1",
            name: "Oil Can",
            style: DelayStyle::OilCan,
        },
        Style {
            engine_id: "delay.multitap@1",
            name: "Multi-Tap",
            style: DelayStyle::MultiTap,
        },
        Style {
            engine_id: "delay.spectral@1",
            name: "Spectral",
            style: DelayStyle::Spectral,
        },
        Style {
            engine_id: "delay.filter@1",
            name: "Filter",
            style: DelayStyle::Filter,
        },
        Style {
            engine_id: "delay.reverb@1",
            name: "Reverb Delay",
            style: DelayStyle::Reverb,
        },
    ];

    #[must_use]
    pub fn find(engine_id: &str) -> Option<&'static Style> {
        STYLES.iter().find(|s| s.engine_id == engine_id)
    }

    pub const TARGETS: &[ParamSpec] = &[
        ParamSpec::new("time", "Time", Unit::Ms, 1.0, 2000.0, 375.0).skewed(0.5),
        ParamSpec::new("feedback", "Feedback", Unit::Percent, 0.0, 0.98, 0.35),
        ParamSpec::new("mix", "Mix", Unit::Percent, 0.0, 1.0, 0.25),
        ParamSpec::new("high_cut", "High Cut", Unit::Hz, 1000.0, 20_000.0, 12_000.0).skewed(0.25),
        ParamSpec::new("low_cut", "Low Cut", Unit::Hz, 20.0, 1000.0, 60.0).skewed(0.25),
        ParamSpec::new("width", "Width", Unit::Percent, 0.0, 1.0, 1.0),
        ParamSpec::new("drive", "Drive", Unit::Percent, 0.0, 1.0, 0.2),
        ParamSpec::new("wow", "Wow", Unit::Percent, 0.0, 1.0, 0.1),
        ParamSpec::new("flutter", "Flutter", Unit::Percent, 0.0, 1.0, 0.1),
    ];

    pub struct DelayEngine {
        chain: DelayChain,
        config: AudioConfig,
        l: Vec<f64>,
        r: Vec<f64>,
    }

    impl DelayEngine {
        #[must_use]
        pub fn new(style: &Style, sample_rate: f32, max_block: usize) -> Self {
            let config = AudioConfig {
                sample_rate: f64::from(sample_rate),
                max_buffer_size: max_block,
            };
            let mut chain = DelayChain::new();
            chain.set_style(style.style);
            chain.update(config);
            Self {
                chain,
                config,
                l: vec![0.0; max_block],
                r: vec![0.0; max_block],
            }
        }
    }

    impl BlockProcessor for DelayEngine {
        fn params(&self) -> &'static [ParamSpec] {
            TARGETS
        }

        fn set_param(&mut self, index: usize, value: f32) {
            let v = f64::from(value);
            let c = &mut self.chain;
            let changed = match index {
                0 => set2(&mut c.delay_l.time_ms, &mut c.delay_r.time_ms, v),
                1 => set2(&mut c.delay_l.feedback, &mut c.delay_r.feedback, v),
                2 => set1(&mut c.mix, v),
                3 => set2(&mut c.delay_l.hicut_freq, &mut c.delay_r.hicut_freq, v),
                4 => set2(&mut c.delay_l.locut_freq, &mut c.delay_r.locut_freq, v),
                5 => set1(&mut c.width, v),
                6 => set2(&mut c.delay_l.drive, &mut c.delay_r.drive, v),
                7 => set2(&mut c.delay_l.wow_depth, &mut c.delay_r.wow_depth, v),
                8 => set2(
                    &mut c.delay_l.flutter_depth,
                    &mut c.delay_r.flutter_depth,
                    v,
                ),
                _ => false,
            };
            // `update` is what hands time, feedback, drive and the rest to
            // the style's inner engine (and redesigns its filters). It is
            // not free of side effects (smoother timing, the freeze ramp, the
            // diffusers), so only for what needs it and only on a real
            // change: mix and width are read per sample, through the
            // chain's own smoothers.
            if changed && !matches!(index, 2 | 5) {
                self.chain.update(self.config);
            }
        }

        fn reset(&mut self) {
            self.chain.reset();
        }

        fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
            let n = left.len().min(right.len()).min(self.l.len());
            let (Some(l), Some(r)) = (self.l.get_mut(..n), self.r.get_mut(..n)) else {
                return;
            };
            for ((d, s), (e, t)) in l
                .iter_mut()
                .zip(left.iter())
                .zip(r.iter_mut().zip(right.iter()))
            {
                *d = f64::from(*s);
                *e = f64::from(*t);
            }
            self.chain.process(l, r);
            for ((d, s), (e, t)) in left
                .iter_mut()
                .zip(l.iter())
                .zip(right.iter_mut().zip(r.iter()))
            {
                *d = narrow(*s);
                *e = narrow(*t);
            }
        }
    }

    fn set1(slot: &mut f64, v: f64) -> bool {
        if slot.to_bits() == v.to_bits() {
            return false;
        }
        *slot = v;
        true
    }

    fn set2(a: &mut f64, b: &mut f64, v: f64) -> bool {
        let changed = set1(a, v);
        set1(b, v) || changed
    }
}

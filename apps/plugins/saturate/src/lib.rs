//! FTS Saturate — CLAP/VST3 saturation plugin.
//!
//! Five circuits on a rail — Tube, Tape, Transformer, Transistor, Digital —
//! and nine profiles inside them. What each one *is* lives in
//! [`saturate_profiles`]: the per-side shaper pair, the operating point, the
//! sag and its ballistics, the skew that makes the even harmonics, and which
//! band meets the knee first. Nothing about a circuit lives in this file.
//!
//! That is deliberate and it is the point of the crate split. The plugin
//! shell used to carry its own `ModelParam` table — Preamp/Tube/Tape/
//! Transformer/Console/Fuzz — which named nearly the same circuits as the
//! rail did, mapped them to slightly different settings, and drifted from it.
//! Two vocabularies for one choice is a bug that ships as a sound. There is
//! one now, and [`dsp::saturate::profiles::apply`] is the only thing that knows how
//! a knob reaches the DSP — which is also why the editor's curve is the
//! engine's curve rather than a drawing of one.
//!
//! Engine: [`dsp::saturate::preamp::ClassAPreamp`] for the nonlinearity, plus
//! [`dsp::saturate::digital::DigitalStage`] on the wet path for the digital
//! family (there is no transfer curve that produces an alias, so quantisation
//! and rate reduction cannot be a shaper).

// `plugin_core::prelude` re-exports nice-plug's, plus the Dioxus editor
// pieces — one import rather than two that overlap.
use plugin_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dsp::saturate::bluebox::Bluebox;
use dsp::saturate::deluxe::Deluxe;
use dsp::saturate::digital::DigitalStage;
use dsp::saturate::emphasis::EmphasisEq;
use dsp::saturate::preamp::ClassAPreamp;
use dsp::saturate::ui::params::{SatParams, SatUiState};

const PLUGIN_NAME: &str = "FTS Saturate";

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsSaturate {
    params: Arc<SatParams>,
    ui_state: Arc<SatUiState>,
    editor_state: Arc<DioxusState>,
    /// One stage per channel: the preamp's sag envelope, DC blocker and tilt
    /// filters are all per-channel state, so the two sides must not share.
    pre: [ClassAPreamp; 2],
    /// …and one quantiser per channel, for the same reason (the hold is
    /// state) plus one more: a shared dither sequence would collapse into the
    /// middle of the stereo image instead of sitting behind it.
    quantiser: [DigitalStage; 2],
    /// The emphasis / de-emphasis EQ pair (`fx.sat.emphasis`): applied
    /// around the whole wet path, per channel like the preamps.
    emph: [EmphasisEq; 2],
    /// The measured models. Stereo engines (Deluxe's detector hears the
    /// sum of both channels), built for the session's rate in `activate`
    /// because their filters are designed there — never on the audio
    /// thread.
    deluxe: Deluxe,
    bluebox: Bluebox,
    /// The latency last reported to the host: Bluebox's oversampler delays
    /// its wet path (and its dry, to match); the other engines are
    /// zero-latency.
    latency: u32,
}

impl Default for FtsSaturate {
    fn default() -> Self {
        Self {
            params: Arc::new(SatParams::default()),
            ui_state: Arc::new(SatUiState::default()),
            // The editor sizes itself — see dsp::saturate::ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    dsp::saturate::ui::control_view::EDITOR_W,
                    dsp::saturate::ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(dsp::saturate::ui::control_view::resize_hint()),
            pre: [ClassAPreamp::new(48_000.0), ClassAPreamp::new(48_000.0)],
            quantiser: [DigitalStage::new(), DigitalStage::new()],
            emph: [EmphasisEq::new(48_000.0), EmphasisEq::new(48_000.0)],
            deluxe: Deluxe::new(48_000.0),
            bluebox: Bluebox::new(48_000.0),
            latency: 0,
        }
    }
}

impl FtsSaturate {
    /// The latency `engine` adds, in samples.
    fn latency_of(&self, engine: dsp::saturate::profiles::Engine) -> u32 {
        match engine {
            dsp::saturate::profiles::Engine::Bluebox { .. } => {
                u32::try_from(self.bluebox.latency()).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Point both channels at the resolved profile.
    ///
    /// Resolved through the *persisted id* rather than the automatable index,
    /// so growing the rail never repoints an old session at a different
    /// circuit — same contract the reverb and the compressor keep.
    fn sync_params(&mut self) -> (bool, dsp::saturate::profiles::Engine) {
        let profile = self.params.resolved_profile();
        let controls = dsp::saturate::profiles::Controls {
            drive: self.params.drive.value(),
            bias: self.params.bias.value(),
            sag: self.params.sag.value(),
            tilt: self.params.tilt.value(),
            character_a: self.params.character_a.value(),
            character_b: self.params.character_b.value(),
            mix: self.params.mix.value(),
        };
        // The emphasis bands, designed once per block (setters early-out on
        // unchanged values inside the biquad design's own cheapness — the
        // whole table is a handful of arithmetic).
        let bands = std::array::from_fn(|i| self.params.emph[i].to_band());
        for emph in &mut self.emph {
            if *emph.bands() != bands {
                emph.set_bands(&bands);
            }
        }
        // The makeup calibrates at the level the shaper actually sees — the
        // emphasis raises it exactly like the tilt does
        // (`fx.sat.emphasis.makeup`). Before `apply`, which refreshes makeup.
        let sigma_gain = self.emph[0].sigma_gain();
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.set_emphasis_sigma_gain(sigma_gain);
            dsp::saturate::profiles::apply(profile, &controls, pre, quantiser);
        }
        // The measured models take the same knobs through their own mapping.
        match profile.engine {
            dsp::saturate::profiles::Engine::Deluxe => self
                .deluxe
                .set(dsp::saturate::profiles::deluxe_params(&controls)),
            dsp::saturate::profiles::Engine::Bluebox { sat_freq, alt_tube } => {
                self.bluebox.set(dsp::saturate::profiles::bluebox_params(
                    &controls, sat_freq, alt_tube,
                ));
            }
            dsp::saturate::profiles::Engine::Preamp => {}
        }
        (
            profile.voicing.digital && !self.quantiser[0].is_transparent(),
            profile.engine,
        )
    }
}

impl Plugin for FtsSaturate {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Audio effect: stereo in, stereo out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type Editor = plugin_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(dsp::saturate::ui::control_view::SatUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            dsp::saturate::ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.set_sample_rate(buffer_config.sample_rate);
            pre.reset();
            quantiser.reset();
        }
        for emph in &mut self.emph {
            emph.set_sample_rate(buffer_config.sample_rate);
        }
        self.deluxe = Deluxe::new(f64::from(buffer_config.sample_rate));
        self.bluebox = Bluebox::new(f64::from(buffer_config.sample_rate));
        // Land the settings before the first block, so the tilt filters and
        // the sag ballistics start on the real values rather than ramping
        // out of a default nobody chose.
        let (_, engine) = self.sync_params();
        self.latency = self.latency_of(engine);
        context.set_latency_samples(self.latency);
        true
    }

    fn reset(&mut self) {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.reset();
            quantiser.reset();
        }
        for emph in &mut self.emph {
            emph.reset();
        }
        self.deluxe.reset();
        self.bluebox.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let (crushing, engine) = self.sync_params();
        let latency = self.latency_of(engine);
        if latency != self.latency {
            self.latency = latency;
            context.set_latency_samples(latency);
        }
        let mix = self.params.mix.value();
        let output = 10.0f32.powf(self.params.output.value() / 20.0);

        let mut input_peak = 0.0f32;
        let mut output_peak = 0.0f32;

        if engine != dsp::saturate::profiles::Engine::Preamp {
            // A measured model is a whole stereo algorithm with its own mix
            // law (handed over in `sync_params`); the emphasis pair still
            // wraps what it distorts, and Output stays ours.
            for mut frame in buffer.iter_samples() {
                let mut io = [0.0f32; 2];
                for ((sample, slot), emph) in frame
                    .iter_mut()
                    .zip(io.iter_mut())
                    .zip(self.emph.iter_mut())
                {
                    input_peak = input_peak.max(sample.abs());
                    *slot = emph.pre(*sample);
                }
                let [l, r] = io;
                let wet_pair: [f64; 2] = match engine {
                    dsp::saturate::profiles::Engine::Deluxe => {
                        self.deluxe.process(f64::from(l), f64::from(r))
                    }
                    _ => self.bluebox.process(f64::from(l), f64::from(r)),
                }
                .into();
                for ((sample, wet), emph) in
                    frame.iter_mut().zip(wet_pair).zip(self.emph.iter_mut())
                {
                    let out = emph.post(to_f32(wet)) * output;
                    output_peak = output_peak.max(out.abs());
                    *sample = out;
                }
            }
            store_peak(&self.ui_state.input_db, input_peak);
            store_peak(&self.ui_state.out_db, output_peak);
            return ProcessStatus::Normal;
        }

        for mut frame in buffer.iter_samples() {
            for (c, sample) in frame.iter_mut().enumerate() {
                let ch = c.min(1);
                let dry = *sample;
                input_peak = input_peak.max(dry.abs());

                // Emphasis in, the stage, the quantiser, de-emphasis out,
                // and only THEN the mix (`fx.sat.emphasis`): the EQ pair
                // wraps the whole wet path so it shapes what distorts —
                // including what the bitcrusher chews on — and is net-flat
                // around it. A flat EQ short-circuits to identity, keeping
                // the default path bit-exact.
                let emphasized = self.emph[ch].pre(dry);
                let mut wet = self.pre[ch].process_wet(ch, emphasized);
                if crushing {
                    wet = self.quantiser[ch].process(ch, wet);
                }
                wet = self.emph[ch].post(wet);
                let out = (wet - dry).mul_add(mix, dry) * output;
                output_peak = output_peak.max(out.abs());
                *sample = out;
            }
        }

        // Meter ballistics: instant attack, a slow fall the editor can
        // read at whatever rate it happens to draw at.
        store_peak(&self.ui_state.input_db, input_peak);
        store_peak(&self.ui_state.out_db, output_peak);

        ProcessStatus::Normal
    }
}

/// The measured models run in f64; the host's buffer is f32.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "narrowing to the host's f32 buffer is the point"
)]
const fn to_f32(x: f64) -> f32 {
    x as f32
}

/// Peak-hold in dB, falling 0.3 dB a block. The editor samples this when it
/// draws; a frame that misses an update simply draws the next one.
fn store_peak(slot: &atomic_float::AtomicF32, peak: f32) {
    let db = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        -100.0
    };
    let previous = slot.load(Ordering::Relaxed);
    slot.store(
        if db > previous { db } else { previous - 0.3 },
        Ordering::Relaxed,
    );
}

impl ClapPlugin for FtsSaturate {
    const CLAP_ID: &'static str = "com.fasttrackstudio.saturate";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Five circuits: tube, tape, transformer, transistor, and digital");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsSaturate {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsSaturatePlg01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

#[cfg(feature = "export")]
nice_export_clap!(FtsSaturate);
#[cfg(feature = "export")]
nice_export_vst3!(FtsSaturate);

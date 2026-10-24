//! FTS Modulation — CLAP/VST3 multi-mode modulation plugin.
//!
//! Five circuits on a rail — Chorus, Flanger, Vibrato, Tremolo, Wah — and
//! the profiles inside them. What each one *is* lives in
//! [`modulation_profiles`]: which of the three chains it runs, which engine,
//! and where its own controls rest. Nothing about a circuit lives in this
//! file.
//!
//! That is the point of the crate split, and it is worth saying what it
//! fixed. This shell used to carry a five-value `Mode` enum, hardcode
//! `EngineType::Cubic`, and never call `set_engine` — its own doc note said
//! so. Four of the five chorus engines (BBD, Tape, Orbit, Juno) had no way to
//! be heard at all; the tremolo's groove, feel, accent and saturation and the
//! wah's resonance, stages and sensitivity were fields nothing wrote. Seven
//! parameters over about twenty-five real controls.
//!
//! [`dsp::modulation::profiles::apply`] is the only thing that knows how a knob
//! reaches the DSP — which is also why the editor's shape is the engine's own
//! movement rather than a drawing of one.

use plugin_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dsp::modulation::chorus::chain::ChorusChain;
use dsp::modulation::profiles::Chain;
use dsp::modulation::trem::chain::TremChain;
use dsp::modulation::ui::params::{ModParams, ModUiState};
use dsp::modulation::wah::chain::WahChain;
use dsp_core::audio::{AudioConfig, Processor};
use dsp_core::modulation::tempo::TransportInfo;

const PLUGIN_NAME: &str = "FTS Modulation";

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsModulation {
    params: Arc<ModParams>,
    ui_state: Arc<ModUiState>,
    editor_state: Arc<DioxusState>,
    /// All three chains live for the life of the plugin: switching families
    /// is a dispatch, not a rebuild, and coming back to one finds it as you
    /// left it.
    chorus: ChorusChain,
    trem: TremChain,
    wah: WahChain,
    /// Which chain the last block ran, so a family switch resets the incoming
    /// one instead of resuming from stale state.
    current: Chain,
    sample_rate: f64,
    max_buffer_size: usize,
}

impl Default for FtsModulation {
    fn default() -> Self {
        // The modulators' clocks are set by `dsp::modulation::profiles::apply`, not
        // here — a chain configured in two places is how the editor ended up
        // drawing a still line for the tremolo while the plugin ran fine.
        Self {
            params: Arc::new(ModParams::default()),
            ui_state: Arc::new(ModUiState::default()),
            // The editor sizes itself — see dsp::modulation::ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    dsp::modulation::ui::control_view::EDITOR_W,
                    dsp::modulation::ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(dsp::modulation::ui::control_view::resize_hint()),
            chorus: ChorusChain::new(),
            trem: TremChain::new(),
            wah: WahChain::new(),
            current: Chain::Delay,
            sample_rate: 48_000.0,
            max_buffer_size: 512,
        }
    }
}

impl FtsModulation {
    /// Point the engines at the resolved profile.
    ///
    /// Resolved through the *persisted id* rather than the automatable index,
    /// so growing the rail never repoints an old session at a different
    /// circuit — same contract the reverb and the saturator keep.
    ///
    /// Returns the chain to run this block.
    fn sync_params(&mut self) -> Chain {
        let profile = self.params.resolved_profile();
        let chain = profile.voicing.circuit.chain();

        // Reset the incoming chain on a family switch so it starts clean
        // rather than resuming a half-finished sweep.
        if chain != self.current {
            self.current = chain;
            match chain {
                Chain::Delay => self.chorus.reset(),
                Chain::Tremolo => self.trem.reset(),
                Chain::Wah => self.wah.reset(),
            }
        }

        dsp::modulation::profiles::apply(
            profile,
            &self.params.controls(),
            &mut self.chorus,
            &mut self.trem,
            &mut self.wah,
        );
        chain
    }
}

impl Plugin for FtsModulation {
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
            Arc::new(dsp::modulation::ui::control_view::ModUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            dsp::modulation::ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = f64::from(buffer_config.sample_rate);
        self.max_buffer_size = buffer_config.max_buffer_size as usize;
        let config = AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: self.max_buffer_size,
        };
        // Land the profile before the reconfigure, so each chain retunes onto
        // the real settings rather than ramping out of a default nobody chose.
        self.sync_params();
        self.chorus.update(config);
        self.trem.update(config);
        self.wah.update(config);
        true
    }

    fn reset(&mut self) {
        self.chorus.reset();
        self.trem.reset();
        self.wah.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if buffer.channels() < 2 {
            return ProcessStatus::Normal;
        }
        let chain = self.sync_params();
        // The bar line a synced modulator locks to, and what Groove and Feel
        // push and pull against. A host with no tempo reads as stopped: a
        // synced chain then stands still rather than guessing a tempo.
        let t = context.transport();
        let transport = TransportInfo {
            position_qn: t.pos_beats().unwrap_or(0.0),
            tempo_bpm: t.tempo.unwrap_or(0.0),
            playing: t.playing && t.tempo.is_some(),
        };
        self.trem.set_transport(transport);
        self.wah.set_transport(transport);
        self.ui_state
            .tempo_bpm
            .store(t.tempo.map_or(0.0, |bpm| bpm as f32), Ordering::Relaxed);
        let output = 10.0f32.powf(self.params.output.value() / 20.0);

        let mut input_peak = 0.0f32;
        let mut output_peak = 0.0f32;

        // Fixed stack chunks, f32 ↔ f64. No allocation on the audio thread.
        const CHUNK: usize = 128;
        let total = buffer.samples();
        let mut offset = 0;

        while offset < total {
            let len = (offset + CHUNK).min(total) - offset;
            let mut left = [0.0f64; CHUNK];
            let mut right = [0.0f64; CHUNK];

            {
                let channels = buffer.as_slice();
                for i in 0..len {
                    let (l, r) = (channels[0][offset + i], channels[1][offset + i]);
                    input_peak = input_peak.max(l.abs().max(r.abs()));
                    left[i] = f64::from(l);
                    right[i] = f64::from(r);
                }
            }

            match chain {
                Chain::Delay => self.chorus.process(&mut left[..len], &mut right[..len]),
                Chain::Tremolo => self.trem.process(&mut left[..len], &mut right[..len]),
                Chain::Wah => self.wah.process(&mut left[..len], &mut right[..len]),
            }

            {
                let channels = buffer.as_slice();
                for i in 0..len {
                    let (l, r) = (left[i] as f32 * output, right[i] as f32 * output);
                    output_peak = output_peak.max(l.abs().max(r.abs()));
                    channels[0][offset + i] = l;
                    channels[1][offset + i] = r;
                }
            }

            offset += len;
        }

        store_peak(&self.ui_state.input_db, input_peak);
        store_peak(&self.ui_state.out_db, output_peak);
        // Where the modulator is sitting, for the panel's playhead.
        let modulated = match chain {
            Chain::Tremolo => self.trem.modulator.output(),
            Chain::Wah => self.wah.modulator.output(),
            Chain::Delay => 0.0,
        };
        self.ui_state
            .mod_value
            .store(modulated as f32, Ordering::Relaxed);

        ProcessStatus::Normal
    }
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

impl ClapPlugin for FtsModulation {
    const CLAP_ID: &'static str = "com.fasttrackstudio.modulation";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Five circuits: chorus, flanger, vibrato, tremolo, and wah");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Chorus,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsModulation {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsModPlugin0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

#[cfg(feature = "export")]
nice_export_clap!(FtsModulation);
#[cfg(feature = "export")]
nice_export_vst3!(FtsModulation);

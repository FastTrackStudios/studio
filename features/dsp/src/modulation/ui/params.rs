//! The modulator's parameters, and the two pieces of editor state a session
//! has to remember.
//!
//! Lives here rather than in the plugin shell for the same reason the
//! saturator's do: the editor is what constrains them. The profile is a
//! parameter so a host can automate the space; the profile *id* is persisted
//! state so growing the list cannot repoint an old project at a different
//! circuit.

use std::sync::Arc;

use atomic_float::AtomicF32;
use nice_plug::prelude::*;

/// Live values the editor reads and the audio thread writes.
///
/// Plain atomics rather than a channel: the editor samples them when it
/// draws, and a frame that misses an update simply draws the next one.
#[derive(Default)]
pub struct ModUiState {
    /// Output level in dB, for the meter.
    pub out_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
    /// Where the modulator is in its cycle, 0..1 — the panel draws a
    /// playhead on the shape with it. Written every block, read whenever the
    /// editor happens to draw; nothing is synchronised because nothing needs
    /// to be.
    pub phase: AtomicF32,
    /// The modulator's current output, 0..1. On a wah this is where the
    /// filter is sitting, which no number on a panel has ever conveyed.
    pub mod_value: AtomicF32,
    /// The host's tempo, as the audio thread last saw it. Zero means the
    /// host has no transport — the Sync switch will not lock to one then.
    pub tempo_bpm: AtomicF32,
}

/// **Noon is the circuit as designed.**
///
/// The four circuit knobs are *trims* around the active profile's voicing
/// rather than absolute values: 0.5 means "a Juno, as a Juno is", and turning
/// one is always a statement about this circuit. That is what lets one set of
/// parameters serve every modulator without a knob meaning something
/// different on each — see [`crate::modulation::profiles::Controls`], where they land.
///
/// `rate`, `depth` and `mix` are absolute, because they mean the same thing
/// on every modulator ever built.
#[derive(Params)]
pub struct ModParams {
    /// Which of the five families' profiles is active, as an index into
    /// [`crate::modulation::profiles::PROFILES`]. A parameter because the space is
    /// worth automating; see [`Self::profile_id`] for what a session saves.
    #[id = "profile"]
    pub profile: IntParam,

    /// How fast it moves. On every one of the five, which is why it is a
    /// control and not a family. Exponential — see
    /// [`crate::modulation::profiles::rate_hz_from`].
    #[id = "rate"]
    pub rate: FloatParam,

    /// How far it moves.
    #[id = "depth"]
    pub depth: FloatParam,

    /// Dry/wet. Vibrato has no dry path at all, and its panel does not offer
    /// this rather than offering one that does nothing.
    #[id = "mix"]
    pub mix: FloatParam,

    /// Output trim.
    #[id = "output"]
    pub output: FloatParam,

    /// The circuit's own four controls; the panel names them per profile.
    #[id = "knoba"]
    pub knob_a: FloatParam,
    #[id = "knobb"]
    pub knob_b: FloatParam,
    #[id = "knobc"]
    pub knob_c: FloatParam,
    #[id = "knobd"]
    pub knob_d: FloatParam,

    /// Lock the modulator to the host's tempo: Rate stops being Hz and the
    /// cycle lasts [`Self::rate_div`]. Appended, so an old session opens
    /// free, as it was saved.
    #[id = "rate_sync"]
    pub rate_sync: BoolParam,

    /// The note value one cycle lasts while [`Self::rate_sync`] is on.
    #[id = "rate_div"]
    pub rate_div: IntParam,

    /// What a session restores from.
    ///
    /// The index is not stable: adding a family renumbers everything after
    /// it, and a project saved last year would open on whatever now sits at
    /// that number. The id is stable, so it is what gets written down.
    #[persist = "profile_id"]
    pub profile_id: parking_lot::RwLock<String>,

    /// The editor's form factor, persisted by id.
    #[persist = "editor_form"]
    pub editor_form: parking_lot::RwLock<String>,
}

impl Default for ModParams {
    fn default() -> Self {
        Self {
            profile: IntParam::new(
                "Circuit",
                crate::modulation::profiles::profile_index("juno").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (crate::modulation::profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                crate::modulation::profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map_or_else(|| "—".to_string(), |p| p.name.to_string())
            })),

            // Shown in Hz, stored normalised: the taper belongs to the
            // profiles crate so the editor and the audio thread cannot
            // disagree about what 0.5 means.
            rate: FloatParam::new("Rate", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    format!("{:.2} Hz", crate::modulation::profiles::rate_hz_from(v))
                }))
                .with_string_to_value(Arc::new(|s| {
                    s.trim()
                        .trim_end_matches("Hz")
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .map(crate::modulation::profiles::rate_knob_from)
                })),

            depth: FloatParam::new("Depth", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            output: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            knob_a: knob("Knob A"),
            knob_b: knob("Knob B"),
            knob_c: knob("Knob C"),
            knob_d: knob("Knob D"),

            rate_sync: dsp_core::time::params::sync_param("Rate Sync", false),
            rate_div: dsp_core::time::params::division_param(
                "Rate Div",
                dsp_core::time::MusicalTime::new(
                    dsp_core::time::NoteValue::Quarter,
                    dsp_core::time::Flavour::Straight,
                ),
            ),

            profile_id: parking_lot::RwLock::new(String::new()),
            editor_form: parking_lot::RwLock::new(String::new()),
        }
    }
}

/// A circuit knob: centred, because noon is the voicing.
fn knob(name: &str) -> FloatParam {
    FloatParam::new(name, 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_value_to_string(formatters::v2s_f32_percentage(0))
}

impl ModParams {
    /// The profile index the editor should be showing.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        crate::modulation::profiles::profile_index(&id).unwrap_or_else(|| {
            (self.profile.value().max(0) as usize)
                .min(crate::modulation::profiles::PROFILES.len() - 1)
        })
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static crate::modulation::profiles::Profile {
        &crate::modulation::profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = crate::modulation::profiles::PROFILES
            .get(index)
            .map_or("juno", |p| p.id);
        *self.profile_id.write() = id.to_string();
    }

    /// Everything the mapping needs, in one struct.
    pub fn controls(&self) -> crate::modulation::profiles::Controls {
        crate::modulation::profiles::Controls {
            rate: self.rate.value(),
            depth: self.depth.value(),
            mix: self.mix.value(),
            knobs: [
                self.knob_a.value(),
                self.knob_b.value(),
                self.knob_c.value(),
                self.knob_d.value(),
            ],
            clock: if self.rate_sync.value() {
                crate::modulation::profiles::Clock::Synced(dsp_core::time::MusicalTime::from_param(
                    self.rate_div.value(),
                ))
            } else {
                crate::modulation::profiles::Clock::Free
            },
        }
    }

    pub fn resolved_editor_form(&self) -> fts_audio_ui::EditorForm {
        fts_audio_ui::EditorForm::from_id(&self.editor_form.read()).unwrap_or_default()
    }

    pub fn store_editor_form(&self, form: fts_audio_ui::EditorForm) {
        *self.editor_form.write() = form.id().to_string();
    }
}

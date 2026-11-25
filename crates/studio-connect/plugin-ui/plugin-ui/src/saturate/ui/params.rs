//! The saturator's parameters, and the pieces of editor state a session has
//! to remember.
//!
//! Lives here rather than in the plugin shell for the same reason the
//! compressor's do: the editor is what constrains them. The profile is a
//! parameter so a host can automate the space; the profile *id* is persisted
//! state so growing the list cannot repoint an old project at a different
//! circuit.
//!
//! The tree is an FX stack (`fx.stack.params`, docs/spec/fx/stack.md): a pool
//! of [`MAX_STAGES`] complete saturators ([`SatStageParams`]), each with its
//! own circuit, knobs and emphasis EQ, plus the lanes and the stack globals.
//! Stage 1 is nested bare, so its ids and persist keys are the pre-stack
//! plugin's and an old session loads into it unchanged; stage *n* is
//! prefixed `s{n}_`.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use atomic_float::AtomicF32;
use nice_plug::prelude::*;

/// Live values the editor reads and the audio thread writes, plus the
/// editor's side of IR loading.
///
/// The meters are plain atomics rather than a channel: the editor samples them
/// when it draws, and a frame that misses an update simply draws the next one.
#[derive(Default)]
pub struct SatUiState {
    /// Output level in dB, for the meter.
    pub out_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
    /// The stage the editor is focused on (`fx.stack.focus`), written by
    /// the editor. The two meters above are the stack's edges — what goes
    /// in and what comes out — so they do not follow it; anything that
    /// meters a stage's own state reads it from here.
    pub focused_stage: AtomicUsize,
}

/// How many stages the saturator stack can hold (`fx.stack.limits`).
pub const MAX_STAGES: usize = 8;

/// **Noon is the circuit as designed.**
///
/// Every control here except `drive`, `mix` and `output` is a *trim* around
/// the active profile's voicing rather than an absolute value: 0.5 means "a
/// Triode, as a Triode is", and turning a knob is always a statement about
/// this circuit. That is what lets one set of nine parameters serve nine
/// machines without a knob meaning something different on each — see
/// [`crate::saturate::profiles::Controls`], which is where they land.
///
/// One stage of the stack (`fx.stack.model`): everything a single-circuit
/// saturator has — the circuit, its knobs, its emphasis EQ — then its place
/// in the stack. The order up to `emph` is the pre-stack plugin's and must
/// not change.
#[derive(Params)]
pub struct SatStageParams {
    /// Which of the seven families' profiles is active, as an index into
    /// [`crate::saturate::profiles::PROFILES`]. A parameter because the space is worth
    /// automating; see [`Self::profile_id`] for what a session actually saves.
    #[id = "profile"]
    pub profile: IntParam,

    /// How hard the signal is pushed into the circuit. On every one of the
    /// five, which is why it is a control and not a family.
    #[id = "drive"]
    pub drive: FloatParam,

    /// Where the circuit is biased. On an asymmetric stage this is what
    /// decides how much of the even-harmonic content you get; on a symmetric
    /// one it does almost nothing, and the panel only offers it where it
    /// means something.
    #[id = "q_point"]
    pub bias: FloatParam,

    /// Supply sag: the circuit ducking under a transient and recovering.
    #[id = "sag"]
    pub sag: FloatParam,

    /// Pre-emphasis into the stage — drive the top end harder than the
    /// bottom, which is how a transformer is usually flattered.
    #[id = "tilt"]
    pub tilt: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    #[id = "output"]
    pub output: FloatParam,

    /// The circuit's own two controls; the panel names them per profile.
    #[id = "chara"]
    pub character_a: FloatParam,

    #[id = "charb"]
    pub character_b: FloatParam,

    /// What a session restores from.
    ///
    /// The index is not stable: adding a family, or another plate, renumbers
    /// everything after it, and a project saved last year would open on
    /// whatever now sits at that number. The id is stable, so it is what gets
    /// written down. Same contract as the compressor's.
    #[persist = "profile_id"]
    pub profile_id: parking_lot::RwLock<String>,

    // ── Appended (never reorder anything above) ────────────────────────
    /// The emphasis / de-emphasis EQ (`fx.sat.emphasis`,
    /// docs/spec/fx/embedded-eq.md): six bands applied before the stage and
    /// exactly inverted after it, so the curve chooses what distorts. Ids
    /// come out `eshape_1`…`eq_6`.
    #[nested(array, group = "Emphasis")]
    pub emph: [EmphBandParams; crate::saturate::emphasis::BANDS],

    /// Where this stage sits in the stack (`use`, `on`, `lane`). Last, so
    /// the ids above keep their pre-stack order.
    #[nested]
    pub place: crate::stack::host::StagePlacement,
}

/// The full parameter tree: the stage pool, the lanes, and the stack
/// globals (`fx.stack.params`).
///
/// Stage 1 is nested bare so its ids are the pre-stack plugin's, unchanged;
/// stages 2–8 are prefixed `s2_`…`s8_`.
#[derive(Params)]
pub struct SatParams {
    #[nested]
    pub stage1: SatStageParams,
    #[nested(id_prefix = "s2", group = "Stage 2")]
    pub stage2: SatStageParams,
    #[nested(id_prefix = "s3", group = "Stage 3")]
    pub stage3: SatStageParams,
    #[nested(id_prefix = "s4", group = "Stage 4")]
    pub stage4: SatStageParams,
    #[nested(id_prefix = "s5", group = "Stage 5")]
    pub stage5: SatStageParams,
    #[nested(id_prefix = "s6", group = "Stage 6")]
    pub stage6: SatStageParams,
    #[nested(id_prefix = "s7", group = "Stage 7")]
    pub stage7: SatStageParams,
    #[nested(id_prefix = "s8", group = "Stage 8")]
    pub stage8: SatStageParams,

    /// Per-lane gain / mute / solo, ids `lgain_1`…`lsolo_8`.
    #[nested(array, group = "Lane")]
    pub lanes: [crate::stack::host::LaneParams; MAX_STAGES],

    /// Parallel sum law (`fx.stack.sum`).
    #[id = "summode"]
    pub sum_mode: IntParam,
    /// Stack output trim.
    #[id = "stktrim"]
    pub output_trim_db: FloatParam,

    /// The editor's form factor, persisted by id. One window, so one form —
    /// not per stage.
    #[persist = "editor_form"]
    pub editor_form: parking_lot::RwLock<String>,

    /// Which stage the editor is focused on (`fx.stack.focus`), 0-based —
    /// UI state, persisted so the editor reopens where it was left.
    #[persist = "focused_stage"]
    pub focused_stage: parking_lot::RwLock<String>,
}

/// One emphasis band (`fx.embed-eq.band-params`). Bell / Low Shelf / High
/// Shelf only — cut shapes have no inverse (`fx.sat.emphasis.mirror`). A
/// band at 0 dB is idle and costs nothing.
#[derive(Params)]
pub struct EmphBandParams {
    #[id = "eshape"]
    pub shape: IntParam,
    #[id = "efreq"]
    pub freq_hz: FloatParam,
    #[id = "egain"]
    pub gain_db: FloatParam,
    #[id = "eq"]
    pub q: FloatParam,
}

/// Emphasis shape labels, in `eshape` value order
/// (`crate::saturate::emphasis::EmphShape`).
pub const EMPH_SHAPE_LABELS: &[&str] = &["Bell", "Low Shelf", "High Shelf"];

impl EmphBandParams {
    fn new(default_freq: f32) -> Self {
        Self {
            shape: IntParam::new("Emph Shape", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| {
                    EMPH_SHAPE_LABELS
                        .get(v.max(0) as usize)
                        .map_or_else(|| v.to_string(), std::string::ToString::to_string)
                })),
            freq_hz: FloatParam::new(
                "Emph Freq",
                default_freq,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            // No `.with_unit(" Hz")` here: `v2s_f32_hz_then_khz` writes the
            // unit itself, and switches it to kHz above 1000. Adding one on
            // top printed "1.3 kHz Hz".
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            gain_db: FloatParam::new(
                "Emph Gain",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            q: FloatParam::new(
                "Emph Q",
                0.707,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 18.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }

    /// This band as the DSP sees it.
    pub fn to_band(&self) -> crate::saturate::emphasis::EmphBand {
        crate::saturate::emphasis::EmphBand {
            shape: crate::saturate::emphasis::EmphShape::from_index(
                self.shape.value().max(0) as u32
            ),
            freq_hz: self.freq_hz.value(),
            gain_db: self.gain_db.value(),
            q: self.q.value(),
        }
    }
}

/// The default emphasis band frequencies — a useful spread, all at 0 dB
/// (idle) until moved.
pub const EMPH_DEFAULT_FREQS: [f32; crate::saturate::emphasis::BANDS] =
    [80.0, 250.0, 700.0, 1_800.0, 4_500.0, 10_000.0];

impl Default for SatStageParams {
    fn default() -> Self {
        Self::new(true)
    }
}

impl SatStageParams {
    /// A stage's params. `first` = stage 1, which starts in the stack (the
    /// plain plugin); the rest start unused until added (`fx.stack.add`).
    #[must_use]
    pub fn new(first: bool) -> Self {
        Self {
            profile: IntParam::new(
                "Space",
                crate::saturate::profiles::profile_index("triode").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (crate::saturate::profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                crate::saturate::profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map_or_else(|| "—".to_string(), |p| p.name.to_string())
            })),

            drive: FloatParam::new("Drive", 0.25, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            bias: FloatParam::new("Bias", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            // Noon, like every other trim on the panel — see the type's note.
            sag: FloatParam::new("Sag", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            tilt: FloatParam::new("Tilt", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

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

            character_a: FloatParam::new(
                "Character A",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            character_b: FloatParam::new(
                "Character B",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            profile_id: parking_lot::RwLock::new(String::new()),
            emph: std::array::from_fn(|i| EmphBandParams::new(EMPH_DEFAULT_FREQS[i])),
            place: crate::stack::host::StagePlacement::new(first, MAX_STAGES),
        }
    }

    /// This stage's profile index.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        crate::saturate::profiles::profile_index(&id).unwrap_or_else(|| {
            (self.profile.value().max(0) as usize)
                .min(crate::saturate::profiles::PROFILES.len() - 1)
        })
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static crate::saturate::profiles::Profile {
        &crate::saturate::profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = crate::saturate::profiles::PROFILES
            .get(index)
            .map_or("triode", |p| p.id);
        *self.profile_id.write() = id.to_string();
    }
}

impl Default for SatParams {
    fn default() -> Self {
        Self {
            stage1: SatStageParams::new(true),
            stage2: SatStageParams::new(false),
            stage3: SatStageParams::new(false),
            stage4: SatStageParams::new(false),
            stage5: SatStageParams::new(false),
            stage6: SatStageParams::new(false),
            stage7: SatStageParams::new(false),
            stage8: SatStageParams::new(false),
            lanes: std::array::from_fn(|_| crate::stack::host::LaneParams::default()),
            sum_mode: crate::stack::host::sum_mode_param(),
            output_trim_db: crate::stack::host::output_trim_param(),
            editor_form: parking_lot::RwLock::new(String::new()),
            focused_stage: parking_lot::RwLock::new(String::new()),
        }
    }
}

impl SatParams {
    /// Stage `i` (0-based).
    ///
    /// # Panics
    ///
    /// Panics if `i >= MAX_STAGES`.
    #[must_use]
    pub fn stage(&self, i: usize) -> &SatStageParams {
        match i {
            0 => &self.stage1,
            1 => &self.stage2,
            2 => &self.stage3,
            3 => &self.stage4,
            4 => &self.stage5,
            5 => &self.stage6,
            6 => &self.stage7,
            7 => &self.stage8,
            _ => panic!("stage {i} out of range"),
        }
    }

    /// All stages, in pool order.
    pub fn stages(&self) -> impl Iterator<Item = &SatStageParams> {
        (0..MAX_STAGES).map(|i| self.stage(i))
    }

    pub fn resolved_editor_form(&self) -> fts_audio_ui::EditorForm {
        fts_audio_ui::EditorForm::from_id(&self.editor_form.read()).unwrap_or_default()
    }

    pub fn store_editor_form(&self, form: fts_audio_ui::EditorForm) {
        *self.editor_form.write() = form.id().to_string();
    }
}

impl crate::stack::host::StackHost for SatParams {
    fn max_stages(&self) -> usize {
        MAX_STAGES
    }

    fn placement(&self, stage: usize) -> &crate::stack::host::StagePlacement {
        &self.stage(stage).place
    }

    fn lane(&self, lane: usize) -> &crate::stack::host::LaneParams {
        &self.lanes[lane]
    }

    fn sum_mode(&self) -> &IntParam {
        &self.sum_mode
    }

    fn output_trim_db(&self) -> &FloatParam {
        &self.output_trim_db
    }

    fn focused_cell(&self) -> &parking_lot::RwLock<String> {
        &self.focused_stage
    }

    fn style_param(&self, stage: usize) -> &IntParam {
        &self.stage(stage).profile
    }

    fn resolved_style(&self, stage: usize) -> usize {
        self.stage(stage).resolved_profile_index()
    }

    fn store_style_id(&self, stage: usize, index: usize) {
        self.stage(stage).store_profile_id(index);
    }

    fn style_badge(&self, index: usize) -> String {
        crate::saturate::profiles::PROFILES.get(index).map_or_else(
            || "SAT".to_string(),
            |p| crate::saturate::ui::control_view::profile_badge(p.id),
        )
    }

    fn style_name(&self, index: usize) -> String {
        crate::saturate::profiles::PROFILES
            .get(index)
            .map_or_else(|| "—".to_string(), |p| p.name.to_string())
    }
}

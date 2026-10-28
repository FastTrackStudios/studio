//! The delay's parameters, and the pieces of editor state a session has to
//! remember.
//!
//! Lives here rather than in the plugin shell for the same reason the
//! compressor's do: the editor is what constrains them. The profile is a
//! parameter so a host can automate the space; the profile *id* is persisted
//! state so growing the list cannot repoint an old project at a different
//! delay.
//!
//! The plugin is a **stack** (docs/spec/fx/stack.md): [`DelayParams`] holds
//! [`MAX_STAGES`] complete delays ([`DelayStageParams`]), each with its own
//! profile, time, sync, feedback, tone and mix — every vocal delay a mix
//! wants in one instance, routed serially or in parallel lanes. Stage 1 is
//! nested bare, so its ids and persist keys are the pre-stack plugin's and
//! an old session opens unchanged; stages 2–8 are prefixed `s2_`…`s8_`.

use std::sync::Arc;

use atomic_float::AtomicF32;
use nice_plug::prelude::*;

/// Live values the editor reads and the audio thread writes, plus the
/// editor's side of IR loading.
///
/// The meters are plain atomics rather than a channel: the editor samples them
/// when it draws, and a frame that misses an update simply draws the next one.
#[derive(Default)]
pub struct DelayUiState {
    /// Wet output level in dB, for the repeat display.
    pub wet_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
    /// The host's tempo, as the audio thread last saw it. Zero means the
    /// host has no transport — the editor shows the free-running time then,
    /// because a note value with no tempo behind it is not a duration.
    pub tempo_bpm: AtomicF32,
}

/// How many delays one instance can stack (`fx.stack.limits`).
pub const MAX_STAGES: usize = 8;

/// One complete delay — the full parameter set of a single-style instance
/// (`fx.stack.model`).
///
/// [`DelayParams`] holds [`MAX_STAGES`] of these. Fields **must keep their
/// order and ids** — hosts persist VST3 state by index, and stage 1's ids are
/// the pre-stack plugin's.
#[derive(Params)]
pub struct DelayStageParams {
    /// Which of the seven families' profiles is active, as an index into
    /// [`crate::delay::profiles::PROFILES`]. A parameter because the space is worth
    /// automating; see [`Self::profile_id`] for what a session actually saves.
    #[id = "profile"]
    pub profile: IntParam,

    /// Left delay time. The right side follows it when `link` is on.
    #[id = "time_l"]
    pub time_l: FloatParam,

    #[id = "time_r"]
    pub time_r: FloatParam,

    /// Read the times as note values against the host's tempo instead of as
    /// milliseconds. The free-running times are kept either way, so turning
    /// this off returns you to the number you dialled.
    #[id = "time_sync"]
    pub time_sync: BoolParam,

    /// Which note the left time locks to while `time_sync` is on.
    #[id = "div_l"]
    pub div_l: IntParam,

    #[id = "div_r"]
    pub div_r: IntParam,

    /// Right time follows left. Off is where ping-pong and stereo spread
    /// come from.
    #[id = "link"]
    pub link: BoolParam,

    #[id = "feedback"]
    pub feedback: FloatParam,

    #[id = "tone"]
    pub tone: FloatParam,

    /// How hard the repeats are driven into whatever the medium is. Silent on
    /// a clean digital line and the whole character of a tape.
    #[id = "drive"]
    pub drive: FloatParam,

    /// Slow pitch drift, and fast. A tape has both; a digital delay has
    /// neither unless you ask.
    #[id = "wow"]
    pub wow: FloatParam,

    #[id = "flutter"]
    pub flutter: FloatParam,

    /// Repeats duck out of the way of the input.
    #[id = "duck"]
    pub duck: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    /// The engine's own two controls.
    ///
    /// Every engine in `delay-dsp` reads them as something different — the
    /// pitch shifter's interval, the multi-tap's spread, the spectral tilt.
    /// The panel names them for what they do on that machine; see
    /// `faces::character_legends`.
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

    /// Where this delay sits in the stack (`use`, `on`, `lane`). Last, so
    /// the params above keep their pre-stack order.
    #[nested]
    pub place: crate::stack::host::StagePlacement,
}

/// The full parameter tree: the stage pool, the lanes, and the stack
/// globals (`fx.stack.params`).
///
/// Stage 1 is nested bare so its ids are the pre-stack plugin's, unchanged;
/// stages 2–8 are prefixed `s2_`…`s8_`.
#[derive(Params)]
pub struct DelayParams {
    #[nested]
    pub stage1: DelayStageParams,
    #[nested(id_prefix = "s2", group = "Stage 2")]
    pub stage2: DelayStageParams,
    #[nested(id_prefix = "s3", group = "Stage 3")]
    pub stage3: DelayStageParams,
    #[nested(id_prefix = "s4", group = "Stage 4")]
    pub stage4: DelayStageParams,
    #[nested(id_prefix = "s5", group = "Stage 5")]
    pub stage5: DelayStageParams,
    #[nested(id_prefix = "s6", group = "Stage 6")]
    pub stage6: DelayStageParams,
    #[nested(id_prefix = "s7", group = "Stage 7")]
    pub stage7: DelayStageParams,
    #[nested(id_prefix = "s8", group = "Stage 8")]
    pub stage8: DelayStageParams,

    /// Per-lane gain / mute / solo, ids `lgain_1`…`lsolo_8`.
    #[nested(array, group = "Lane")]
    pub lanes: [crate::stack::host::LaneParams; MAX_STAGES],

    /// Parallel sum law (`fx.stack.sum`).
    #[id = "summode"]
    pub sum_mode: IntParam,
    /// Stack output trim.
    #[id = "stktrim"]
    pub output_trim_db: FloatParam,

    /// The editor's form factor, persisted by id. One editor, so one form —
    /// not part of any delay's sound.
    #[persist = "editor_form"]
    pub editor_form: parking_lot::RwLock<String>,

    /// Which stage the editor is focused on (`fx.stack.focus`), 0-based —
    /// persisted so the editor reopens where it was left.
    #[persist = "focused_stage"]
    pub focused_stage: parking_lot::RwLock<String>,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            stage1: DelayStageParams::new(true),
            stage2: DelayStageParams::new(false),
            stage3: DelayStageParams::new(false),
            stage4: DelayStageParams::new(false),
            stage5: DelayStageParams::new(false),
            stage6: DelayStageParams::new(false),
            stage7: DelayStageParams::new(false),
            stage8: DelayStageParams::new(false),
            lanes: std::array::from_fn(|_| crate::stack::host::LaneParams::default()),
            sum_mode: crate::stack::host::sum_mode_param(),
            output_trim_db: crate::stack::host::output_trim_param(),
            editor_form: parking_lot::RwLock::new(String::new()),
            focused_stage: parking_lot::RwLock::new(String::new()),
        }
    }
}

impl Default for DelayStageParams {
    fn default() -> Self {
        Self::new(true)
    }
}

impl DelayStageParams {
    /// A stage's params. `first` = stage 1, which starts in the stack (the
    /// plain plugin); the rest start unused until added (`fx.stack.add`).
    #[must_use]
    pub fn new(first: bool) -> Self {
        Self {
            profile: IntParam::new(
                "Space",
                crate::delay::profiles::profile_index("digital").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (crate::delay::profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                crate::delay::profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map_or_else(|| "—".to_string(), |p| p.name.to_string())
            })),

            time_l: FloatParam::new(
                "Time L",
                375.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 4000.0,
                    factor: FloatRange::skew_factor(-1.6),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            time_r: FloatParam::new(
                "Time R",
                375.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 4000.0,
                    factor: FloatRange::skew_factor(-1.6),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            // Off by default: a delay that silently re-times itself the
            // moment it is dropped on a track is a surprise, and the
            // free-running default below is already musical.
            time_sync: dsp_core::time::params::sync_param("Time Sync", false),
            // 1/8D — which at 120 BPM is exactly the 375 ms default above,
            // so switching to Sync on a 120 BPM session changes nothing and
            // the control reads as the same delay in the other language.
            div_l: dsp_core::time::params::division_param("Time L Div", DEFAULT_DIV),
            div_r: dsp_core::time::params::division_param("Time R Div", DEFAULT_DIV),

            link: BoolParam::new("Link", true),

            feedback: FloatParam::new("Feedback", 0.35, FloatRange::Linear { min: 0.0, max: 1.1 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            tone: FloatParam::new("Tone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            drive: FloatParam::new("Drive", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            wow: FloatParam::new("Wow", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            flutter: FloatParam::new("Flutter", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            duck: FloatParam::new("Duck", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            mix: FloatParam::new("Mix", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

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
            place: crate::stack::host::StagePlacement::new(first, MAX_STAGES),
        }
    }
}

/// What the time controls can reach, in milliseconds.
///
/// Named because the sync clamp has to agree with the parameter range: a 1/1
/// at 60 BPM is four seconds, exactly the top of this range, and a slower
/// session asks for more than the delay line has.
pub const MIN_TIME_MS: f64 = 1.0;
pub const MAX_TIME_MS: f64 = 4000.0;

/// The note the times lock to until someone picks another.
///
/// A dotted eighth: the delay behind most of the guitar parts anyone has
/// wanted to copy, and at 120 BPM it is 375 ms — the same as the free-running
/// default, so the two modes agree out of the box.
pub const DEFAULT_DIV: dsp_core::time::MusicalTime = dsp_core::time::MusicalTime::new(
    dsp_core::time::NoteValue::Eighth,
    dsp_core::time::Flavour::Dotted,
);

impl DelayStageParams {
    /// The left time as the audio thread should read it: milliseconds, with
    /// the note value already resolved against `tempo` if Sync is on.
    ///
    /// `tempo` is an `Option` because that is how a host reports it. With no
    /// transport there is no tempo, and the honest answer is the number the
    /// user dialled — a synced delay in a host that has no tempo should keep
    /// making its sound rather than collapse to zero.
    #[must_use]
    pub fn time_l_ms(&self, tempo: Option<f64>) -> f64 {
        self.synced_time(self.time_l.value(), self.div_l.value())
            .resolve_ms_clamped(tempo, MIN_TIME_MS, MAX_TIME_MS)
    }

    /// The right time, which follows the left while `link` is on — including
    /// the note value, so a linked pair stays linked in Sync.
    #[must_use]
    pub fn time_r_ms(&self, tempo: Option<f64>) -> f64 {
        if self.link.value() {
            return self.time_l_ms(tempo);
        }
        self.synced_time(self.time_r.value(), self.div_r.value())
            .resolve_ms_clamped(tempo, MIN_TIME_MS, MAX_TIME_MS)
    }

    fn synced_time(&self, free_ms: f32, div: i32) -> dsp_core::time::SyncedTime {
        dsp_core::time::SyncedTime {
            mode: if self.time_sync.value() {
                dsp_core::time::TimeMode::Synced
            } else {
                dsp_core::time::TimeMode::Free
            },
            free_ms: f64::from(free_ms),
            division: dsp_core::time::MusicalTime::from_param(div),
        }
    }

    /// The profile index the editor should be showing.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        crate::delay::profiles::profile_index(&id).unwrap_or_else(|| {
            (self.profile.value().max(0) as usize).min(crate::delay::profiles::PROFILES.len() - 1)
        })
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static crate::delay::profiles::Profile {
        &crate::delay::profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = crate::delay::profiles::PROFILES
            .get(index)
            .map_or("digital", |p| p.id);
        *self.profile_id.write() = id.to_string();
    }
}

impl DelayParams {
    /// Stage `i` (0-based).
    ///
    /// # Panics
    ///
    /// Panics if `i >= MAX_STAGES`.
    #[must_use]
    pub fn stage(&self, i: usize) -> &DelayStageParams {
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
    pub fn stages(&self) -> impl Iterator<Item = &DelayStageParams> {
        (0..MAX_STAGES).map(|i| self.stage(i))
    }

    pub fn resolved_editor_form(&self) -> fts_audio_ui::EditorForm {
        fts_audio_ui::EditorForm::from_id(&self.editor_form.read()).unwrap_or_default()
    }

    pub fn store_editor_form(&self, form: fts_audio_ui::EditorForm) {
        *self.editor_form.write() = form.id().to_string();
    }
}

impl crate::stack::host::StackHost for DelayParams {
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
        crate::delay::profiles::PROFILES.get(index).map_or_else(
            || "DLY".to_string(),
            |p| crate::delay::ui::control_view::profile_badge(p.id),
        )
    }

    fn style_name(&self, index: usize) -> String {
        crate::delay::profiles::PROFILES
            .get(index)
            .map_or_else(|| "—".to_string(), |p| p.name.to_string())
    }
}

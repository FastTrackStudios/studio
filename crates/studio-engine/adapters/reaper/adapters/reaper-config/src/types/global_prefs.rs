//! Global preference variables stored in `reaper.ini` `[REAPER]` section.
//!
//! Unlike the `proj*` keys (which hold per-project defaults), these keys
//! control application-wide behaviour that cannot differ between projects
//! opened simultaneously.
//!
//! Reference: Mespotine's REAPER config-variable documentation at
//! <https://mespotin.uber.space/Ultraschall/Reaper_Config_Variables.html>

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::{
    AutomationMode, ItemMixBehavior, ItemTimeBase, PanLawTaper, PanMode, ResampleQuality,
};

const S: &str = "REAPER";

/// Global pan-law and pan-mode preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct GlobalPanPrefs {
    /// Pan law value (e.g. -3.0 dB, -4.5 dB).  Stored as a linear factor.
    /// Key: `panlaw`
    pub pan_law: Option<f64>,

    /// Pan law taper curve above −3 dB.
    /// Key: `panlawflags`
    #[facet(opaque)]
    pub pan_law_flags: Option<PanLawTaper>,

    /// Pan mode / stereo algorithm.
    /// Key: `panmode`
    #[facet(opaque)]
    pub pan_mode: Option<PanMode>,
}

impl GlobalPanPrefs {
    /// Returns a new [`GlobalPanPrefs`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            pan_law: Some(1.0),                            // panlaw=1.00000000000000
            pan_law_flags: Some(PanLawTaper::HybridTaper), // panlawflags=3
            pan_mode: Some(PanMode::StereoPan),            // panmode=3
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            pan_law: get_f64(ini, S, "panlaw"),
            pan_law_flags: get_enum_i32(ini, S, "panlawflags"),
            pan_mode: get_enum_i32(ini, S, "panmode"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_f64(ini, S, "panlaw", self.pan_law);
        set_opt_enum_i32(ini, S, "panlawflags", self.pan_law_flags);
        set_opt_enum_i32(ini, S, "panmode", self.pan_mode);
    }
}

/// Default settings for new items.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct GlobalItemDefaults {
    /// How overlapping items on a track interact.
    /// Key: `itemmixflag`
    #[facet(opaque)]
    pub item_mix_flag: Option<ItemMixBehavior>,

    /// Timebase used for item positions and lengths.
    /// Key: `itemtimelock`
    #[facet(opaque)]
    pub item_time_lock: Option<ItemTimeBase>,
}

impl GlobalItemDefaults {
    /// Returns a new [`GlobalItemDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            item_mix_flag: Some(ItemMixBehavior::AlwaysMix), // itemmixflag=1
            item_time_lock: Some(ItemTimeBase::BeatsPositionLengthRate), // itemtimelock=1
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            item_mix_flag: get_enum_i32(ini, S, "itemmixflag"),
            item_time_lock: get_enum_i32(ini, S, "itemtimelock"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_enum_i32(ini, S, "itemmixflag", self.item_mix_flag);
        set_opt_enum_i32(ini, S, "itemtimelock", self.item_time_lock);
    }
}

/// Global pitch-shifting and time-stretching preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct GlobalPitchStretchPrefs {
    /// Default pitch-shift algorithm bitfield.
    /// Key: `defpitchcfg`
    pub def_pitch_cfg: Option<i32>,

    /// Default time-stretch mode bitfield.
    /// Key: `defstretchmode`
    pub def_stretch_mode: Option<i32>,

    /// Playback resampling quality.
    /// Key: `playresamplemode`
    #[facet(opaque)]
    pub play_resample_mode: Option<ResampleQuality>,
}

impl GlobalPitchStretchPrefs {
    /// Returns a new [`GlobalPitchStretchPrefs`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            def_pitch_cfg: Some(589824),                       // defpitchcfg=589824
            def_stretch_mode: Some(0),                         // defstretchmode=0
            play_resample_mode: Some(ResampleQuality::Lowest), // playresamplemode=0
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            def_pitch_cfg: get_i32(ini, S, "defpitchcfg"),
            def_stretch_mode: get_i32(ini, S, "defstretchmode"),
            play_resample_mode: get_enum_i32(ini, S, "playresamplemode"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_i32(ini, S, "defpitchcfg", self.def_pitch_cfg);
        set_opt_i32(ini, S, "defstretchmode", self.def_stretch_mode);
        set_opt_enum_i32(ini, S, "playresamplemode", self.play_resample_mode);
    }
}

/// Global track / send default preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct GlobalTrackDefaults {
    /// Default track automation mode.
    /// Key: `defautomode`
    #[facet(opaque)]
    pub def_auto_mode: Option<AutomationMode>,

    /// Default send flags bitfield.
    /// Key: `defsendflag`
    pub def_send_flag: Option<i32>,

    /// Default track recording flags bitfield.
    /// Key: `deftrackrecflags`
    pub def_track_rec_flags: Option<i32>,

    /// Default track volume in linear amplitude (1.0 = 0 dB).
    /// Key: `deftrackvol`
    pub def_track_vol: Option<f64>,

    /// Default vertical zoom level for tracks.
    /// Key: `defvzoom`
    pub def_vzoom: Option<i32>,

    /// New track flags bitfield.
    /// Key: `newtflag`
    pub new_track_flag: Option<i32>,
}

impl GlobalTrackDefaults {
    /// Returns a new [`GlobalTrackDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            def_auto_mode: Some(AutomationMode::TrimRead), // defautomode=0
            def_send_flag: Some(0),                        // defsendflag=0
            def_track_rec_flags: Some(256),                // deftrackrecflags=256
            def_track_vol: Some(1.0),                      // deftrackvol=1.00000000000000
            def_vzoom: Some(6),                            // defvzoom=6
            new_track_flag: Some(16515),                   // newtflag=16515
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            def_auto_mode: get_enum_i32(ini, S, "defautomode"),
            def_send_flag: get_i32(ini, S, "defsendflag"),
            def_track_rec_flags: get_i32(ini, S, "deftrackrecflags"),
            def_track_vol: get_f64(ini, S, "deftrackvol"),
            def_vzoom: get_i32(ini, S, "defvzoom"),
            new_track_flag: get_i32(ini, S, "newtflag"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_enum_i32(ini, S, "defautomode", self.def_auto_mode);
        set_opt_i32(ini, S, "defsendflag", self.def_send_flag);
        set_opt_i32(ini, S, "deftrackrecflags", self.def_track_rec_flags);
        set_opt_f64(ini, S, "deftrackvol", self.def_track_vol);
        set_opt_i32(ini, S, "defvzoom", self.def_vzoom);
        set_opt_i32(ini, S, "newtflag", self.new_track_flag);
    }
}

/// Global mixer UI preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct GlobalMixerPrefs {
    /// Mixer display flags bitfield.
    /// Key: `mixerflag`
    pub mixer_flag: Option<i32>,

    /// Mixer UI flags bitfield.
    /// Key: `mixeruiflag`
    pub mixer_ui_flag: Option<i32>,

    /// Mixer row configuration flags.
    /// Key: `mixrowflags`
    pub mix_row_flags: Option<i32>,

    /// Show master track in arrange view.
    /// Key: `showmaintrack`
    pub show_main_track: Option<bool>,
}

impl GlobalMixerPrefs {
    /// Returns a new [`GlobalMixerPrefs`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            mixer_flag: Some(0),          // mixerflag=0
            mixer_ui_flag: None,          // mixeruiflag not in factory defaults
            mix_row_flags: Some(48),      // mixrowflags=48
            show_main_track: Some(false), // showmaintrack=0
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            mixer_flag: get_i32(ini, S, "mixerflag"),
            mixer_ui_flag: get_i32(ini, S, "mixeruiflag"),
            mix_row_flags: get_i32(ini, S, "mixrowflags"),
            show_main_track: get_bool_from_i32(ini, S, "showmaintrack"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_i32(ini, S, "mixerflag", self.mixer_flag);
        set_opt_i32(ini, S, "mixeruiflag", self.mixer_ui_flag);
        set_opt_i32(ini, S, "mixrowflags", self.mix_row_flags);
        set_opt_bool_as_i32(ini, S, "showmaintrack", self.show_main_track);
    }
}

/// Miscellaneous global preferences that do not fit a single category.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct GlobalMiscPrefs {
    /// Master mute/solo state bitfield.
    /// Key: `mastermutesolo`
    pub master_mute_solo: Option<i32>,

    /// Auto-crossfade length in seconds.
    /// Key: `autoxfade`
    pub auto_xfade: Option<f64>,

    /// Allow audio feedback in routing (can result in loud noises).
    /// Key: `feedbackmode`
    pub feedback_mode: Option<bool>,

    /// Loop granularity / minimum loop unit.
    /// Key: `loopgran`
    pub loop_gran: Option<f64>,

    /// Loop granule length (in beats).
    /// Key: `loopgranlen`
    pub loop_gran_len: Option<f64>,

    /// Horizontal zoom level (pixels per second of audio).
    /// Key: `zoom`
    pub zoom: Option<f64>,

    /// Vertical zoom level for the track area.
    /// Key: `vzoom2`
    pub vzoom2: Option<f64>,

    /// Playback rate multiplier (1.0 = normal speed).
    /// Key: `playrate`
    pub play_rate: Option<f64>,

    /// Miscellaneous options bitfield.
    /// Key: `miscopts`
    pub misc_opts: Option<i32>,

    /// Project notes window open state.
    /// Key: `opennotes`
    pub open_notes: Option<i32>,

    /// Volume-envelope display range in dB.
    /// Key: `volenvrange`
    pub vol_env_range: Option<f64>,

    /// Pooled-envelope attachment mode.
    /// Key: `pooledenvattach`
    pub pooled_env_attach: Option<i32>,

    /// ReWire slave mode enable (0 = off, 1 = on).
    /// Key: `rewireslave`
    pub rewire_slave: Option<i32>,

    /// Score minimum note length.
    /// Key: `scoreminnotelen`
    pub score_min_note_len: Option<i32>,

    /// Score quantization setting.
    /// Key: `scorequant`
    pub score_quant: Option<i32>,

    /// Peak/spectrum display maximum value in dB.
    /// Key: `psmaxv`
    pub ps_max_v: Option<f64>,

    /// Peak/spectrum display minimum value in dB.
    /// Key: `psminv`
    pub ps_min_v: Option<f64>,
}

impl GlobalMiscPrefs {
    /// Returns a new [`GlobalMiscPrefs`] with REAPER factory default values.
    ///
    /// Fields whose factory value cannot be parsed into the field type are `None`.
    pub fn factory_defaults() -> Self {
        Self {
            master_mute_solo: Some(0),  // mastermutesolo=0
            auto_xfade: Some(129.0),    // autoxfade=129
            feedback_mode: Some(false), // feedbackmode=0
            loop_gran: Some(0.0),       // loopgran=0
            loop_gran_len: Some(4.0),   // loopgranlen=4.00000000000000
            zoom: Some(100.0),          // zoom=100.00000000000000
            vzoom2: Some(6.0),          // vzoom2=6
            play_rate: Some(1.0),       // playrate=1.00000000000000
            misc_opts: Some(0),         // miscopts=0
            open_notes: Some(2),        // opennotes=2
            vol_env_range: Some(2.0),   // volenvrange=2 (field is f64)
            pooled_env_attach: Some(0), // pooledenvattach=0
            rewire_slave: Some(1),      // rewireslave=1
            score_min_note_len: None, // scoreminnotelen=0.00000000000000 (float, unparseable as i32)
            score_quant: None,        // scorequant=0.00000000000000 (float, unparseable as i32)
            ps_max_v: Some(4.0),      // psmaxv=4.00000000000000
            ps_min_v: Some(0.25),     // psminv=0.25000000000000
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            master_mute_solo: get_i32(ini, S, "mastermutesolo"),
            auto_xfade: get_f64(ini, S, "autoxfade"),
            feedback_mode: get_bool_from_i32(ini, S, "feedbackmode"),
            loop_gran: get_f64(ini, S, "loopgran"),
            loop_gran_len: get_f64(ini, S, "loopgranlen"),
            zoom: get_f64(ini, S, "zoom"),
            vzoom2: get_f64(ini, S, "vzoom2"),
            play_rate: get_f64(ini, S, "playrate"),
            misc_opts: get_i32(ini, S, "miscopts"),
            open_notes: get_i32(ini, S, "opennotes"),
            vol_env_range: get_f64(ini, S, "volenvrange"),
            pooled_env_attach: get_i32(ini, S, "pooledenvattach"),
            rewire_slave: get_i32(ini, S, "rewireslave"),
            score_min_note_len: get_i32(ini, S, "scoreminnotelen"),
            score_quant: get_i32(ini, S, "scorequant"),
            ps_max_v: get_f64(ini, S, "psmaxv"),
            ps_min_v: get_f64(ini, S, "psminv"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_i32(ini, S, "mastermutesolo", self.master_mute_solo);
        set_opt_f64(ini, S, "autoxfade", self.auto_xfade);
        set_opt_bool_as_i32(ini, S, "feedbackmode", self.feedback_mode);
        set_opt_f64(ini, S, "loopgran", self.loop_gran);
        set_opt_f64(ini, S, "loopgranlen", self.loop_gran_len);
        set_opt_f64(ini, S, "zoom", self.zoom);
        set_opt_f64(ini, S, "vzoom2", self.vzoom2);
        set_opt_f64(ini, S, "playrate", self.play_rate);
        set_opt_i32(ini, S, "miscopts", self.misc_opts);
        set_opt_i32(ini, S, "opennotes", self.open_notes);
        set_opt_f64(ini, S, "volenvrange", self.vol_env_range);
        set_opt_i32(ini, S, "pooledenvattach", self.pooled_env_attach);
        set_opt_i32(ini, S, "rewireslave", self.rewire_slave);
        set_opt_i32(ini, S, "scoreminnotelen", self.score_min_note_len);
        set_opt_i32(ini, S, "scorequant", self.score_quant);
        set_opt_f64(ini, S, "psmaxv", self.ps_max_v);
        set_opt_f64(ini, S, "psminv", self.ps_min_v);
    }
}

/// All global (non-project-specific) preference variables from `reaper.ini`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct GlobalPrefs {
    /// Pan law and pan mode.
    pub pan: GlobalPanPrefs,

    /// Item defaults.
    pub items: GlobalItemDefaults,

    /// Pitch-shifting and time-stretching defaults.
    pub pitch_stretch: GlobalPitchStretchPrefs,

    /// Track and send defaults.
    pub track_defaults: GlobalTrackDefaults,

    /// Mixer UI preferences.
    pub mixer: GlobalMixerPrefs,

    /// Miscellaneous global preferences.
    pub misc: GlobalMiscPrefs,
}

impl GlobalPrefs {
    /// Returns a new [`GlobalPrefs`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            pan: GlobalPanPrefs::factory_defaults(),
            items: GlobalItemDefaults::factory_defaults(),
            pitch_stretch: GlobalPitchStretchPrefs::factory_defaults(),
            track_defaults: GlobalTrackDefaults::factory_defaults(),
            mixer: GlobalMixerPrefs::factory_defaults(),
            misc: GlobalMiscPrefs::factory_defaults(),
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            pan: GlobalPanPrefs::from_ini(ini),
            items: GlobalItemDefaults::from_ini(ini),
            pitch_stretch: GlobalPitchStretchPrefs::from_ini(ini),
            track_defaults: GlobalTrackDefaults::from_ini(ini),
            mixer: GlobalMixerPrefs::from_ini(ini),
            misc: GlobalMiscPrefs::from_ini(ini),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        self.pan.write_to(ini);
        self.items.write_to(ini);
        self.pitch_stretch.write_to(ini);
        self.track_defaults.write_to(ini);
        self.mixer.write_to(ini);
        self.misc.write_to(ini);
    }
}

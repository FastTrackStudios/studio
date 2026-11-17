//! A script's monophonic legato over a one-layer sustain
//! (`ArticulationSpec::script_legato`, MOJO 2's legato engine; the decode is
//! `_notes/decode-legato.md` beside the library, script_1.ksp line numbers
//! below).
//!
//! The line keeps the script's three globals: the last legato-path note
//! (`$fvjr3`), the one before it (`$fy05x`, for a legato back) and the frame
//! of the last connection (`$1xho1`, for the speed law). A transition holds
//! for its computed time, then fades and hands over to the target sustain
//! (`ScriptLegPending`, fired from `render_split` on its exact frame).

use super::*;
use crate::spec::{ScriptLegatoSpec, ScriptLegatoTiming};

/// The line's legato memory.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScriptLegState {
    /// The last legato-path note, cleared when it is released.
    prev: Option<u8>,
    /// The note before it (the target of a legato back).
    back: Option<u8>,
    /// Frame of the last fresh note or transition.
    last_on: u64,
    /// The last note-on's velocity (a legato back plays at it).
    last_vel: u8,
    /// The transition holding before its target.
    pending: Option<ScriptLegPending>,
    /// The voices of the last target (`MARK_7`).
    target_strike: Option<u32>,
}

/// A transition waiting out its hold.
#[derive(Debug, Clone)]
pub(crate) struct ScriptLegPending {
    due: u64,
    strike: u32,
    note: u8,
    velocity: u8,
    artic: String,
    fade_out: usize,
    target_offset: usize,
}

/// The connection's speed in percent and hold in ms, as the script forms
/// them in integers (26064-26100). `m` is the speed index 0 (slow) ..= 127.
pub(crate) fn legato_speed_and_hold(t: &ScriptLegatoTiming, interval: u8, m: i32) -> (i32, i32) {
    let k = i32::from(interval.max(1)) - 1;
    let lo = k * (t.slow_large_pct - t.slow_small_pct) / 11 + t.slow_small_pct;
    let hi = k * (t.fast_large_pct - t.fast_small_pct) / 11 + t.fast_small_pct;
    let speed = (m * (hi - lo) / 127 + lo).max(1);
    let h = k * (t.hold_large_ms - t.hold_small_ms) as i32 / 11 + t.hold_small_ms as i32;
    let f = (100.0 / speed as f32 * 100.0) as i32;
    (speed, h * f / 100)
}

/// The speed index from the time since the last connection (Speed mode):
/// `(ioi − slowest)·127/(fastest − slowest)`, the time clamped to the range.
pub(crate) fn legato_speed_index(t: &ScriptLegatoTiming, ioi_ms: i32) -> i32 {
    let span = t.fastest_ms - t.slowest_ms;
    if span == 0 {
        return 0;
    }
    let ioi = ioi_ms.clamp(
        t.fastest_ms.min(t.slowest_ms),
        t.fastest_ms.max(t.slowest_ms),
    );
    (ioi - t.slowest_ms) * 127 / span
}

impl SampleEngine {
    /// The current articulation's legato, when it is switched on.
    fn script_legato_on(&self) -> Option<ScriptLegatoSpec> {
        let spec = self
            .patch
            .spec
            .articulation(&self.articulation)?
            .script_legato
            .clone()?;
        let cc = usize::from(spec.enable_cc & 127);
        let on = if self.cc_sent[cc] {
            self.cc_values[cc] > 63
        } else {
            spec.on
        };
        on.then_some(spec)
    }

    /// A note-on of a legato articulation. `false` when its legato is off
    /// (the caller plays the plain short).
    pub(crate) fn script_legato_note_on(&mut self, note: u8, velocity: u8) -> bool {
        let Some(spec) = self.script_legato_on() else {
            return false;
        };
        // A transition still holding is cut short (its callback's wait is
        // aborted by this note's queue entry, 25631-25646).
        if let Some(p) = self.script_leg.pending.take() {
            let fade = ms_to_frames_f(spec.timing.fade_out_ms, self.sample_rate);
            self.voices.fade_out_strike_bodies(p.strike, fade);
        }
        let prev = self.script_leg.prev;
        self.script_leg.back = prev;
        self.script_leg.last_vel = velocity;
        let since_ms = frames_to_ms(
            self.frames_rendered.saturating_sub(self.script_leg.last_on),
            self.sample_rate,
        );
        let connects = prev.is_some_and(|p| p.abs_diff(note) <= spec.max_interval)
            && (spec.bypass_ms <= 0.0 || since_ms <= spec.bypass_ms);
        match prev {
            Some(from) if connects => self.script_transition(&spec, from, note, velocity),
            _ => {
                // A fresh note: the line is monophonic, its voices fade
                // over the fade-in (25653-25690), then the sustain plays.
                let fade = ms_to_frames_f(spec.timing.fade_in_ms, self.sample_rate);
                let line = self.cur_line as u8;
                self.voices.fade_out_line_bodies(line, fade);
                self.trigger_zoned_short(note, velocity);
            }
        }
        self.script_leg.prev = Some(note);
        self.script_leg.last_on = self.frames_rendered;
        true
    }

    /// The transition `from → to` (26028-26469).
    fn script_transition(&mut self, spec: &ScriptLegatoSpec, from: u8, to: u8, velocity: u8) {
        let t = &spec.timing;
        let sr = self.sample_rate;
        let interval = from.abs_diff(to);
        let (artic, direction) = if from >= to {
            (&spec.down, "down")
        } else {
            (&spec.up, "up")
        };
        let m = match t.speed_cc {
            _ if t.speed_by_velocity => i32::from(velocity),
            Some(cc) => {
                let c = usize::from(cc & 127);
                127 - i32::from(if self.cc_sent[c] {
                    self.cc_values[c]
                } else {
                    t.speed_cc_default
                })
            }
            None => {
                let ioi = frames_to_ms(
                    self.frames_rendered.saturating_sub(self.script_leg.last_on),
                    sr,
                );
                legato_speed_index(t, ioi as i32)
            }
        };
        let (speed, hold_ms) = legato_speed_and_hold(t, interval, m);

        // Every sounding release fades (25571), the line's voices over the
        // fade-in (26438-26443, the previous transition too, 26124).
        let sustain = self.articulation.clone();
        let fade_rel = self
            .patch
            .spec
            .articulation(&sustain)
            .map_or(0.0, |a| a.fade_releases_ms);
        if fade_rel > 0.0 {
            self.voices.fade_out_releases(ms_to_frames_f(fade_rel, sr));
        }
        let line = self.cur_line as u8;
        self.voices
            .fade_out_line_bodies(line, ms_to_frames_f(t.fade_in_ms, sr));

        // The transition zone: the destination key, the interval, the
        // dynamic by velocity.
        let layer = spec
            .layer_thresholds
            .iter()
            .filter(|&&e| velocity >= e)
            .count();
        let dynamic = self
            .patch
            .spec
            .articulation(artic)
            .and_then(|a| a.dynamics.get(layer).or(a.dynamics.last()).cloned())
            .unwrap_or_default();
        let idx = self.zone_keys.get(usize::from(to)).and_then(|c| {
            c.iter().map(|&i| i as usize).find(|&i| {
                let z = &self.patch.spec.zones[i];
                z.interval == u32::from(interval) && self.zone_passes(z, artic, direction, &dynamic)
            })
        });
        self.voices.begin_strike();
        let strike = self.voices.current_strike();
        if let Some(idx) = idx {
            let gain = spec
                .gain_db
                .get(usize::from(velocity.min(127)))
                .map_or(1.0, |&db| db_to_gain(db));
            let saved = (
                self.spawn_extra_start,
                self.spawn_stretch,
                self.spawn_fade_in,
            );
            self.spawn_extra_start = ms_to_frames_f(t.trans_offset_ms, sr);
            self.spawn_stretch = f64::from(speed) / 100.0;
            self.spawn_fade_in = ms_to_frames_f(t.fade_in_ms / 2.0, sr);
            self.spawn_zone_voice(idx, to, VoiceKind::Short, gain, None, 0.0);
            (
                self.spawn_extra_start,
                self.spawn_stretch,
                self.spawn_fade_in,
            ) = saved;
        }
        // Its key-up before the target plays the release sample over no
        // body: the holding transition is not faded (37014-37579).
        let k = usize::from(to & 127);
        self.release_body_strike[k] = Some(u32::MAX);
        self.release_sample_notes[k].clear();
        let has_release = self
            .patch
            .spec
            .articulation(&sustain)
            .is_some_and(|a| a.release_sample.is_some());
        if has_release {
            let (dynamic_of, _) = self.short_layer_and_velvol(&sustain, velocity);
            self.release_sample_notes[k].push((sustain.clone(), dynamic_of, velocity));
        }
        self.script_leg.pending = Some(ScriptLegPending {
            due: self.frames_rendered + ms_to_frames_f(hold_ms as f32, sr) as u64,
            strike,
            note: to,
            velocity,
            artic: sustain,
            fade_out: ms_to_frames_f(t.fade_out_ms, sr),
            target_offset: ms_to_frames_f(t.target_offset_ms, sr),
        });
    }

    /// The key-up of a note while its articulation's legato is on. `true`
    /// when the legato took it (no release sample here).
    pub(crate) fn script_legato_note_off(&mut self, note: u8) -> bool {
        let Some(spec) = self.script_legato_on() else {
            return false;
        };
        if self.script_leg.prev != Some(note) {
            // Not the line's last note: its voices have been faded already.
            self.release_sample_notes[usize::from(note & 127)].clear();
            return true;
        }
        self.script_leg.prev = None;
        if spec.release_fade_ms > 0.0 {
            // `fade_out(MARK_7, Exp2(release knob))`; the key-up runs the
            // rest of the note's release as usual.
            if let Some(strike) = self.script_leg.target_strike.take() {
                let fade = ms_to_frames_f(spec.release_fade_ms, self.sample_rate);
                self.voices.fade_out_strike_bodies(strike, fade);
            }
            return false;
        }
        let back = self
            .script_leg
            .back
            .filter(|&b| b != note && self.held_notes.contains_key(&b));
        let Some(back) = back else {
            // The normal release sample (the caller plays it).
            return false;
        };
        // Legato back to the held note before it (33710-36437), at the last
        // note-on's velocity; no release sample.
        self.release_sample_notes[usize::from(note & 127)].clear();
        let line = self.cur_line as u8;
        self.voices.release_note_envelopes(line, note);
        if let Some(p) = self.script_leg.pending.take() {
            let fade = ms_to_frames_f(spec.timing.fade_out_ms, self.sample_rate);
            self.voices.fade_out_strike_bodies(p.strike, fade);
        }
        let velocity = self.script_leg.last_vel;
        self.script_leg.back = Some(note);
        if note.abs_diff(back) <= spec.max_interval {
            self.script_transition(&spec, note, back, velocity);
        }
        self.script_leg.prev = Some(back);
        self.script_leg.last_on = self.frames_rendered;
        true
    }

    /// Frames until the holding transition's end.
    pub(crate) fn script_leg_due_in(&self) -> Option<usize> {
        let p = self.script_leg.pending.as_ref()?;
        Some(p.due.saturating_sub(self.frames_rendered) as usize)
    }

    /// The hold is over (26470-27111): the transition fades and, the key
    /// still down, the target sustain enters at its offset, fading in.
    pub(crate) fn fire_script_legato(&mut self) {
        if self
            .script_leg
            .pending
            .as_ref()
            .is_none_or(|p| p.due > self.frames_rendered)
        {
            return;
        }
        let Some(p) = self.script_leg.pending.take() else {
            return;
        };
        self.voices.fade_out_strike_bodies(p.strike, p.fade_out);
        if !self.held_notes.contains_key(&p.note) {
            return;
        }
        let saved_artic = std::mem::replace(&mut self.articulation, p.artic);
        let saved = (
            self.spawn_extra_start,
            self.spawn_fade_in,
            self.in_transition,
        );
        self.spawn_extra_start = p.target_offset;
        self.spawn_fade_in = p.fade_out;
        self.in_transition = true;
        self.release_sample_notes[usize::from(p.note & 127)].clear();
        self.voices.begin_strike();
        self.script_leg.target_strike = Some(self.voices.current_strike());
        self.trigger_zoned_short(p.note, p.velocity);
        (
            self.spawn_extra_start,
            self.spawn_fade_in,
            self.in_transition,
        ) = saved;
        self.articulation = saved_artic;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trumpet_sus() -> ScriptLegatoTiming {
        ScriptLegatoTiming {
            fade_in_ms: 50.0,
            trans_offset_ms: 28.0,
            hold_small_ms: 116.0,
            hold_large_ms: 120.0,
            fade_out_ms: 95.0,
            target_offset_ms: 373.0,
            slow_small_pct: 95,
            fast_small_pct: 120,
            slow_large_pct: 95,
            fast_large_pct: 120,
            slowest_ms: 1420,
            fastest_ms: 46,
            speed_cc: None,
            speed_cc_default: 0,
            speed_by_velocity: false,
        }
    }

    /// KONTRA's trace of Trumpet 66 → 67 at 300 ms: speed 117 % (Time
    /// Machine 698062), the target 99 ms after the transition.
    #[test]
    fn trumpet_semitone_at_300_ms() {
        let t = trumpet_sus();
        let m = legato_speed_index(&t, 300);
        assert_eq!(m, 103);
        assert_eq!(legato_speed_and_hold(&t, 1, m), (115, 99));
    }

    /// The decode's worked values at i = 1: 120 % → 96 ms, 95 % → 121 ms.
    #[test]
    fn hold_follows_speed() {
        let t = trumpet_sus();
        assert_eq!(legato_speed_and_hold(&t, 1, 127), (120, 96));
        assert_eq!(legato_speed_and_hold(&t, 1, 0), (95, 121));
    }
}

//! A CSS legato transition exactly as Kontakt runs the instrument's script —
//! `LegatoEngineSpec::kontakt_transition`. Every step and number was
//! null-tested against Kontakt 8 renders of CSS 1st Violins (each transition
//! explained to −41 dB by the four voices below); the KSP line references are
//! in `KontaktTransitionSpec`'s doc and the orchestra decode notes.

use super::*;
use crate::spec::{KontaktTransitionSpec, css_transition_ccs};

/// The transition voice's phrase-start dip: a measured recovery curve, or a
/// plain linear ramp back from `(gain, frames)`.
#[derive(Clone)]
pub(crate) enum KontaktDip {
    Curve(voice::FlexEnv),
    Ramp(f32, usize),
}

impl SampleEngine {
    /// Fire one transition `from → to` (the overlap delay has already been
    /// waited by the reactive countdown).
    pub(crate) fn fire_kontakt_transition(
        &mut self,
        from: u8,
        to: u8,
        velocity: u8,
        ioi_ms: f32,
        kt: &KontaktTransitionSpec,
    ) {
        let expressive = self.legato_expressive;
        let (zone, od_ms, lt_base) = {
            let cfg = self.patch.spec.legato_cfg();
            let base = if expressive {
                cfg.lt_offset_expressive.clone()
            } else {
                cfg.lt_offset_low_latency.clone()
            };
            (
                cfg.velocity_range(velocity),
                cfg.overlap_delay_ms(ioi_ms, velocity, expressive) as f32,
                base,
            )
        };
        let interval = u32::from(to.abs_diff(from));
        let big_leap = interval > 12;
        let line = self.cur_line as u8;
        let sr = self.sample_rate;
        let ms = move |v: f32| ms_to_frames_f(v, sr);

        // 1. Retire the old pair with linear fades: the body over
        //    `old_body_fade_ms` (500 beyond an octave), the previous transition
        //    voice over 250 → 400 ms by IOI (low latency) / flat 400 (expressive).
        let lt_retire_ms = if expressive {
            400.0
        } else {
            250.0 + 150.0 * ((ioi_ms - 150.0) / 150.0).clamp(0.0, 1.0)
        };
        let old_body_ms = if big_leap {
            kt.big_leap_old_fade_ms
        } else {
            kt.old_body_fade_ms
        };
        self.voices
            .fade_out_note_line(line, from, ms(lt_retire_ms), ms(old_body_ms));

        // 2. The old note's "note in phrase" release, at a flat level.
        self.release_gain_override = Some(db_to_gain(kt.in_phrase_release_db));
        self.spawn_release(from);
        self.release_gain_override = None;

        // 3. The destination body: silent, then a linear fade-in — after
        //    `body_wait_ms` normally; immediately (and entered 75 ms in)
        //    beyond an octave.
        let (wait, fade) = if big_leap {
            (0, ms(kt.big_leap_new_fade_ms))
        } else {
            (ms(kt.body_wait_ms), ms(kt.body_fade_ms))
        };
        self.sustain_fade_in = Some((wait, fade, fade, 0));
        self.spawn_extra_start = if big_leap {
            ms(kt.big_leap_body_offset_ms)
        } else {
            0
        };
        self.in_transition = true;
        self.play_direction = if to >= from { "up" } else { "down" }.to_string();
        self.trigger_zoned_sustain(to);
        self.in_transition = false;
        self.spawn_extra_start = 0;
        self.sustain_fade_in = None;

        // 4. The transition sample (interval ≤ an octave): the LOWER note's
        //    interval-coded zone of BOTH CC2 sides, entered at the mode/zone
        //    offset + the overlap delay.
        if !big_leap {
            let base_ms = lt_base
                .get(usize::from(zone.max(1) - 1))
                .copied()
                .unwrap_or(0) as f32;
            let offset = ms(base_ms + od_ms);
            let env = self.kontakt_transition_env(kt, expressive, zone, ioi_ms);
            let dip = self.kontakt_phrase_dip(kt);
            let (leg_nv, leg_vib) = self.legato_pair_ids(false);
            for leg in [leg_nv, leg_vib].into_iter().flatten() {
                self.spawn_kontakt_lt(&leg, from, to, interval, offset, env.clone(), dip.clone());
            }
        }
        self.line_mut().note = Some(to);
    }

    /// The transition voice's amplitude envelope for this transition: first
    /// node `(CC66·CC67)/127²` reached in the mode's `lt_seg0_ms`, then
    /// `lt_env`.
    fn kontakt_transition_env(
        &self,
        kt: &KontaktTransitionSpec,
        expressive: bool,
        zone: u8,
        ioi_ms: f32,
    ) -> Option<voice::FlexEnv> {
        let (cc66, cc67) = css_transition_ccs(expressive, zone, ioi_ms);
        let node = (cc66 * cc67) / (127.0 * 127.0);
        let seg0 = if expressive {
            kt.lt_seg0_ms_expressive
        } else {
            kt.lt_seg0_ms_low_latency
        };
        let mut segs = vec![(seg0, node, 0.5)];
        segs.extend(kt.lt_env.iter().map(|s| (s.time_ms, s.level, s.curve)));
        if kt.lt_node1_cc66_depth > 0.0 {
            if let Some(first) = segs.get_mut(1) {
                first.1 = 1.0 - kt.lt_node1_cc66_depth * (1.0 - cc66 / 127.0);
            }
        }
        voice::FlexEnv::from_segments(&segs, 0.0, self.sample_rate, false)
    }

    /// The transition voice's phrase-start dip (KSP `$1z3x0`, 20038-20069):
    /// within the window of the phrase anchor it plays `phrase_dip_db` down
    /// (recovering over the window); otherwise 0 dB and the anchor moves to
    /// `now − window` so the dip never applies twice in a row.
    fn kontakt_phrase_dip(&mut self, kt: &KontaktTransitionSpec) -> Option<KontaktDip> {
        let now = self.frames_rendered;
        let window = ms_to_frames_f(kt.phrase_dip_window_ms, self.sample_rate);
        if now.saturating_sub(self.ab_anchor_frame) > window as u64 {
            self.ab_anchor_frame = now.saturating_sub(window as u64);
            return None;
        }
        let start = db_to_gain(kt.phrase_dip_db);
        if kt.phrase_dip_curve.is_empty() {
            return Some(KontaktDip::Ramp(start, window));
        }
        let segs: Vec<(f32, f32, f32)> = kt
            .phrase_dip_curve
            .iter()
            .map(|s| (s.time_ms, s.level, s.curve))
            .collect();
        voice::FlexEnv::from_segments(&segs, start, self.sample_rate, false).map(KontaktDip::Curve)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_kontakt_lt(
        &mut self,
        leg: &str,
        from: u8,
        to: u8,
        interval: u32,
        offset: usize,
        env: Option<voice::FlexEnv>,
        dip: Option<KontaktDip>,
    ) {
        self.spawn_kontakt_lt_at(leg, from, to, interval, offset, env, dip, 1.0);
    }

    /// [`spawn_kontakt_lt`](Self::spawn_kontakt_lt) at an extra `level`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_kontakt_lt_at(
        &mut self,
        leg: &str,
        from: u8,
        to: u8,
        interval: u32,
        offset: usize,
        env: Option<voice::FlexEnv>,
        dip: Option<KontaktDip>,
        level: f32,
    ) {
        let lower = from.min(to);
        let direction = if to > from { "up" } else { "down" };
        let Some(a) = self.patch.spec.articulation(leg) else {
            return;
        };
        let dyns = a.dynamics.clone();
        for label in dyns {
            let g = self.group_curve_gain(leg, &label).unwrap_or(1.0) * level;
            if g <= 0.0 {
                continue;
            }
            let idx = self.patch.spec.zones.iter().position(|z| {
                z.articulation.eq_ignore_ascii_case(leg)
                    && z.direction.eq_ignore_ascii_case(direction)
                    && z.interval == interval
                    && z.dynamic.eq_ignore_ascii_case(&label)
                    && (z.key_min..=z.key_max).contains(&lower)
                    && self
                        .solo_mic
                        .as_ref()
                        .is_none_or(|m| z.mic.eq_ignore_ascii_case(m))
            });
            let Some(idx) = idx else { continue };
            // Tagged `to` (so the next transition retires it); pitched from
            // the LOWER note, which is where Kontakt maps the zone.
            let pitch = f64::from(lower) - f64::from(to);
            if self.spawn_zone_voice_at(idx, to, VoiceKind::Legato, g, None, pitch, offset) {
                if let Some(v) = self.voices.last_spawned_mut() {
                    if let Some(e) = env.clone() {
                        v.set_flex_env(e);
                    }
                    match dip.clone() {
                        Some(KontaktDip::Curve(c)) => v.set_flex_env2(c),
                        Some(KontaktDip::Ramp(d, frames)) => {
                            let full = v.target_gain;
                            v.gain = full * d;
                            v.ramp_gain(full, frames);
                        }
                        None => {}
                    }
                }
            }
        }
    }
}

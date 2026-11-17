//! The fretboard stage: a fretted instrument's note-on finds a string and
//! fret, then plays that string's zone (see [`crate::spec::FretboardSpec`]).
//!
//! It runs instead of the zoned paths for an articulation whose zones carry
//! a string (`ZoneSpec::string`); anything else (a library's noises, FX)
//! falls through to them. The state here is what the placement and the
//! round robins remember between notes, so a render that starts from a
//! fresh engine reproduces Kontakt's from a fresh instrument.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::spec::{FretLeadInSpec, FretboardSpec, LibrarySpec};

/// What the fretboard remembers between notes.
#[derive(Debug, Default)]
pub(crate) struct FretState {
    /// Built for this many strings (0 = not yet).
    strings: usize,
    /// Articulations whose zones are string-bound.
    fretted: HashSet<String>,
    /// The last string played (0-based) and its fret.
    last_string: Option<usize>,
    last_fret: i32,
    /// Each string's last fret.
    last_frets: Vec<i32>,
    /// The note held on each string.
    ringing: Vec<Option<u8>>,
    /// The string each held note sounds on.
    note_string: HashMap<u8, usize>,
    /// Notes played per (articulation, string, fret): the RR order's index.
    rr_count: HashMap<(String, usize, i32), usize>,
    /// The borrow generator's state.
    lcg: u32,
    /// Lead-in bodies waiting for their time.
    pub(crate) due: Vec<FretDue>,
}

/// A lead-in's body, due at frame `at` (see `FretLeadInSpec`).
#[derive(Debug, Clone)]
pub(crate) struct FretDue {
    at: u64,
    note: u8,
    velocity: u8,
    string: usize,
    lead: FretLeadInSpec,
    line: u8,
}

impl FretState {
    fn reset(&mut self, spec: &LibrarySpec, fb: &FretboardSpec) {
        let n = fb.strings.len();
        *self = Self {
            strings: n,
            fretted: spec
                .zones
                .iter()
                .filter(|z| z.string > 0)
                .map(|z| z.articulation.to_ascii_lowercase())
                .collect(),
            last_string: None,
            last_fret: -1,
            last_frets: vec![i32::from(fb.fresh_last_fret); n],
            ringing: vec![None; n],
            note_string: HashMap::new(),
            rr_count: HashMap::new(),
            lcg: fb.borrow.as_ref().map_or(0, |b| b.seed),
            due: Vec::new(),
        };
    }

    /// The string (0-based) `note` is placed on, or `None` when it fits on
    /// none (`str___get_best_string`, Shreddage 3.5 script_0.ksp L7484).
    fn place(&self, fb: &FretboardSpec, note: u8) -> Option<usize> {
        let w = &fb.weights;
        let fits = |s: usize| {
            let open = i32::from(fb.strings[s]);
            let fret = i32::from(note) - open;
            (0..=i32::from(fb.frets)).contains(&fret).then_some(fret)
        };
        let in_hand = |s: usize, fret: i32| {
            let lo = i32::from(fb.hand_fret);
            let hi = (lo + i32::from(fb.hand_widths.get(s).copied().unwrap_or(0))).min(24);
            (lo..=hi).contains(&fret)
        };
        let mut best: Option<(usize, u32)> = None;
        for s in 0..fb.strings.len() {
            let Some(fret) = fits(s) else { continue };
            let mut score = 1;
            if in_hand(s, fret) {
                score += w.in_hand;
            }
            if self.ringing[s].is_none() {
                score += w.free;
            }
            if let Some(last) = self.last_string {
                if last.abs_diff(s) == 1 {
                    score += w.adjacent;
                }
                if last == s {
                    score += w.same_string;
                }
            }
            if self.last_frets[s] == fret {
                score += w.same_fret;
            }
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((s, score));
            }
        }
        let (string, _) = best?;
        if in_hand(string, fits(string)?) {
            return Some(string);
        }
        // Not under the hand: the string whose fret is nearest the last.
        let mut nearest = (string, 24);
        for s in 0..fb.strings.len() {
            let Some(fret) = fits(s) else { continue };
            if in_hand(s, fret) {
                continue;
            }
            let d = (fret - self.last_fret).abs();
            if d < nearest.1 {
                nearest = (s, d);
            }
        }
        Some(nearest.0)
    }

    /// One draw of the borrow generator: `((s >> 8) & 0xFFFFFF)` after
    /// stepping it (`None` without a borrow spec). The borrow takes
    /// `draw mod 1000 + 1 <= chance_permille`.
    fn draw(&mut self, fb: &FretboardSpec) -> Option<u32> {
        let b = fb.borrow.as_ref()?;
        self.lcg = b.multiplier.wrapping_mul(self.lcg).wrapping_add(1);
        Some((self.lcg >> 8) & 0x00FF_FFFF)
    }
}

impl SampleEngine {
    /// Note-on through the fretboard. `false` = not a fretted note (no
    /// fretboard, or an articulation with no string-bound zones): the
    /// caller plays it as any other.
    pub(crate) fn fretted_note_on(&mut self, note: u8, velocity: u8) -> bool {
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return false;
        };
        if self.fret.strings != fb.strings.len() {
            self.fret.reset(&self.patch.spec, fb);
        }
        let artic = self.articulation.clone();
        if !self.fret.fretted.contains(&artic.to_ascii_lowercase()) {
            return false;
        }
        let Some(string) = self.fret.place(fb, note) else {
            return true;
        };
        let fret = i32::from(note) - i32::from(fb.strings[string]);
        let retrigger_ms = fb.retrigger_fade_ms;
        let lead_in = fb
            .lead_ins
            .iter()
            .find(|l| l.articulation.eq_ignore_ascii_case(&artic) && velocity >= l.min_velocity)
            .cloned();

        // A note already ringing on this string: struck again, it fades;
        // another note there is let go.
        let line = self.cur_line as u8;
        if let Some(old) = self.fret.ringing[string].take() {
            self.fret.note_string.remove(&old);
            if old == note {
                let frames = ms_to_frames_f(retrigger_ms, self.sample_rate);
                self.voices.fade_out_note_line_any(line, old, frames);
            } else {
                let frames = self.fretted_release_frames();
                self.voices.note_off_line(line, old, Some(frames));
            }
        }

        match lead_in {
            // The lead first (Shreddage's pop); the body follows when due.
            Some(l) => {
                self.play_fretted_lead(&l.lead, string, note, velocity);
                let at =
                    self.frames_rendered + ms_to_frames_f(l.body_delay_ms, self.sample_rate) as u64;
                self.fret.due.push(FretDue {
                    at,
                    note,
                    velocity,
                    string,
                    lead: l,
                    line,
                });
            }
            None => self.play_fretted(&artic, string, note, velocity, 0.0),
        }

        let st = &mut self.fret;
        st.ringing[string] = Some(note);
        st.note_string.insert(note, string);
        st.last_string = Some(string);
        st.last_fret = fret;
        st.last_frets[string] = fret;
        true
    }

    /// Play `artic` on `string` (0-based) at `note`: its velocity layer,
    /// the position's next round robin, the borrow draw, then the zone,
    /// started `offset_ms` into its recording.
    fn play_fretted(&mut self, artic: &str, string: usize, note: u8, velocity: u8, offset_ms: f32) {
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return;
        };
        let Some(a) = self.patch.spec.articulation(artic) else {
            return;
        };
        let open = fb.strings[string];
        let fret = i32::from(note) - i32::from(open);
        let dynamic = Self::short_band(a, velocity)
            .and_then(|(band, ..)| a.dynamics.get(band).cloned())
            .unwrap_or_default();
        let rr_n = a.rr.max(1);
        let gain_db = a
            .vel_gain_db
            .get(usize::from(velocity.min(127)))
            .copied()
            .unwrap_or(0.0);
        let key = match fb.rr_slot_stride {
            0 => (artic.to_ascii_lowercase(), string, fret),
            s => (
                artic.to_ascii_lowercase(),
                0,
                i32::from(s) * string as i32 + fret,
            ),
        };
        let n = self.fret.rr_count.get(&key).copied().unwrap_or(0);
        let rr = fb
            .rr_orders
            .iter()
            .find(|o| usize::from(o.rr) == rr_n)
            .filter(|o| !o.order.is_empty())
            .map_or(n % rr_n, |o| usize::from(o.order[n % o.order.len()]));
        self.fret.rr_count.insert(key, n + 1);

        // The recording: this fret's, or a neighbour's retuned to it.
        let frets = fb.frets;
        let borrow_frets = fb.borrow.as_ref().map_or(0, |b| b.frets);
        let borrowed = self.fret.draw(fb).is_some_and(|d| {
            fb.borrow
                .as_ref()
                .is_some_and(|b| d % 1000 + 1 <= b.chance_permille)
        });
        let zone_key = if borrowed {
            let up = i32::from(note) + i32::from(borrow_frets);
            if up <= i32::from(open) + i32::from(frets) {
                up
            } else {
                i32::from(note) - i32::from(borrow_frets)
            }
        } else {
            i32::from(note)
        };
        let Some(idx) = u8::try_from(zone_key)
            .ok()
            .and_then(|k| self.find_fret_zone(artic, string, &dynamic, rr, k))
        else {
            self.fret_miss(note, artic, dynamic, rr);
            return;
        };
        // Spawned under the played key: a neighbour's recording plays at
        // the played pitch (Kontakt: ±2 frets, `change_tune` ∓200 cents).
        let start = self.fret_offset_frames(idx, offset_ms);
        let hold = self.fret_onset_hold();
        self.spawn_script_offset = true;
        self.spawn_extra_hold += hold;
        self.spawn_zone_voice_at(
            idx,
            note,
            VoiceKind::Zoned,
            db_to_gain(gain_db),
            None,
            0.0,
            start,
        );
        self.spawn_extra_hold -= hold;
        self.spawn_script_offset = false;
    }

    /// A lead-in (see `FretLeadInSpec`): its own play takes a borrow draw
    /// it never uses, then its round robin is the next draw mod its RR
    /// count (Shreddage pops, `s3___play_event` L7952, L7963).
    fn play_fretted_lead(&mut self, lead: &str, string: usize, note: u8, velocity: u8) {
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return;
        };
        let Some(a) = self.patch.spec.articulation(lead) else {
            return;
        };
        let rr_n = a.rr.max(1);
        let gain_db = a
            .vel_gain_db
            .get(usize::from(velocity.min(127)))
            .copied()
            .unwrap_or(0.0);
        let _unused_borrow = self.fret.draw(fb);
        let rr = self.fret.draw(fb).map_or(0, |d| d as usize % rr_n);
        let Some(idx) = self.find_fret_zone(lead, string, "", rr, note) else {
            self.fret_miss(note, lead, String::new(), rr);
            return;
        };
        let hold = self.fret_onset_hold();
        self.spawn_extra_hold += hold;
        self.spawn_zone_voice(idx, note, VoiceKind::Zoned, db_to_gain(gain_db), None, 0.0);
        self.spawn_extra_hold -= hold;
    }

    /// `FretboardSpec::onset_delay_us` in whole frames (floored).
    fn fret_onset_hold(&self) -> usize {
        self.patch.spec.fretboard.as_ref().map_or(0, |fb| {
            (f64::from(fb.onset_delay_us) * 1e-6 * f64::from(self.sample_rate)).floor() as usize
        })
    }

    /// Fire every lead-in body that is due: if its key still holds the
    /// string, the lead fades and the body enters (`FretLeadInSpec`).
    pub(crate) fn fire_fret_due(&mut self) {
        let now = self.frames_rendered;
        let mut i = 0;
        while i < self.fret.due.len() {
            if self.fret.due[i].at > now {
                i += 1;
                continue;
            }
            let d = self.fret.due.remove(i);
            let held = self.fret.ringing.get(d.string) == Some(&Some(d.note))
                && self.fret.note_string.get(&d.note) == Some(&d.string);
            if !held {
                continue;
            }
            self.set_active_line(usize::from(d.line));
            let fade = ms_to_frames_f(d.lead.lead_fade_ms, self.sample_rate);
            self.voices.fade_out_note_line_any(d.line, d.note, fade);
            let artic = d.lead.articulation.clone();
            self.play_fretted(&artic, d.string, d.note, d.velocity, d.lead.body_offset_ms);
        }
    }

    /// Frames until the next lead-in body is due (for the block split).
    pub(crate) fn fret_due_in(&self) -> Option<usize> {
        self.fret
            .due
            .iter()
            .map(|d| d.at.saturating_sub(self.frames_rendered) as usize)
            .min()
    }

    /// `ms` of `idx`'s own recording, in its file's frames.
    fn fret_offset_frames(&self, idx: usize, ms: f32) -> usize {
        if ms <= 0.0 {
            return 0;
        }
        let rate = self
            .cache
            .get_loaded(&self.patch.zone_paths[idx])
            .map_or(self.sample_rate, |d| d.sample_rate);
        (f64::from(ms) / 1000.0 * f64::from(rate)).round() as usize
    }

    fn fret_miss(&mut self, note: u8, artic: &str, dynamic: String, rr: usize) {
        self.trace_push(TraceKind::SampleMiss {
            note,
            articulation: artic.to_string(),
            dynamic,
            rr,
            reason: MissReason::NoSample,
        });
    }

    /// Note-off of a fretted note. `false` = not one.
    pub(crate) fn fretted_note_off(&mut self, note: u8) -> bool {
        let Some(string) = self.fret.note_string.remove(&note) else {
            return false;
        };
        if self.fret.ringing[string] == Some(note) {
            self.fret.ringing[string] = None;
        }
        let frames = self.fretted_release_frames();
        let line = self.cur_line as u8;
        self.voices.note_off_line(line, note, Some(frames));
        true
    }

    /// The articulation's release: its amp envelope's release segment.
    fn fretted_release_frames(&self) -> usize {
        let ms = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| {
                a.amp_env_release
                    .and_then(|i| a.amp_env.get(i as usize))
                    .map(|s| s.time_ms)
            })
            .unwrap_or(self.patch.spec.performance.sustain_noteoff_ms as f32);
        ms_to_frames_f(ms, self.sample_rate).max(1)
    }

    /// The zone of `artic` on `string` (0-based) at `key`, in `dynamic`
    /// (empty = any) and round robin `rr`.
    fn find_fret_zone(
        &self,
        artic: &str,
        string: usize,
        dynamic: &str,
        rr: usize,
        key: u8,
    ) -> Option<usize> {
        let cands = self.zone_keys.get(usize::from(key))?;
        cands.iter().map(|&i| i as usize).find(|&i| {
            let z = &self.patch.spec.zones[i];
            usize::from(z.string) == string + 1
                && z.rr_index as usize == rr
                && self.zone_passes(z, artic, "", dynamic)
        })
    }
}

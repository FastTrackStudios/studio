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
use crate::spec::{FretboardSpec, LibrarySpec};

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

    /// The borrow draw: `true` when this note plays a neighbour.
    fn borrows(&mut self, fb: &FretboardSpec) -> bool {
        let Some(b) = &fb.borrow else {
            return false;
        };
        self.lcg = b.multiplier.wrapping_mul(self.lcg).wrapping_add(1);
        let draw = ((self.lcg >> 8) & 0x00FF_FFFF) % 1000 + 1;
        draw <= b.chance_permille
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
        let artic = self.articulation.to_ascii_lowercase();
        if !self.fret.fretted.contains(&artic) {
            return false;
        }
        let Some(string) = self.fret.place(fb, note) else {
            return true;
        };
        let open = fb.strings[string];
        let fret = i32::from(note) - i32::from(open);
        let (frets, retrigger_ms) = (fb.frets, fb.retrigger_fade_ms);
        let borrow_frets = fb.borrow.as_ref().map_or(0, |b| b.frets);

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

        let Some(a) = self.patch.spec.articulation(&self.articulation) else {
            return true;
        };
        let dynamic = Self::short_band(a, velocity)
            .and_then(|(band, ..)| a.dynamics.get(band).cloned())
            .unwrap_or_default();
        let rr_n = a.rr.max(1);
        let gain_db = a
            .vel_gain_db
            .get(usize::from(velocity.min(127)))
            .copied()
            .unwrap_or(0.0);
        let key = (artic, string, fret);
        let n = self.fret.rr_count.get(&key).copied().unwrap_or(0);
        let rr = self
            .patch
            .spec
            .fretboard
            .as_ref()
            .and_then(|fb| fb.rr_orders.iter().find(|o| usize::from(o.rr) == rr_n))
            .filter(|o| !o.order.is_empty())
            .map_or(n % rr_n, |o| usize::from(o.order[n % o.order.len()]));
        self.fret.rr_count.insert(key, n + 1);

        // The recording: this fret's, or a neighbour's retuned to it.
        let borrowed = self
            .patch
            .spec
            .fretboard
            .as_ref()
            .is_some_and(|fb| self.fret.borrows(fb));
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
            .and_then(|k| self.find_fret_zone(&self.articulation.clone(), string, &dynamic, rr, k))
        else {
            self.trace_push(TraceKind::SampleMiss {
                note,
                articulation: self.articulation.clone(),
                dynamic,
                rr,
                reason: MissReason::NoSample,
            });
            return true;
        };
        // Spawned under the played key: a neighbour's recording plays at
        // the played pitch (Kontakt: ±2 frets, `change_tune` ∓200 cents).
        self.spawn_zone_voice(idx, note, VoiceKind::Zoned, db_to_gain(gain_db), None, 0.0);

        let st = &mut self.fret;
        st.ringing[string] = Some(note);
        st.note_string.insert(note, string);
        st.last_string = Some(string);
        st.last_fret = fret;
        st.last_frets[string] = fret;
        true
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

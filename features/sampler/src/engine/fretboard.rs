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

const MISSING: &str = "";

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
    /// Each held note: where and what it plays.
    held: HashMap<u8, Held>,
    /// Notes played per (articulation, string, fret): the RR order's index.
    rr_count: HashMap<(String, usize, i32), usize>,
    /// The borrow generator's state and multiplier (`None` = no borrow).
    lcg: u32,
    lcg_mult: Option<u32>,
    /// Script steps waiting for their time.
    pub(crate) due: Vec<FretDue>,
}

/// A note on the fretboard.
#[derive(Debug, Clone)]
pub(crate) struct Held {
    string: usize,
    fret: i32,
    /// The key of the recording it plays (a borrow's neighbour).
    zone_key: u8,
    artic: String,
    /// Its velocity gain (dB).
    gain_db: f32,
    /// When it started (frame).
    start: u64,
    line: u8,
}

/// A script step due at frame `at`.
#[derive(Debug, Clone)]
pub(crate) struct FretDue {
    at: u64,
    kind: DueKind,
}

#[derive(Debug, Clone)]
pub(crate) enum DueKind {
    /// A lead-in's body (see `FretLeadInSpec`).
    Body {
        note: u8,
        velocity: u8,
        string: usize,
        lead: FretLeadInSpec,
        line: u8,
    },
    /// A note let go: its voices release and it may leave a release
    /// recording (`FretReleaseSpec`).
    Release { note: u8, held: Held },
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
            held: HashMap::new(),
            rr_count: HashMap::new(),
            lcg: fb.borrow.as_ref().map_or(0, |b| b.seed),
            lcg_mult: fb.borrow.as_ref().map(|b| b.multiplier),
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
    fn draw_raw(&mut self) -> Option<u32> {
        let m = self.lcg_mult?;
        self.lcg = m.wrapping_mul(self.lcg).wrapping_add(1);
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
        let line = self.cur_line as u8;

        // What rings on this string decides a hammer, then is let go: a
        // repeat of its note fades, another note releases it.
        let ringing = self.fret.ringing[string].take();
        let hammer = ringing.and_then(|old| {
            let h = fb.hammer.as_ref()?;
            let src = self.fret.held.get(&old)?;
            let dist = i32::from(note) - i32::from(old);
            (old != note
                && velocity <= h.max_velocity
                && dist.unsigned_abs() <= u32::from(h.range)
                && h.sources.iter().any(|s| s.eq_ignore_ascii_case(&src.artic))
                && h.from.iter().any(|s| s.eq_ignore_ascii_case(&artic)))
            .then(|| (h.articulation.clone(), dist))
        });
        if let Some(old) = ringing {
            if old == note {
                let frames = ms_to_frames_f(retrigger_ms, self.sample_rate);
                self.fret.held.remove(&old);
                self.voices.fade_out_note_line_any(line, old, frames);
            } else {
                self.fret_let_go(old);
            }
        }

        let played = match (hammer, lead_in) {
            (Some((h, dist)), _) => {
                let dir = if dist >= 0 { "up" } else { "down" };
                self.play_fretted(&h, dir, string, note, velocity, 0.0)
                    .map(|zk| (h, zk))
            }
            // The lead first (Shreddage's pop); the body follows when due.
            (None, Some(l)) => {
                let zk = self.play_fretted_lead(&l.lead, string, note, velocity);
                let at =
                    self.frames_rendered + ms_to_frames_f(l.body_delay_ms, self.sample_rate) as u64;
                self.fret.due.push(FretDue {
                    at,
                    kind: DueKind::Body {
                        note,
                        velocity,
                        string,
                        lead: l,
                        line,
                    },
                });
                // no lead recording (Abyss string 1 has no pop above key
                // 63): the note still holds its string and its body enters
                Some((artic.clone(), zk.unwrap_or(note)))
            }
            (None, None) => self
                .play_fretted(&artic, MISSING, string, note, velocity, 0.0)
                .map(|zk| (artic.clone(), zk)),
        };

        let gain_db = self.fret_vel_gain(
            &played.as_ref().map_or(artic.clone(), |p| p.0.clone()),
            velocity,
        );
        let st = &mut self.fret;
        st.ringing[string] = Some(note);
        if let Some((a, zone_key)) = played {
            st.held.insert(
                note,
                Held {
                    string,
                    fret,
                    zone_key,
                    artic: a,
                    gain_db,
                    start: self.frames_rendered,
                    line,
                },
            );
        }
        st.last_string = Some(string);
        st.last_fret = fret;
        st.last_frets[string] = fret;
        true
    }

    /// The articulation's velocity gain (dB) plus its hammer offset when
    /// it is the hammer articulation.
    fn fret_vel_gain(&self, artic: &str, velocity: u8) -> f32 {
        let vel = self
            .patch
            .spec
            .articulation(artic)
            .and_then(|a| a.vel_gain_db.get(usize::from(velocity.min(127))).copied())
            .unwrap_or(0.0);
        let hammer = self
            .patch
            .spec
            .fretboard
            .as_ref()
            .and_then(|fb| fb.hammer.as_ref())
            .filter(|h| h.articulation.eq_ignore_ascii_case(artic))
            .map_or(0.0, |h| h.gain_db);
        vel + hammer
    }

    /// Let `note` go now: it leaves the board, and its release (with any
    /// release recording) happens after the onset delay, as the script's
    /// `release_event` runs after its `wait`.
    fn fret_let_go(&mut self, note: u8) {
        if let Some(held) = self.fret.held.remove(&note) {
            if self.fret.ringing.get(held.string) == Some(&Some(note)) {
                self.fret.ringing[held.string] = None;
            }
            let at = self.frames_rendered + self.fret_onset_hold() as u64;
            self.fret.due.push(FretDue {
                at,
                kind: DueKind::Release { note, held },
            });
        }
    }

    /// Play `artic` (zones in `direction`, empty = any) on `string`
    /// (0-based) at `note`: its velocity layer, the position's next round
    /// robin, the borrow draw, then the zone, started `offset_ms` into its
    /// recording, at its velocity gain (`fret_vel_gain`). Returns the key
    /// of the recording it played.
    fn play_fretted(
        &mut self,
        artic: &str,
        direction: &str,
        string: usize,
        note: u8,
        velocity: u8,
        offset_ms: f32,
    ) -> Option<u8> {
        let fb = self.patch.spec.fretboard.as_ref()?;
        let a = self.patch.spec.articulation(artic)?;
        let open = fb.strings[string];
        let fret = i32::from(note) - i32::from(open);
        let dynamic = Self::short_band(a, velocity)
            .and_then(|(band, ..)| a.dynamics.get(band).cloned())
            .unwrap_or_default();
        let rr_n = a.rr.max(1);
        let rr = self.fret_next_rr(artic, rr_n, string, fret, true);
        let gain_db = self.fret_vel_gain(artic, velocity);

        // The recording: this fret's, or a neighbour's retuned to it.
        let fb = self.patch.spec.fretboard.as_ref()?;
        let frets = fb.frets;
        let borrow_frets = fb.borrow.as_ref().map_or(0, |b| b.frets);
        let chance = fb.borrow.as_ref().map(|b| b.chance_permille);
        let borrowed = match chance {
            Some(c) => self.fret.draw_raw().is_some_and(|d| d % 1000 + 1 <= c),
            None => false,
        };
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
        let zone_key = u8::try_from(zone_key).ok()?;
        let Some(idx) = self.find_fret_zone(artic, direction, string, &dynamic, rr, zone_key)
        else {
            self.fret_miss(note, artic, dynamic, rr);
            return None;
        };
        // Spawned under the played key: a neighbour's recording plays at
        // the played pitch (Kontakt: ±2 frets, `change_tune` ∓200 cents).
        let start = self.fret_offset_frames(idx, offset_ms);
        self.fret_spawn(idx, note, VoiceKind::Zoned, gain_db, start);
        Some(zone_key)
    }

    /// The position's next round robin of `artic` (RR orders, slot
    /// stride); `step` = advance its counter.
    fn fret_next_rr(
        &mut self,
        artic: &str,
        rr_n: usize,
        string: usize,
        fret: i32,
        step: bool,
    ) -> usize {
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return 0;
        };
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
        if step {
            self.fret.rr_count.insert(key, n + 1);
        }
        rr
    }

    /// Spawn zone `idx` under `note` after the onset delay.
    fn fret_spawn(&mut self, idx: usize, note: u8, kind: VoiceKind, gain_db: f32, start: usize) {
        let hold = self.fret_onset_hold();
        self.spawn_script_offset = true;
        self.spawn_extra_hold += hold;
        self.spawn_zone_voice_at(idx, note, kind, db_to_gain(gain_db), None, 0.0, start);
        self.spawn_extra_hold -= hold;
        self.spawn_script_offset = false;
    }

    /// A lead-in (see `FretLeadInSpec`): its own play takes a borrow draw
    /// it never uses, then its round robin is the next draw mod its RR
    /// count (Shreddage pops, `s3___play_event` L7952, L7963).
    fn play_fretted_lead(
        &mut self,
        lead: &str,
        string: usize,
        note: u8,
        velocity: u8,
    ) -> Option<u8> {
        let a = self.patch.spec.articulation(lead)?;
        let rr_n = a.rr.max(1);
        let gain_db = self.fret_vel_gain(lead, velocity);
        let _unused_borrow = self.fret.draw_raw();
        let rr = self.fret.draw_raw().map_or(0, |d| d as usize % rr_n);
        let Some(idx) = self.find_fret_zone(lead, MISSING, string, "", rr, note) else {
            self.fret_miss(note, lead, String::new(), rr);
            return None;
        };
        self.fret_spawn(idx, note, VoiceKind::Zoned, gain_db, 0);
        Some(note)
    }

    /// The release recording a let-go note leaves, if any
    /// (`FretReleaseSpec`; the script's `release_event`, L6946-6990).
    fn play_fret_release(&mut self, note: u8, held: &Held) {
        let Some(rl) = self
            .patch
            .spec
            .fretboard
            .as_ref()
            .and_then(|fb| fb.release_layer.clone())
        else {
            return;
        };
        if !rl
            .sources
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&held.artic))
        {
            return;
        }
        let age_ms = (self.frames_rendered.saturating_sub(held.start)) as f32 * 1000.0
            / self.sample_rate as f32;
        if age_ms < rl.min_held_ms || age_ms > rl.max_held_ms {
            return;
        }
        let rr_n = self
            .patch
            .spec
            .articulation(&rl.articulation)
            .map_or(1, |a| a.rr.max(1));
        let step = rl.open_string_counts || held.fret > 0;
        let rr = self.fret_next_rr(&rl.articulation, rr_n, held.string, held.fret, step);
        let Some(idx) = self.find_fret_zone(
            &rl.articulation,
            MISSING,
            held.string,
            "",
            rr,
            held.zone_key,
        ) else {
            self.fret_miss(note, &rl.articulation, String::new(), rr);
            return;
        };
        // No onset hold: the release step already waited it out.
        self.spawn_zone_voice(
            idx,
            note,
            VoiceKind::Short,
            db_to_gain(held.gain_db),
            None,
            0.0,
        );
    }

    /// `FretboardSpec::onset_delay_us` in whole frames (floored).
    fn fret_onset_hold(&self) -> usize {
        self.patch.spec.fretboard.as_ref().map_or(0, |fb| {
            (f64::from(fb.onset_delay_us) * 1e-6 * f64::from(self.sample_rate)).floor() as usize
        })
    }

    /// Run every script step that is due.
    pub(crate) fn fire_fret_due(&mut self) {
        let now = self.frames_rendered;
        let mut i = 0;
        while i < self.fret.due.len() {
            if self.fret.due[i].at > now {
                i += 1;
                continue;
            }
            match self.fret.due.remove(i).kind {
                // The body enters if the key still holds the string.
                DueKind::Body {
                    note,
                    velocity,
                    string,
                    lead,
                    line,
                } => {
                    let held = self.fret.ringing.get(string) == Some(&Some(note))
                        && self
                            .fret
                            .held
                            .get(&note)
                            .is_some_and(|h| h.string == string);
                    if !held {
                        continue;
                    }
                    self.set_active_line(usize::from(line));
                    let fade = ms_to_frames_f(lead.lead_fade_ms, self.sample_rate);
                    self.voices.fade_out_note_line_any(line, note, fade);
                    let artic = lead.articulation.clone();
                    if let Some(zk) = self.play_fretted(
                        &artic,
                        MISSING,
                        string,
                        note,
                        velocity,
                        lead.body_offset_ms,
                    ) {
                        if let Some(h) = self.fret.held.get_mut(&note) {
                            h.artic = artic;
                            h.zone_key = zk;
                        }
                    }
                }
                DueKind::Release { note, held } => {
                    self.set_active_line(usize::from(held.line));
                    let frames = self.fretted_release_frames(&held.artic);
                    self.voices.note_off_line(held.line, note, Some(frames));
                    self.play_fret_release(note, &held);
                }
            }
        }
    }

    /// Frames until the next script step is due (for the block split).
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
        // Kontakt floors a `play_note` offset to whole source frames
        // (15 ms at 44.1 kHz starts at 661, KONTRA's trace)
        (f64::from(ms) / 1000.0 * f64::from(rate)).floor() as usize
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
        if !self.fret.held.contains_key(&note) {
            return false;
        }
        self.fret_let_go(note);
        true
    }

    /// `artic`'s release time (`release_ms_by_held`; the voice's own
    /// note-off fade runs it, on `release_curve_k`). The authored envelope
    /// must not carry a release too: a released flex envelope plays its
    /// remaining segments, so both would fade the note.
    fn fretted_release_frames(&self, artic: &str) -> usize {
        let ms = self
            .patch
            .spec
            .articulation(artic)
            .and_then(|a| a.release_ms_by_held.first().map(|p| p[1]))
            .unwrap_or(self.patch.spec.performance.sustain_noteoff_ms as f32);
        ms_to_frames_f(ms, self.sample_rate).max(1)
    }

    /// The zone of `artic` on `string` (0-based) at `key`, in `direction`
    /// and `dynamic` (empty = any) and round robin `rr`.
    fn find_fret_zone(
        &self,
        artic: &str,
        direction: &str,
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
                && self.zone_passes(z, artic, direction, dynamic)
        })
    }
}

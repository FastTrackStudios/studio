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
    /// The slide key is held (`FretSlideSpec::key`).
    slide_held: bool,
    /// Keys still down whose note slid away to another.
    gone: HashSet<u8>,
    /// The open poly window: its notes in arrival order.
    window: Option<FretWindow>,
    /// Keys sounding a free articulation, and which.
    free: HashMap<u8, String>,
}

/// Notes struck within one poly window: (note, velocity, articulation, line).
#[derive(Debug, Clone, Default)]
pub(crate) struct FretWindow {
    notes: Vec<(u8, u8, String, u8)>,
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
    /// Its slide chain: where the chain began (frame), the first note's
    /// velocity, and this note's place in the chain (0 = not slid to).
    origin_start: u64,
    origin_velocity: u8,
    slide_index: u32,
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
    /// The poly window closes: its notes play.
    Window,
    /// A slide begins: the source fades, the transition plays.
    SlideStart {
        note: u8,
        from: u8,
        string: usize,
        interval: i32,
        gain_db: f32,
        line: u8,
    },
    /// A slide's destination enters (if its key still holds the string).
    SlideBody {
        note: u8,
        string: usize,
        layer_velocity: u8,
        gain_db: f32,
        line: u8,
    },
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
            slide_held: false,
            gone: HashSet::new(),
            window: None,
            free: HashMap::new(),
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
        // The slide key only arms slides.
        if fb.slide.as_ref().is_some_and(|s| s.key == note) {
            self.fret.slide_held = true;
            return true;
        }
        let artic = self.articulation.clone();
        if fb
            .free_articulations
            .iter()
            .any(|a| a.eq_ignore_ascii_case(&artic))
        {
            self.fret_play_free(&artic, note, velocity);
            return true;
        }
        if !self.fret.fretted.contains(&artic.to_ascii_lowercase()) {
            return false;
        }
        // Every note waits out the poly window (the script's
        // `activate_latency`, `wait(50)` µs at latency 0): notes struck
        // within it are voiced together (`str__voicing`, L16594-16990).
        let line = self.cur_line as u8;
        let now = self.frames_rendered;
        match &mut self.fret.window {
            Some(w) if w.notes.len() < 6 => w.notes.push((note, velocity, artic, line)),
            Some(_) => {}
            None => {
                let at = now + self.fret_onset_hold() as u64;
                self.fret.window = Some(FretWindow {
                    notes: vec![(note, velocity, artic, line)],
                });
                self.fret.due.push(FretDue {
                    at,
                    kind: DueKind::Window,
                });
            }
        }
        true
    }

    /// Play the notes of a closed poly window: alone, a note is placed as
    /// usual; together, each takes its string from the chord's voicing.
    fn fret_close_window(&mut self) {
        let Some(w) = self.fret.window.take() else {
            return;
        };
        let voicing = if w.notes.len() > 1 {
            self.fret_voicing(&w.notes)
        } else {
            Vec::new()
        };
        let mut taken = vec![false; voicing.len()];
        self.fret_no_hold = true;
        for (note, velocity, artic, line) in w.notes {
            self.set_active_line(usize::from(line));
            self.articulation.clone_from(&artic);
            let forced = if voicing.is_empty() {
                None
            } else {
                let fb = self.patch.spec.fretboard.as_ref();
                let s = (0..voicing.len()).find(|&s| {
                    !taken[s]
                        && fb.is_some_and(|fb| {
                            voicing[s] == i32::from(note) - i32::from(fb.strings[s])
                        })
                });
                match s {
                    Some(s) => {
                        taken[s] = true;
                        Some(s)
                    }
                    None => continue,
                }
            };
            self.fretted_note_now(note, velocity, &artic, forced);
        }
        self.fret_no_hold = false;
    }

    /// The chord voicing the script picks for `notes` (sorted high to
    /// low): from each start string, fingerings that put every note on the
    /// first free string at or past it that it has not tried there, scored
    /// by reach, gaps and distance from the hand (L16687-16912). Per string,
    /// its fret or −1.
    fn fret_voicing(&self, notes: &[(u8, u8, String, u8)]) -> Vec<i32> {
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return Vec::new();
        };
        let ns = fb.strings.len();
        let n = notes.len();
        if n < 2 || n > ns {
            return Vec::new();
        }
        let mut sorted: Vec<i32> = notes.iter().map(|x| i32::from(x.0)).collect();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        let limit = i32::from(fb.frets);
        let hand = i32::from(fb.hand_fret);
        // the hand's reach: second-lowest open string − lowest − 1
        let size = if ns >= 2 {
            i32::from(fb.strings[ns - 2]) - i32::from(fb.strings[ns - 1]) - 1
        } else {
            0
        };
        let fits = |note: i32, k: usize| {
            let f = note - i32::from(fb.strings[k]);
            (0..=limit).contains(&f).then_some(f)
        };
        let (sliding, hammering) = self.fret_poly_flags(notes);
        let mut best: Option<(i32, Vec<i32>)> = None;
        'starts: for i in 0..=(ns - n) {
            let mut tested = vec![false; n * ns];
            loop {
                let mut fing = vec![-1i32; ns];
                let (mut min_f, mut max_f, mut assigned) = (25, -1, 0);
                for (j, &note) in sorted.iter().enumerate() {
                    for k in i..ns {
                        let Some(f) = fits(note, k) else { continue };
                        if fing[k] == -1 && !tested[j * ns + k] {
                            if f > 0 {
                                min_f = min_f.min(f);
                            }
                            max_f = max_f.max(f);
                            fing[k] = f;
                            tested[j * ns + k] = true;
                            assigned += 1;
                            break;
                        }
                    }
                }
                if assigned != n {
                    break;
                }
                let reach = max_f - min_f;
                // gap: a string unused between used ones
                let mut gap = -1;
                for &f in &fing {
                    if gap == -1 && f > -1 {
                        gap = 0;
                    }
                    if gap == 0 && f == -1 {
                        gap = 2;
                    }
                    if gap == 2 && f > -1 {
                        gap = 1;
                    }
                }
                if gap == 2 {
                    gap = 0;
                }
                let (slideable, hammerable) = self.fret_voicing_moves(&fing);
                let score = if (slideable && sliding) || (hammerable && hammering) {
                    10_000
                } else {
                    let s = if reach < size {
                        if gap == 0 { 2000 } else { 1000 - reach * 100 }
                    } else {
                        1000 - (size - reach).abs() * 100
                    };
                    s - (min_f - hand).abs() * 10
                };
                if best.as_ref().is_none_or(|(b, _)| score > *b) {
                    best = Some((score, fing));
                }
                if score == 10_000 {
                    break 'starts;
                }
            }
        }
        best.map(|(_, f)| f).unwrap_or_default()
    }

    /// Whether a fingering can slide / hammer from what rings: every used
    /// string ringing, and each a slide source within the widest slide /
    /// a hammer source within the hammer range of its fret.
    fn fret_voicing_moves(&self, fing: &[i32]) -> (bool, bool) {
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return (false, false);
        };
        let (mut slide, mut hammer) = (true, true);
        for (s, &f) in fing.iter().enumerate() {
            if f < 0 {
                continue;
            }
            let Some(h) = self.fret.ringing[s].and_then(|n| self.fret.held.get(&n)) else {
                return (false, false);
            };
            let d = (f - h.fret).abs();
            let is_src = |list: &[String]| list.iter().any(|a| a.eq_ignore_ascii_case(&h.artic));
            if !fb
                .slide
                .as_ref()
                .is_some_and(|sl| is_src(&sl.sources) && d > 0 && d <= i32::from(sl.max_interval))
            {
                slide = false;
            }
            if !fb
                .hammer
                .as_ref()
                .is_some_and(|hm| is_src(&hm.sources) && d >= 1 && d <= i32::from(hm.range))
            {
                hammer = false;
            }
        }
        (slide, hammer)
    }

    /// Whether any note of a window could slide (the slide key held, its
    /// articulation one that slides, a ringing slide source within reach)
    /// or hammer (its velocity a hammer's, a ringing hammer source within
    /// range) — the script's `use_poly_slide` / `use_poly_hammer`.
    fn fret_poly_flags(&self, notes: &[(u8, u8, String, u8)]) -> (bool, bool) {
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return (false, false);
        };
        let (mut sliding, mut hammering) = (false, false);
        for (note, vel, artic, _) in notes {
            for ringing in self.fret.ringing.iter().flatten() {
                let Some(h) = self.fret.held.get(ringing) else {
                    continue;
                };
                let d = (i32::from(*note) - i32::from(*ringing)).abs();
                if let Some(s) = &fb.slide {
                    if self.fret.slide_held
                        && s.from.iter().any(|a| a.eq_ignore_ascii_case(artic))
                        && s.sources.iter().any(|a| a.eq_ignore_ascii_case(&h.artic))
                        && d > 0
                        && d <= i32::from(s.max_interval)
                    {
                        sliding = true;
                    }
                }
                if let Some(hm) = &fb.hammer {
                    if *vel <= hm.max_velocity
                        && hm.from.iter().any(|a| a.eq_ignore_ascii_case(artic))
                        && hm.sources.iter().any(|a| a.eq_ignore_ascii_case(&h.artic))
                        && d >= 1
                        && d <= i32::from(hm.range)
                    {
                        hammering = true;
                    }
                }
            }
        }
        (sliding, hammering)
    }

    /// A note out of the poly window: slides, hammers, lead-ins and plain
    /// notes, on `forced` (its voicing's string) or the string it is placed on.
    fn fretted_note_now(&mut self, note: u8, velocity: u8, artic: &str, forced: Option<usize>) {
        let artic = artic.to_string();
        if forced.is_none() {
            if let Some((string, from, interval)) = self.fret_slide_source(note, &artic) {
                self.fretted_slide(note, velocity, &artic, string, from, interval);
                return;
            }
        }
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return;
        };
        let Some(string) = forced.or_else(|| self.fret.place(fb, note)) else {
            return;
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
        // Tapping (dispatched before a hammer): a hammer by the interval from
        // the ringing note, or up by the fret from an idle string; on an idle
        // open string it is `open_as`.
        let tap = fb
            .tap
            .as_ref()
            .filter(|t| t.articulation.eq_ignore_ascii_case(&artic))
            .map(|t| {
                let dist = ringing.map_or(fret, |old| i32::from(note) - i32::from(old));
                if fret == 0 && dist == 0 {
                    (t.open_as.clone(), MISSING)
                } else {
                    (
                        t.articulation.clone(),
                        if dist >= 0 { "up" } else { "down" },
                    )
                }
            });
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

        let hammer = if tap.is_some() { None } else { hammer };
        let played = match (hammer, lead_in) {
            _ if tap.is_some() => {
                let (a, dir) = tap.clone().unwrap_or_default();
                let g = self.fret_vel_gain(&a, velocity);
                self.play_fretted(&a, dir, 0, string, note, velocity, g, 0.0)
                    .map(|zk| (a, zk))
            }
            (Some((h, dist)), _) => {
                let dir = if dist >= 0 { "up" } else { "down" };
                let g = self.fret_vel_gain(&h, velocity);
                self.play_fretted(&h, dir, 0, string, note, velocity, g, 0.0)
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
                .play_fretted(
                    &artic,
                    MISSING,
                    0,
                    string,
                    note,
                    velocity,
                    self.fret_vel_gain(&artic, velocity),
                    0.0,
                )
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
                    origin_start: self.frames_rendered,
                    origin_velocity: velocity,
                    slide_index: 0,
                },
            );
        }
        st.last_string = Some(string);
        st.last_fret = fret;
        st.last_frets[string] = fret;
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
    #[allow(clippy::too_many_arguments)]
    fn play_fretted(
        &mut self,
        artic: &str,
        direction: &str,
        interval: u32,
        string: usize,
        note: u8,
        layer_velocity: u8,
        gain_db: f32,
        offset_ms: f32,
    ) -> Option<u8> {
        let velocity = layer_velocity;
        let fb = self.patch.spec.fretboard.as_ref()?;
        let a = self.patch.spec.articulation(artic)?;
        let open = fb.strings[string];
        let fret = i32::from(note) - i32::from(open);
        let dynamic = Self::short_band(a, velocity)
            .and_then(|(band, ..)| a.dynamics.get(band).cloned())
            .unwrap_or_default();
        let rr_n = a.rr.max(1);
        let rr = self.fret_next_rr(artic, rr_n, string, fret, true);

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
        let Some(idx) =
            self.find_fret_zone(artic, direction, interval, string, &dynamic, rr, zone_key)
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
        let Some(idx) = self.find_fret_zone(lead, MISSING, 0, string, "", rr, note) else {
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
            0,
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

    /// Where `note` slides from, if it slides (the slide key held, `artic`
    /// one that slides): `(string it plays on, ringing note, interval)`.
    /// First the nearest string ringing a slide source within the widest
    /// slide that `note` fits on, played there (`str___get_source_string`,
    /// L7699); failing that, the nearest such string whether `note` fits on
    /// it or not: `note` is placed as any other note and the transition's
    /// interval is the distance clamped to `fret − frets ..= fret` on its
    /// string (`str___string_selection`, L7653-7720).
    fn fret_slide_source(&self, note: u8, artic: &str) -> Option<(usize, u8, i32)> {
        let fb = self.patch.spec.fretboard.as_ref()?;
        let s = fb.slide.as_ref()?;
        if !self.fret.slide_held || !s.from.iter().any(|a| a.eq_ignore_ascii_case(artic)) {
            return None;
        }
        let fits = |string: usize| {
            let fret = i32::from(note) - i32::from(fb.strings[string]);
            (0..=i32::from(fb.frets)).contains(&fret)
        };
        let nearest = |need_fit: bool| {
            let mut best: Option<(usize, u8, i32)> = None;
            for (string, ringing) in self.fret.ringing.iter().enumerate() {
                let Some(old) = *ringing else { continue };
                let Some(h) = self.fret.held.get(&old) else {
                    continue;
                };
                if !s.sources.iter().any(|a| a.eq_ignore_ascii_case(&h.artic)) {
                    continue;
                }
                let d = i32::from(note) - i32::from(old);
                if d == 0
                    || d.unsigned_abs() > u32::from(s.max_interval)
                    || (need_fit && !fits(string))
                {
                    continue;
                }
                if best.is_none_or(|(_, _, bd)| d.abs() < bd.abs()) {
                    best = Some((string, old, d));
                }
            }
            best
        };
        if let Some(hit) = nearest(true) {
            return Some(hit);
        }
        let (_, old, d) = nearest(false)?;
        let string = self.fret.place(fb, note)?;
        let fret = i32::from(note) - i32::from(fb.strings[string]);
        Some((string, old, d.clamp(fret - i32::from(fb.frets), fret)))
    }

    /// `note` slides from `from` on `string`: it takes the string now; the
    /// source fades and the transition plays after the onset delay, the
    /// destination after the interval's time.
    fn fretted_slide(
        &mut self,
        note: u8,
        velocity: u8,
        artic: &str,
        string: usize,
        from: u8,
        interval: i32,
    ) {
        let Some(src) = self.fret.held.remove(&from) else {
            return;
        };
        if self.fret.ringing.get(src.string) == Some(&Some(from)) {
            self.fret.ringing[src.string] = None;
        }
        // a note already on the string it plays on is let go
        if let Some(old) = self.fret.ringing[string].take() {
            self.fret_let_go(old);
        }
        let Some(fb) = self.patch.spec.fretboard.as_ref() else {
            return;
        };
        let fret = i32::from(note) - i32::from(fb.strings[string]);
        let age_db = fb.slide.as_ref().map_or(0.0, |s| s.age_db_per_s);
        let line = self.cur_line as u8;
        let now = self.frames_rendered;
        let hold = self.fret_onset_hold() as u64;
        // Both are levelled by the new note's velocity law under the
        // source's articulation, plus the origin's age in whole ms × the
        // chain step (`[29]·int(−1000·age_s)·slide_index`, L17210).
        let index = src.slide_index + 1;
        let age_ms = ((now + hold).saturating_sub(src.origin_start) * 1000
            / u64::from(self.sample_rate)) as f32;
        let gain_db =
            self.fret_vel_gain(&src.artic, velocity) + age_db * age_ms / 1000.0 * index as f32;
        self.fret.gone.insert(from);
        self.fret.ringing[string] = Some(note);
        self.fret.held.insert(
            note,
            Held {
                string,
                fret,
                zone_key: note,
                artic: artic.to_string(),
                gain_db,
                start: now,
                line,
                origin_start: src.origin_start,
                origin_velocity: src.origin_velocity,
                slide_index: index,
            },
        );
        self.fret.last_string = Some(string);
        self.fret.last_fret = fret;
        self.fret.last_frets[string] = fret;
        self.fret.due.push(FretDue {
            at: now + hold,
            kind: DueKind::SlideStart {
                note,
                from,
                string,
                interval,
                gain_db,
                line,
            },
        });
    }

    /// A slide's first step: the source fades, the transition plays.
    fn fret_slide_start(
        &mut self,
        note: u8,
        from: u8,
        string: usize,
        interval: i32,
        gain_db: f32,
        line: u8,
    ) {
        let Some(s) = self
            .patch
            .spec
            .fretboard
            .as_ref()
            .and_then(|fb| fb.slide.clone())
        else {
            return;
        };
        self.set_active_line(usize::from(line));
        let fade = ms_to_frames_f(s.source_fade_ms, self.sample_rate);
        self.voices.fade_out_note_line_any(line, from, fade);
        let fret = self.fret.held.get(&note).map_or(0, |h| h.fret);
        let rr_n = self
            .patch
            .spec
            .articulation(&s.articulation)
            .map_or(1, |a| a.rr.max(1));
        let rr = self.fret_next_rr(&s.articulation, rr_n, string, fret, false);
        let _unused_borrow = self.fret.draw_raw();
        let dir = if interval > 0 { "up" } else { "down" };
        let iv = interval.unsigned_abs();
        match self.find_fret_zone(&s.articulation, dir, iv, string, "", rr, note) {
            Some(idx) => {
                self.spawn_fade_in = ms_to_frames_f(s.transition_fade_in_ms, self.sample_rate);
                self.spawn_zone_voice(idx, note, VoiceKind::Zoned, db_to_gain(gain_db), None, 0.0);
                self.spawn_fade_in = 0;
            }
            None => self.fret_miss(note, &s.articulation, String::new(), rr),
        }
        let us = s
            .time_us
            .get(iv.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(0);
        let layer_velocity = self.fret.held.get(&note).map_or(64, |h| h.origin_velocity);
        let at = self.frames_rendered + u64::from(us) * u64::from(self.sample_rate) / 1_000_000;
        self.fret.due.push(FretDue {
            at,
            kind: DueKind::SlideBody {
                note,
                string,
                layer_velocity,
                gain_db,
                line,
            },
        });
    }

    /// A slide's second step: the transition fades, the destination
    /// enters (if its key still holds the string).
    fn fret_slide_body(
        &mut self,
        note: u8,
        string: usize,
        layer_velocity: u8,
        gain_db: f32,
        line: u8,
    ) {
        let Some(s) = self
            .patch
            .spec
            .fretboard
            .as_ref()
            .and_then(|fb| fb.slide.clone())
        else {
            return;
        };
        let holds = self.fret.ringing.get(string) == Some(&Some(note))
            && self
                .fret
                .held
                .get(&note)
                .is_some_and(|h| h.string == string);
        if !holds {
            return;
        }
        self.set_active_line(usize::from(line));
        let fade = ms_to_frames_f(s.transition_fade_out_ms, self.sample_rate);
        self.voices.fade_out_note_line_any(line, note, fade);
        let artic = self
            .fret
            .held
            .get(&note)
            .map(|h| h.artic.clone())
            .unwrap_or_default();
        self.spawn_fade_in = ms_to_frames_f(s.body_fade_in_ms, self.sample_rate);
        self.fret_no_hold = true;
        let zk = self.play_fretted(
            &artic,
            MISSING,
            0,
            string,
            note,
            layer_velocity,
            gain_db,
            s.body_offset_ms,
        );
        self.fret_no_hold = false;
        self.spawn_fade_in = 0;
        if let (Some(zk), Some(h)) = (zk, self.fret.held.get_mut(&note)) {
            h.zone_key = zk;
        }
    }

    /// A free articulation (FX): the zone at `note`, at its velocity gain,
    /// at once (no window, no draw), released with its key (`play_note`
    /// duration −1).
    fn fret_play_free(&mut self, artic: &str, note: u8, velocity: u8) {
        let gain_db = self.fret_vel_gain(artic, velocity);
        let Some(cands) = self.zone_keys.get(usize::from(note)) else {
            return;
        };
        let idx = cands
            .iter()
            .map(|&i| i as usize)
            .find(|&i| self.zone_passes(&self.patch.spec.zones[i], artic, "", ""));
        match idx {
            Some(idx) => {
                self.spawn_zone_voice(idx, note, VoiceKind::Zoned, db_to_gain(gain_db), None, 0.0);
                self.fret.free.insert(note, artic.to_string());
            }
            None => self.fret_miss(note, artic, String::new(), 0),
        }
    }

    /// `FretboardSpec::onset_delay_us` in whole frames (floored).
    fn fret_onset_hold(&self) -> usize {
        if self.fret_no_hold {
            return 0;
        }
        self.patch.spec.fretboard.as_ref().map_or(0, |fb| {
            (f64::from(fb.onset_delay_us) * 1e-6 * f64::from(self.sample_rate)).floor() as usize
        })
    }

    /// Run every script step that is due.
    pub(crate) fn fire_fret_due(&mut self) {
        // a due step already waited its delay: nothing it plays waits again
        let outer = std::mem::replace(&mut self.fret_no_hold, true);
        self.fire_fret_due_inner();
        self.fret_no_hold = outer;
    }

    fn fire_fret_due_inner(&mut self) {
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
                    let g = self.fret_vel_gain(&artic, velocity);
                    if let Some(zk) = self.play_fretted(
                        &artic,
                        MISSING,
                        0,
                        string,
                        note,
                        velocity,
                        g,
                        lead.body_offset_ms,
                    ) {
                        if let Some(h) = self.fret.held.get_mut(&note) {
                            h.artic = artic;
                            h.zone_key = zk;
                        }
                    }
                }
                DueKind::SlideStart {
                    note,
                    from,
                    string,
                    interval,
                    gain_db,
                    line,
                } => {
                    self.fret_slide_start(note, from, string, interval, gain_db, line);
                }
                DueKind::SlideBody {
                    note,
                    string,
                    layer_velocity,
                    gain_db,
                    line,
                } => {
                    self.fret_slide_body(note, string, layer_velocity, gain_db, line);
                }
                DueKind::Window => self.fret_close_window(),
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
        if self
            .patch
            .spec
            .fretboard
            .as_ref()
            .and_then(|fb| fb.slide.as_ref())
            .is_some_and(|s| s.key == note)
        {
            self.fret.slide_held = false;
            return true;
        }
        if self.fret.gone.remove(&note) {
            return true;
        }
        if let Some(artic) = self.fret.free.remove(&note) {
            let frames = self.fretted_release_frames(&artic);
            let line = self.cur_line as u8;
            self.voices.note_off_line(line, note, Some(frames));
            return true;
        }
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
    #[allow(clippy::too_many_arguments)]
    fn find_fret_zone(
        &self,
        artic: &str,
        direction: &str,
        interval: u32,
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
                && (interval == 0 || z.interval == interval)
                && self.zone_passes(z, artic, direction, dynamic)
        })
    }
}

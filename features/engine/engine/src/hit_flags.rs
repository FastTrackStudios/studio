//! Flags put on hits at detection, for a plan and a person both to see.
//!
//! **False doubles** — one stroke read twice — are flagged, never removed:
//!
//! - a trigger-only stroke within [`REPEAT_WITHIN`] after a stroke of its
//!   own drum and [`REPEAT_DB`] or more quieter (the trigger firing
//!   twice; Unbreakable's snare did it 24–46 ms after, 7–15 dB down);
//! - a tom heard only on its mic, unsure, within [`BLEED_WITHIN`] of a
//!   louder stroke of another tom (that tom bleeding onto its mic).
//!
//! The flag is the hit's note (`false-double: …`), so it needs nothing new
//! in the marker and a hand can clear it. `HITS_LIST` shows it as the
//! `false-double` flag; `DRUMS_PLAN` puts such hits in `remove`.

/// A trigger-only stroke this soon after its own drum's…
pub const REPEAT_WITHIN: f64 = 0.050;
/// …and this much quieter is the trigger firing twice.
pub const REPEAT_DB: f64 = 6.0;
/// A tom heard on its mic alone this near a louder tom's stroke…
pub const BLEED_WITHIN: f64 = 0.015;
/// …this much quieter is that tom's bleed.
pub const BLEED_DB: f64 = 6.0;

/// The flag's name, and how a hit's note starts when it has it.
pub const FALSE_DOUBLE: &str = "false-double";

/// A detected stroke, as the flags need it.
#[derive(Clone, Copy, Debug)]
pub struct Stroke<'a> {
    pub piece: &'a str,
    /// Seconds (any one clock for all of them).
    pub time: f64,
    pub peak_db: f64,
    pub from_trigger: bool,
    pub from_mic: bool,
    pub unsure: bool,
}

/// Why a stroke is a false double.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The trigger fired again just after its own stroke.
    TriggerRepeat,
    /// Another tom's stroke on this tom's mic.
    TomBleed,
}

impl Why {
    /// The hit's note that says so.
    #[must_use]
    pub fn note(self) -> String {
        match self {
            Self::TriggerRepeat => {
                format!("{FALSE_DOUBLE}: the trigger again, under {REPEAT_DB} dB below its stroke")
            }
            Self::TomBleed => format!("{FALSE_DOUBLE}: another tom on this tom's mic"),
        }
    }
}

/// Whether a hit's note flags it as a false double.
#[must_use]
pub fn is_false_double(note: &str) -> bool {
    note.starts_with(FALSE_DOUBLE)
}

fn is_tom(piece: &str) -> bool {
    piece.to_ascii_lowercase().contains("tom")
}

/// The false doubles among `strokes` (one take's, every piece, any
/// order): each one's index and why.
#[must_use]
pub fn false_doubles(strokes: &[Stroke<'_>]) -> Vec<(usize, Why)> {
    let mut order: Vec<usize> = (0..strokes.len()).collect();
    order.sort_by(|a, b| strokes[*a].time.total_cmp(&strokes[*b].time));
    let mut out = Vec::new();
    for (k, &i) in order.iter().enumerate() {
        let s = &strokes[i];
        let repeat = s.from_trigger
            && !s.from_mic
            && order[..k]
                .iter()
                .rev()
                .map(|&j| &strokes[j])
                .take_while(|o| s.time - o.time <= REPEAT_WITHIN)
                .any(|o| o.piece == s.piece && o.peak_db - s.peak_db >= REPEAT_DB);
        if repeat {
            out.push((i, Why::TriggerRepeat));
            continue;
        }
        let bleed = is_tom(s.piece)
            && s.from_mic
            && !s.from_trigger
            && s.unsure
            && strokes.iter().any(|o| {
                o.piece != s.piece
                    && is_tom(o.piece)
                    && (o.time - s.time).abs() <= BLEED_WITHIN
                    && o.peak_db - s.peak_db >= BLEED_DB
            });
        if bleed {
            out.push((i, Why::TomBleed));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(piece: &str, time: f64, peak_db: f64, trigger: bool, mic: bool) -> Stroke<'_> {
        Stroke {
            piece,
            time,
            peak_db,
            from_trigger: trigger,
            from_mic: mic,
            unsure: false,
        }
    }

    #[test]
    fn a_trigger_firing_twice_is_a_false_double() {
        let strokes = [
            stroke("Snare", 1.000, -15.0, true, true),
            // 30 ms after, trigger only, 9 dB down: the trigger again.
            stroke("Snare", 1.030, -24.0, true, false),
            // 30 ms after, but heard by the mic too: a flam, kept.
            stroke("Snare", 2.000, -15.0, true, true),
            stroke("Snare", 2.030, -24.0, true, true),
            // Trigger only but only 3 dB down: a real stroke.
            stroke("Snare", 3.000, -15.0, true, true),
            stroke("Snare", 3.030, -18.0, true, false),
            // Trigger only, 9 dB down, but 70 ms after: a real stroke.
            stroke("Snare", 4.000, -15.0, true, true),
            stroke("Snare", 4.070, -24.0, true, false),
            // Another drum's stroke before it does not count.
            stroke("Kick", 5.000, -10.0, true, true),
            stroke("Snare", 5.020, -24.0, true, false),
        ];
        assert_eq!(false_doubles(&strokes), vec![(1, Why::TriggerRepeat)]);
    }

    #[test]
    fn a_tom_heard_only_on_its_mic_beside_a_louder_tom_is_bleed() {
        let mut bleed = stroke("Tom 4", 1.004, -36.0, false, true);
        bleed.unsure = true;
        let mut sure = stroke("Tom 4", 2.004, -36.0, false, true);
        sure.unsure = false;
        let mut far = stroke("Tom 4", 3.030, -36.0, false, true);
        far.unsure = true;
        let strokes = [
            stroke("Tom 3", 1.0, -18.0, true, true),
            bleed,
            stroke("Tom 3", 2.0, -18.0, true, true),
            sure,
            stroke("Tom 3", 3.0, -18.0, true, true),
            far,
        ];
        assert_eq!(false_doubles(&strokes), vec![(1, Why::TomBleed)]);
    }

    #[test]
    fn the_flag_is_the_note() {
        assert!(is_false_double(&Why::TriggerRepeat.note()));
        assert!(is_false_double(&Why::TomBleed.note()));
        assert!(!is_false_double("on a written note"));
    }
}

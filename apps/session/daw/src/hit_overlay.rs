//! The kit's hit markers, for the arrangement to draw over the drum rows
//! in Edit mode.
//!
//! The markers live on the recordings (take markers, `crate::hits`);
//! what is drawn is where each one plays now, after every edit, read off
//! the window's thread ([`refresh`]) whenever the hits or the edit
//! change and published here. A piece's marks go on that piece's rows
//! only — the kick's on the kick, the snare's on the snare — while an
//! edit moves the whole kit: the marks say where the hits are, the cuts
//! on every row what was done with them. The fill zones are shaded over
//! the kit, being the kit's.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Who put a hit there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    /// A detector run, unchecked.
    Auto,
    /// Placed (or moved) by a person or an agent.
    Manual,
    /// Found, then checked.
    Approved,
}

/// What has been done to a hit's timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Edited {
    /// It plays where it was played.
    #[default]
    AsPlayed,
    /// An edit moved it (a quantize).
    Quantized,
    /// A hand placed it (its flag dragged).
    ByHand,
}

/// One hit, where it plays now.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    /// Its marker's id: what a hand on it edits.
    pub id: String,
    /// Its piece: an index into [`Overlay::pieces`].
    pub piece: usize,
    /// Project seconds.
    pub time: f64,
    pub who: Who,
    /// A ghost note: drawn short.
    pub ghost: bool,
    /// Bleed, or found with too little confidence: one to look at.
    pub doubt: bool,
    /// Moved by an edit, by a hand, or neither.
    pub edited: Edited,
}

/// A fill or exception zone, shaded over the kit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shade {
    pub start: f64,
    pub end: f64,
    /// Found and not yet looked at.
    pub found: bool,
}

/// A stretch of a piece's slices played slower or faster than recorded
/// — a slice drawn out over a gap — where it plays now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stretch {
    /// Its piece: an index into [`Overlay::pieces`].
    pub piece: usize,
    /// Project seconds.
    pub start: f64,
    pub end: f64,
    /// How fast it plays its take: 1 as recorded, 0.5 half speed.
    pub rate: f64,
}

/// What is drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// The pieces' names (`Kick`, `Snare`, `Tom 1`).
    pub pieces: Vec<String>,
    /// Each piece's tracks (its mics and trigger), by GUID: a row is a
    /// piece's when one of them is the row's track or under it.
    pub tracks: Vec<Vec<String>>,
    /// In time order.
    pub marks: Vec<Mark>,
    pub zones: Vec<Shade>,
    /// Each piece's stretched stretches, in time order.
    pub stretches: Vec<Stretch>,
    /// Which take the comp plays where, in time order: the takes stitched
    /// together and the seams between them.
    pub comps: Vec<CompSpan>,
    /// The stretches kept as comped (`COMP_KEEP`), in time order, seconds.
    pub kept: Vec<(f64, f64)>,
}

/// A stretch of the song the comp plays from one take.
#[derive(Clone, Debug, PartialEq)]
pub struct CompSpan {
    /// Project seconds.
    pub start: f64,
    pub end: f64,
    /// The take's name, as its recording is called on the kit's first mic:
    /// what its colour comes from ([`crate::arrangement::take_color`]).
    pub take: String,
}

impl Overlay {
    /// The comp's spans `from..to` touches, each whole — a click
    /// (`from == to`) the one it is in: the stretches a T or Shift+T
    /// comps from another take.
    #[must_use]
    pub fn comps_over(&self, from: f64, to: f64) -> Vec<(f64, f64)> {
        let first = self.comps.partition_point(|c| c.end < from);
        self.comps
            .get(first..)
            .unwrap_or_default()
            .iter()
            .take_while(|c| c.start <= to)
            .filter(|c| c.end > from || (from - to).abs() < f64::EPSILON && c.end >= from)
            .map(|c| (c.start, c.end))
            .collect()
    }

    /// The marks between `from` and `to` seconds.
    #[must_use]
    pub fn between(&self, from: f64, to: f64) -> &[Mark] {
        let a = self.marks.partition_point(|m| m.time < from);
        let b = self.marks.partition_point(|m| m.time <= to);
        self.marks.get(a..b.max(a)).unwrap_or_default()
    }

    /// The pieces a row of track `guid` shows: those with a track that
    /// is it or under it (`parent_of` a track's folder) — one for a
    /// piece's folder or mic, every piece for the kit's folder shut, none
    /// for the cymbals and rooms.
    pub fn pieces_under<'a>(
        &self,
        guid: &str,
        parent_of: impl Fn(&str) -> Option<&'a str>,
    ) -> Vec<usize> {
        let under = |track: &str| {
            let mut at = Some(track);
            // A folder tree is shallow; the bound only guards a cycle.
            for _ in 0..64 {
                match at {
                    Some(t) if t == guid => return true,
                    Some(t) => at = parent_of(t),
                    None => return false,
                }
            }
            false
        };
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, tracks)| tracks.iter().any(|t| under(t)))
            .map(|(piece, _)| piece)
            .collect()
    }
}

static SHOWN: Mutex<Option<Arc<Overlay>>> = Mutex::new(None);
static VERSION: AtomicU64 = AtomicU64::new(0);

/// Show `overlay` (its marks sorted here).
pub fn publish(mut overlay: Overlay) {
    overlay.marks.sort_by(|a, b| a.time.total_cmp(&b.time));
    if let Ok(mut shown) = SHOWN.lock() {
        *shown = Some(Arc::new(overlay));
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
}

/// What is shown, if anything.
#[must_use]
pub fn current() -> Option<Arc<Overlay>> {
    SHOWN.lock().ok().and_then(|s| s.clone())
}

/// Goes up on every change: a view compares it with the one it drew.
#[must_use]
pub fn version() -> u64 {
    VERSION.load(Ordering::Relaxed)
}

/// A read is running.
static BUSY: AtomicBool = AtomicBool::new(false);
/// Asked for again while one ran: the hits may have changed under it.
static AGAIN: AtomicBool = AtomicBool::new(false);

/// Read the markers again, off the window's thread, and publish them —
/// after anything that moves a hit or a slice — and the tracks' waveform
/// detail with them. Asked for while a read
/// runs, one more runs after it; never two at once.
pub fn refresh() {
    #[cfg(feature = "native")]
    {
        AGAIN.store(true, Ordering::Release);
        if BUSY.swap(true, Ordering::AcqRel) {
            return;
        }
        let Some(runtime) = crate::open::runtime() else {
            BUSY.store(false, Ordering::Release);
            return;
        };
        runtime.spawn_blocking(|| {
            loop {
                AGAIN.store(false, Ordering::Release);
                if let Ok((daw, project)) = crate::actions::song() {
                    publish(crate::hits::overlay(daw, &project));
                }
                // What moved the hits moved the audio: the tracks' detail
                // is read again from the engine. An action (a quantize run
                // over MCP) edits the engine directly, past the window's
                // edit worker; a track read while its edits were landing
                // was held half-edited — the take as played drawn under
                // hits that had moved, for good.
                crate::wave_detail::engine_changed();
                BUSY.store(false, Ordering::Release);
                if !AGAIN.load(Ordering::Acquire) || BUSY.swap(true, Ordering::AcqRel) {
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {

    fn comps(spans: &[(f64, f64, &str)]) -> Overlay {
        Overlay {
            comps: spans
                .iter()
                .map(|&(start, end, take)| CompSpan {
                    start,
                    end,
                    take: take.to_owned(),
                })
                .collect(),
            ..Overlay::default()
        }
    }

    /// A click picks the take's span it is on; a marquee every span it
    /// touches, each whole.
    #[test]
    fn a_click_or_a_marquee_picks_whole_spans_of_the_comp() {
        let o = comps(&[(0.0, 4.0, "a"), (4.0, 6.5, "b"), (6.5, 9.0, "a")]);
        assert_eq!(o.comps_over(5.0, 5.0), vec![(4.0, 6.5)]);
        assert_eq!(
            o.comps_over(3.0, 7.0),
            vec![(0.0, 4.0), (4.0, 6.5), (6.5, 9.0)]
        );
        assert_eq!(o.comps_over(4.5, 6.0), vec![(4.0, 6.5)]);
        assert!(o.comps_over(10.0, 11.0).is_empty());
    }
    use super::*;

    fn mark(piece: usize, time: f64) -> Mark {
        Mark {
            id: String::new(),
            piece,
            time,
            who: Who::Auto,
            ghost: false,
            doubt: false,
            edited: Edited::AsPlayed,
        }
    }

    #[test]
    fn between_finds_the_marks_in_view() {
        let overlay = Overlay {
            pieces: vec!["Kick".into()],
            tracks: vec![Vec::new()],
            marks: vec![mark(0, 1.0), mark(0, 2.0), mark(0, 3.0), mark(0, 4.0)],
            zones: Vec::new(),
            stretches: Vec::new(),
            comps: Vec::new(),
            kept: Vec::new(),
        };
        let times: Vec<f64> = overlay.between(1.5, 3.0).iter().map(|m| m.time).collect();
        assert_eq!(times, [2.0, 3.0]);
        assert!(overlay.between(5.0, 6.0).is_empty());
        assert!(
            overlay.between(3.0, 1.0).is_empty(),
            "a span the wrong way round"
        );
    }

    #[test]
    fn a_row_shows_the_pieces_under_it() {
        let overlay = Overlay {
            pieces: vec!["Kick".into(), "Tom 1".into(), "Tom 2".into()],
            tracks: vec![
                vec!["kick-in".into()],
                vec!["t1".into(), "t1-trig".into()],
                vec!["t2".into()],
            ],
            ..Overlay::default()
        };
        let parents = [
            ("kick-in", "kick"),
            ("kick", "drums"),
            ("t1", "tom1"),
            ("t1-trig", "tom1"),
            ("tom1", "toms"),
            ("t2", "tom2"),
            ("tom2", "toms"),
            ("toms", "drums"),
            ("ohs", "drums"),
        ];
        let parent_of = |t: &str| parents.iter().find(|(c, _)| *c == t).map(|(_, p)| *p);
        assert_eq!(overlay.pieces_under("kick", parent_of), [0]);
        assert_eq!(overlay.pieces_under("t1-trig", parent_of), [1]);
        assert_eq!(overlay.pieces_under("toms", parent_of), [1, 2]);
        assert_eq!(overlay.pieces_under("drums", parent_of), [0, 1, 2]);
        assert!(overlay.pieces_under("ohs", parent_of).is_empty());
    }
}

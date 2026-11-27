//! The TEMPO lane's gestures: the song's tempo and meter set from the
//! ruler.
//!
//! - A click on the lane where nothing changes makes a change there, at the
//!   tempo and meter the song already has: a flag to take hold of.
//! - A change dragged across the lane moves (on the grid); dragged up or
//!   down, its tempo goes faster or slower (whole beats a minute, tenths
//!   with the fine key). A press on empty lane dragged up or down makes a
//!   change there at the tempo dragged to.
//! - A click on a change opens its menu: its meter, half or double time,
//!   or the change taken away.
//!
//! The song's first change, at zero, is its own tempo: it never moves and
//! is never taken away. Every gesture is one undo step, committed as
//! `ARRANGE_TEMPO` (`daw_ui::arrange_actions`), and drawn here at once
//! ([`predict`]) until the song is read back.

use anyrender::PaintScene;
use daw_ui::studio::project::TempoChange;
use vello::kurbo::{Affine, Rect};
use vello::peniko::Fill;

use crate::engine::Edit;

/// The undo step a tempo gesture makes.
pub const LABEL: &str = "Edit the tempo map";

/// The slowest and fastest tempo the lane sets, as `TEMPO_SET` takes them.
pub const BPM: std::ops::RangeInclusive<f64> = 10.0..=400.0;

/// The meters a change's menu offers.
pub const METERS: [(u32, u32); 8] = [
    (2, 4),
    (3, 4),
    (4, 4),
    (5, 4),
    (6, 8),
    (7, 8),
    (9, 8),
    (12, 8),
];

/// How near a change's flag, in pixels, takes hold of it (left of it; to
/// the right, its reading is part of it).
const GRIP: f64 = 6.0;

/// How far right of its flag a change's reading ("120 4/4") reaches.
const READING_W: f64 = 44.0;

/// Beats a minute per pixel dragged: coarse, and with the fine key.
const PER_PX: (f64, f64) = (0.5, 0.05);

/// How close two times are to be one change, seconds: the engine's rule.
const SAME: f64 = crate::engine::SAME_POINT;

/// Where the TEMPO lane is in the widget, and how time runs across it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lane {
    pub top: f64,
    pub bottom: f64,
    /// Where the timeline starts (the track panel's right edge).
    pub left: f64,
    /// Where time zero is, in pixels.
    pub zero: f64,
    pub pps: f64,
}

impl Lane {
    /// The lane in a view, the ruler at the top of the widget. `None` when
    /// the ruler does not show it (a slim ruler on a touchscreen, unless
    /// asked for).
    #[must_use]
    pub fn of(view: crate::arrangement::Viewport) -> Option<Self> {
        let tall = crate::ruler::tempo_h();
        if tall <= 0.0 {
            return None;
        }
        let bottom = crate::ruler::ruler_h() - crate::ruler::bars_h();
        Some(Self {
            top: bottom - tall,
            bottom,
            left: view.panel_w,
            zero: view.panel_w - view.scroll_x,
            pps: view.pps.max(f64::EPSILON),
        })
    }

    /// Whether `(x, y)` is on the lane.
    #[must_use]
    pub fn contains(&self, x: f64, y: f64) -> bool {
        y >= self.top && y < self.bottom && x >= self.left
    }

    /// Where a time is across the lane.
    #[must_use]
    pub fn x_of(&self, at: f64) -> f64 {
        at.mul_add(self.pps, self.zero)
    }
}

/// The change whose flag or reading is at `x` on the lane: the nearest
/// flag of those in reach.
#[must_use]
pub fn change_at(tempo: &[TempoChange], lane: &Lane, x: f64) -> Option<usize> {
    tempo
        .iter()
        .enumerate()
        .filter_map(|(index, change)| {
            let away = x - lane.x_of(change.at);
            (-GRIP..=READING_W)
                .contains(&away)
                .then_some((away.abs(), index))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, index)| index)
}

/// The change in force at `at`: the last at or before it (the first if
/// none is).
#[must_use]
pub fn in_force(tempo: &[TempoChange], at: f64) -> Option<TempoChange> {
    tempo
        .iter()
        .take_while(|change| change.at <= at + SAME)
        .last()
        .or_else(|| tempo.first())
        .copied()
}

/// What a drag on the lane is doing, once it has moved far enough to be one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Across the lane: the change moves in time.
    Move,
    /// Up or down: its tempo.
    Tempo,
}

/// A press on the lane, until it lets go.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    /// The change pressed, by its place in the window's list; `None` for a
    /// press where nothing changes (one is made).
    index: Option<usize>,
    /// The change as it was at the press (or as it would be made).
    was: TempoChange,
    /// Where the press was, in pixels.
    from: (f64, f64),
    mode: Option<Mode>,
    /// The change as the hand has it now.
    pub now: TempoChange,
}

impl Drag {
    /// A press at `(x, y)`, at `at` seconds (on the grid when it snaps).
    /// `None` off the lane.
    #[must_use]
    pub fn press(tempo: &[TempoChange], lane: &Lane, (x, y): (f64, f64), at: f64) -> Option<Self> {
        if !lane.contains(x, y) {
            return None;
        }
        let index = change_at(tempo, lane, x);
        let was = match index {
            Some(index) => *tempo.get(index)?,
            None => TempoChange {
                at: at.max(0.0),
                ..in_force(tempo, at)?
            },
        };
        Some(Self {
            index,
            was,
            from: (x, y),
            mode: None,
            now: was,
        })
    }

    /// Whether the change pressed is the song's own tempo, at zero.
    #[must_use]
    pub fn is_first(&self) -> bool {
        self.index.is_some() && self.was.at < SAME
    }

    /// Whether the press has become a drag.
    #[must_use]
    pub const fn dragging(&self) -> bool {
        self.mode.is_some()
    }

    /// The pointer at `(x, y)`, over `at` seconds (on the grid when it
    /// snaps). Whether what is drawn changed.
    pub fn moved(&mut self, (x, y): (f64, f64), at: f64, fine: bool) -> bool {
        let (dx, dy) = (x - self.from.0, y - self.from.1);
        let mode = match self.mode {
            Some(mode) => mode,
            None => {
                if dx.hypot(dy) < crate::gesture::SLOP {
                    return false;
                }
                // The song's first change has nowhere to move to.
                let mode = if dy.abs() > dx.abs() || self.is_first() {
                    Mode::Tempo
                } else {
                    Mode::Move
                };
                self.mode = Some(mode);
                mode
            }
        };
        let before = self.now;
        match mode {
            Mode::Move => self.now.at = at.max(SAME * 2.0),
            Mode::Tempo => {
                let (per, step) = if fine {
                    (PER_PX.1, 0.1)
                } else {
                    (PER_PX.0, 1.0)
                };
                let bpm = (self.was.bpm - dy * per).clamp(*BPM.start(), *BPM.end());
                self.now.bpm = ((bpm / step).round() * step).clamp(*BPM.start(), *BPM.end());
            }
        }
        self.now != before
    }

    /// The press let go.
    #[must_use]
    pub fn released(self) -> Release {
        let Some(mode) = self.mode else {
            return match self.index {
                Some(index) => Release::Menu(index),
                None => Release::Edits(vec![Edit::SetTempo(
                    String::new(),
                    self.was.at,
                    self.was.bpm,
                )]),
            };
        };
        if self.now == self.was && self.index.is_some() {
            return Release::Nothing;
        }
        Release::Edits(match (mode, self.index) {
            (Mode::Move, Some(_)) => {
                vec![Edit::MoveTempo(String::new(), self.was.at, self.now.at)]
            }
            (Mode::Move | Mode::Tempo, _) => {
                vec![Edit::SetTempo(String::new(), self.now.at, self.now.bpm)]
            }
        })
    }
}

/// What a press on the lane comes to.
#[derive(Clone, Debug, PartialEq)]
pub enum Release {
    /// A click on a change: its menu.
    Menu(usize),
    /// Edits to commit, as one step.
    Edits(Vec<Edit>),
    /// Let go where it started.
    Nothing,
}

/// A choice on a change's menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Meter(u32, u32),
    Tempo(f64),
    Remove,
}

/// A change's menu: its meter, half and double time, and (not the song's
/// first) taking it away.
#[must_use]
pub fn menu(change: &TempoChange, first: bool) -> Vec<crate::popup::Choice<Pick>> {
    use crate::popup::Choice;
    let mut choices = vec![Choice::heading(format!(
        "{} at {:.2} s",
        crate::ruler::reading(change),
        change.at
    ))];
    choices.push(Choice::heading("Meter"));
    for (top, bottom) in METERS {
        choices.push(Choice::new(
            format!("{top}/{bottom}"),
            change.beats_per_bar == top && change.beat_unit == bottom,
            Pick::Meter(top, bottom),
        ));
    }
    choices.push(Choice::heading("Tempo"));
    for (label, bpm) in [
        ("Half time", change.bpm / 2.0),
        ("Double time", change.bpm * 2.0),
    ] {
        if BPM.contains(&bpm) {
            choices.push(Choice::new(
                format!(
                    "{label} ({})",
                    crate::ruler::reading(&TempoChange { bpm, ..*change })
                ),
                false,
                Pick::Tempo(bpm),
            ));
        }
    }
    if !first {
        choices.push(Choice::new("Remove this change", false, Pick::Remove));
    }
    choices
}

/// The edits a menu choice on `change` makes.
#[must_use]
pub fn picked(change: &TempoChange, pick: Pick) -> Vec<Edit> {
    match pick {
        Pick::Meter(top, bottom) => vec![Edit::SetMeter(String::new(), change.at, top, bottom)],
        Pick::Tempo(bpm) => vec![Edit::SetTempo(String::new(), change.at, bpm)],
        Pick::Remove if change.at >= SAME => vec![Edit::RemoveTempo(String::new(), change.at)],
        Pick::Remove => Vec::new(),
    }
}

/// The window's tempo map as `edit` leaves it: drawn at once, before the
/// song is read back. Edits that are not the tempo's change nothing.
pub fn predict(tempo: &mut Vec<TempoChange>, edit: &Edit) {
    let at_point = |tempo: &[TempoChange], at: f64| {
        tempo
            .iter()
            .position(|change| (change.at - at).abs() < SAME)
    };
    match *edit {
        Edit::SetTempo(_, at, bpm) => match at_point(tempo, at) {
            Some(index) => {
                if let Some(change) = tempo.get_mut(index) {
                    change.bpm = bpm;
                }
            }
            None => {
                if let Some(force) = in_force(tempo, at) {
                    tempo.push(TempoChange { at, bpm, ..force });
                }
            }
        },
        // Never onto another change, as the engine refuses it.
        Edit::MoveTempo(_, from, to)
            if from >= SAME
                && to >= SAME
                && ((to - from).abs() < SAME || at_point(tempo, to).is_none()) =>
        {
            if let Some(change) = at_point(tempo, from).and_then(|i| tempo.get_mut(i)) {
                change.at = to;
            }
        }
        Edit::RemoveTempo(_, at) if at >= SAME => {
            if let Some(index) = at_point(tempo, at) {
                tempo.remove(index);
            }
        }
        Edit::SetMeter(_, at, top, bottom) => {
            let index = match at_point(tempo, at) {
                Some(index) => index,
                None => {
                    let Some(force) = in_force(tempo, at) else {
                        return;
                    };
                    tempo.push(TempoChange { at, ..force });
                    tempo.sort_by(|a, b| a.at.total_cmp(&b.at));
                    match at_point(tempo, at) {
                        Some(index) => index,
                        None => return,
                    }
                }
            };
            let Some(old) = tempo.get(index).map(|c| (c.beats_per_bar, c.beat_unit)) else {
                return;
            };
            // The changes after it that only carried the old meter on
            // carry the new one; the first that set its own stops it.
            for change in tempo.iter_mut().skip(index) {
                if (change.beats_per_bar, change.beat_unit) != old {
                    break;
                }
                change.beats_per_bar = top;
                change.beat_unit = bottom;
            }
        }
        _ => return,
    }
    tempo.sort_by(|a, b| a.at.total_cmp(&b.at));
}

/// A drag in flight, drawn over the lane: a flag where the change would
/// land, reading what it would be.
pub fn paint_ghost(
    painter: &mut impl PaintScene,
    palette: &crate::palette::Palette,
    font: &crate::text::Font,
    lane: &Lane,
    drag: &Drag,
) {
    if !drag.dragging() {
        return;
    }
    let x = lane.x_of(drag.now.at);
    if x < lane.left - 1.0 {
        return;
    }
    let back = Rect::new(x, lane.top + 1.0, x + READING_W + 8.0, lane.bottom - 1.0);
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.tcp_field,
        None,
        &back,
    );
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.accent,
        None,
        &Rect::new(x, lane.top, x + 2.0, lane.bottom),
    );
    crate::tcp::glyphs(
        painter,
        font,
        palette.text,
        &crate::ruler::reading(&drag.now),
        x + 4.0,
        lane.bottom - 3.0,
        8.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn change(at: f64, bpm: f64, top: u32, bottom: u32) -> TempoChange {
        TempoChange {
            at,
            bpm,
            beats_per_bar: top,
            beat_unit: bottom,
        }
    }

    fn lane() -> Lane {
        Lane {
            top: 30.0,
            bottom: 43.0,
            left: 100.0,
            zero: 100.0,
            pps: 10.0,
        }
    }

    fn song() -> Vec<TempoChange> {
        vec![change(0.0, 120.0, 4, 4), change(10.0, 90.0, 4, 4)]
    }

    #[test]
    fn a_press_takes_the_change_whose_flag_or_reading_it_is_on() {
        let tempo = song();
        // The second change's flag is at x = 200.
        assert_eq!(change_at(&tempo, &lane(), 197.0), Some(1));
        assert_eq!(change_at(&tempo, &lane(), 230.0), Some(1));
        assert_eq!(change_at(&tempo, &lane(), 160.0), None);
        assert_eq!(change_at(&tempo, &lane(), 101.0), Some(0));
    }

    #[test]
    fn a_click_where_nothing_changes_makes_a_change_at_the_tempo_there() {
        let tempo = song();
        let drag = Drag::press(&tempo, &lane(), (260.0, 35.0), 16.0).expect("on the lane");
        assert_eq!(
            drag.released(),
            Release::Edits(vec![Edit::SetTempo(String::new(), 16.0, 90.0)])
        );
    }

    #[test]
    fn a_click_on_a_change_opens_its_menu() {
        let tempo = song();
        let drag = Drag::press(&tempo, &lane(), (201.0, 35.0), 10.0).expect("on the lane");
        assert_eq!(drag.released(), Release::Menu(1));
    }

    #[test]
    fn a_change_dragged_across_moves_and_up_goes_faster() {
        let tempo = song();
        let mut across = Drag::press(&tempo, &lane(), (201.0, 35.0), 10.0).expect("on the lane");
        assert!(across.moved((241.0, 36.0), 14.0, false));
        assert_eq!(
            across.released(),
            Release::Edits(vec![Edit::MoveTempo(String::new(), 10.0, 14.0)])
        );
        let mut up = Drag::press(&tempo, &lane(), (201.0, 35.0), 10.0).expect("on the lane");
        assert!(up.moved((202.0, 15.0), 10.0, false));
        assert_eq!(
            up.released(),
            Release::Edits(vec![Edit::SetTempo(String::new(), 10.0, 100.0)])
        );
    }

    #[test]
    fn the_songs_first_change_only_takes_a_tempo() {
        let tempo = song();
        let mut drag = Drag::press(&tempo, &lane(), (101.0, 35.0), 0.0).expect("on the lane");
        assert!(drag.is_first());
        drag.moved((141.0, 35.0), 4.0, false);
        assert!(matches!(drag.released(), Release::Nothing));
        assert!(picked(&tempo[0], Pick::Remove).is_empty());
        assert!(
            !menu(&tempo[0], true)
                .iter()
                .any(|c| c.action == Some(Pick::Remove))
        );
    }

    #[test]
    fn a_meter_set_carries_on_until_a_change_sets_its_own() {
        let mut tempo = vec![
            change(0.0, 120.0, 4, 4),
            change(10.0, 90.0, 4, 4),
            change(20.0, 90.0, 6, 8),
        ];
        predict(&mut tempo, &Edit::SetMeter(String::new(), 4.0, 3, 4));
        let meters: Vec<(f64, u32)> = tempo.iter().map(|c| (c.at, c.beats_per_bar)).collect();
        assert_eq!(meters, vec![(0.0, 4), (4.0, 3), (10.0, 3), (20.0, 6)]);
        predict(&mut tempo, &Edit::MoveTempo(String::new(), 10.0, 12.0));
        predict(&mut tempo, &Edit::RemoveTempo(String::new(), 4.0));
        predict(&mut tempo, &Edit::RemoveTempo(String::new(), 0.0));
        let times: Vec<f64> = tempo.iter().map(|c| c.at).collect();
        assert_eq!(times, vec![0.0, 12.0, 20.0]);
    }
}

//! Pointer input the arrangement and the Editor read the same way: the
//! modifier keys, clicks told apart from drags, and the one-axis lock.
//!
//! # Why a press is not a click
//!
//! The obvious implementation acts on press. It is wrong in three ways
//! that all show up the first time somebody uses it: a press that turns
//! into a drag has already toggled the mute it started on; a
//! double-click has toggled it twice on the way to being a
//! double-click; and a press you drag away from and release elsewhere
//! still counts, when every other application treats that as "I changed
//! my mind".
//!
//! So a click is decided on RELEASE, by what happened in between
//! ([`Clicks`]).

use std::time::Duration;
use web_time::Instant;

pub use crate::gestures::SLOP;

/// How long between two clicks for the second to be a double.
pub const DOUBLE: Duration = Duration::from_millis(400);

/// Modifier keys, normalized across platforms (`cmd` on macOS is
/// reported as `ctrl`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// A modifier combination as one key, for a table. Cmd is normalized to
/// Ctrl upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModKey(u8);

impl ModKey {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1);
    pub const CTRL: Self = Self(2);
    pub const ALT: Self = Self(4);

    #[must_use]
    pub const fn of(m: Mods) -> Self {
        let mut bits = 0;
        if m.shift {
            bits |= 1;
        }
        if m.ctrl {
            bits |= 2;
        }
        if m.alt {
            bits |= 4;
        }
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// From REAPER's mouse-modifier index bits (bit0=Shift, bit1=Ctrl,
    /// bit2=Alt) — the same layout this type uses, kept as an explicit
    /// constructor so a host loading `reaper-mouse.ini` states the
    /// correspondence instead of relying on it silently.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b111)
    }

    /// REAPER's `<S-C-A->` notation.
    #[must_use]
    pub fn notation(self) -> String {
        if self.0 == 0 {
            return String::new();
        }
        let mut s = String::from("<");
        if self.0 & 1 != 0 {
            s.push_str("S-");
        }
        if self.0 & 2 != 0 {
            s.push_str("C-");
        }
        if self.0 & 4 != 0 {
            s.push_str("A-");
        }
        s.push('>');
        s
    }
}

impl From<Mods> for ModKey {
    fn from(m: Mods) -> Self {
        Self::of(m)
    }
}

impl From<ModKey> for Mods {
    fn from(k: ModKey) -> Self {
        Self {
            shift: k.0 & 1 != 0,
            ctrl: k.0 & 2 != 0,
            alt: k.0 & 4 != 0,
        }
    }
}

/// What the pointer did, over whatever the surface hit-tested (`H`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClickEvent<H> {
    Click(H),
    DoubleClick(H),
    /// A drag in progress. `delta` is since the last report, already
    /// scaled by what [`Clicks::moved`] was given.
    Drag {
        hit: H,
        delta: (f64, f64),
    },
    /// The drag ended. Sent so a control can commit, or push one undo
    /// step for the whole gesture rather than one per pixel.
    DragEnd(H),
}

/// The pointer's state between events: clicks decided on release, a
/// double within [`DOUBLE`] and [`SLOP`], a drag once past [`SLOP`].
#[derive(Clone, Copy, Debug)]
pub struct Clicks<H> {
    press: Option<Press<H>>,
    last_click: Option<(Instant, (f64, f64))>,
}

impl<H> Default for Clicks<H> {
    fn default() -> Self {
        Self {
            press: None,
            last_click: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Press<H> {
    hit: H,
    at: (f64, f64),
    /// Where the last drag delta was measured from.
    last: (f64, f64),
    dragging: bool,
}

fn beyond_slop(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() > SLOP || (a.1 - b.1).abs() > SLOP
}

impl<H: Copy> Clicks<H> {
    /// The pointer went down on `hit`.
    pub const fn press(&mut self, hit: H, x: f64, y: f64) {
        self.press = Some(Press {
            hit,
            at: (x, y),
            last: (x, y),
            dragging: false,
        });
    }

    /// The pointer moved. Returns a drag once it has moved far enough to
    /// be one, its delta multiplied by `scale` (1 for a plain drag; a
    /// control's fine modifier passes less).
    pub fn moved(&mut self, x: f64, y: f64, scale: f64) -> Option<ClickEvent<H>> {
        let press = self.press.as_mut()?;
        if !press.dragging && !beyond_slop((x, y), press.at) {
            return None;
        }
        // The first drag measures from where the press STARTED, not
        // from the slop threshold — otherwise the control jumps by the
        // slop distance the moment a drag begins.
        press.dragging = true;
        let delta = ((x - press.last.0) * scale, (y - press.last.1) * scale);
        press.last = (x, y);
        Some(ClickEvent::Drag {
            hit: press.hit,
            delta,
        })
    }

    /// The pointer came up. Returns the click, if it was one.
    pub fn release(&mut self, x: f64, y: f64, now: Instant) -> Option<ClickEvent<H>> {
        let press = self.press.take()?;
        if press.dragging {
            return Some(ClickEvent::DragEnd(press.hit));
        }
        // Released somewhere else entirely: not a click on anything.
        if beyond_slop((x, y), press.at) {
            return None;
        }
        let double = self.last_click.is_some_and(|(when, at)| {
            now.saturating_duration_since(when) <= DOUBLE && !beyond_slop((x, y), at)
        });
        if double {
            // Consumed, so three clicks are a double and a single rather
            // than two doubles.
            self.last_click = None;
            return Some(ClickEvent::DoubleClick(press.hit));
        }
        self.last_click = Some((now, (x, y)));
        Some(ClickEvent::Click(press.hit))
    }

    /// Whether a drag is in progress — for a cursor shape, or to keep
    /// dragging a control whose pointer has left it.
    #[must_use]
    pub const fn dragging(&self) -> bool {
        matches!(self.press, Some(Press { dragging: true, .. }))
    }

    /// Abandon whatever was in progress. For focus loss, which is not a
    /// release and must not commit anything.
    pub const fn cancel(&mut self) {
        self.press = None;
    }
}

/// Which axes a move gesture is allowed to travel on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxisLock {
    #[default]
    Both,
    Horizontal,
    Vertical,
    /// Free until the drag commits, then locked to whichever axis it
    /// travelled furthest along first. REAPER's Shift-drag, and the
    /// reason it is not simply "the larger delta each frame": a gesture
    /// that re-decides every frame flickers between axes at the corner.
    Commit(Option<bool>),
}

impl AxisLock {
    /// How far a [`AxisLock::Commit`] drag travels before it decides.
    /// A threshold, not a first-pixel decision: the first pixel of a drag
    /// is noise, and committing on it locks half of them to the wrong
    /// axis.
    pub const DEAD_PX: f64 = 4.0;

    /// Resolve against a drag delta (from the press), committing if this
    /// is the first movement worth deciding on. Returns `(horizontal,
    /// vertical)`.
    pub fn allows(&mut self, dx: f64, dy: f64) -> (bool, bool) {
        match self {
            Self::Both => (true, true),
            Self::Horizontal => (true, false),
            Self::Vertical => (false, true),
            Self::Commit(decided) => {
                if decided.is_none() && dx.abs().max(dy.abs()) >= Self::DEAD_PX {
                    *decided = Some(dx.abs() >= dy.abs());
                }
                match decided {
                    Some(true) => (true, false),
                    Some(false) => (false, true),
                    None => (false, false),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn a_click_is_decided_on_release() {
        let now = Instant::now();
        let mut g = Clicks::default();
        g.press(3, 100.0, 100.0);
        assert!(g.moved(101.0, 100.0, 1.0).is_none(), "that was not a drag");
        assert_eq!(g.release(101.0, 100.0, now), Some(ClickEvent::Click(3)));
    }

    #[test]
    fn dragging_off_a_control_cancels_it() {
        let now = Instant::now();
        let mut g = Clicks::default();
        g.press(1, 100.0, 100.0);
        assert!(g.moved(140.0, 100.0, 1.0).is_some());
        assert_eq!(g.release(140.0, 100.0, now), Some(ClickEvent::DragEnd(1)));

        // A press released elsewhere WITHOUT ever becoming a drag is
        // nothing at all.
        let mut g = Clicks::default();
        g.press(1, 100.0, 100.0);
        assert_eq!(g.release(200.0, 100.0, now), None);
    }

    #[test]
    fn two_quick_clicks_are_a_double() {
        let base = Instant::now();
        let mut g = Clicks::default();
        g.press(2, 50.0, 50.0);
        assert_eq!(g.release(50.0, 50.0, base), Some(ClickEvent::Click(2)));
        g.press(2, 50.0, 50.0);
        assert_eq!(
            g.release(50.0, 50.0, at(base, 120)),
            Some(ClickEvent::DoubleClick(2))
        );
    }

    #[test]
    fn a_slow_or_distant_second_click_is_its_own_click() {
        let base = Instant::now();
        let mut g = Clicks::default();
        g.press(2, 50.0, 50.0);
        g.release(50.0, 50.0, base);
        g.press(2, 50.0, 50.0);
        assert_eq!(
            g.release(50.0, 50.0, at(base, 900)),
            Some(ClickEvent::Click(2))
        );

        let mut g = Clicks::default();
        g.press(2, 50.0, 50.0);
        g.release(50.0, 50.0, base);
        g.press(2, 400.0, 50.0);
        assert_eq!(
            g.release(400.0, 50.0, at(base, 100)),
            Some(ClickEvent::Click(2))
        );
    }

    #[test]
    fn a_third_click_starts_over() {
        let base = Instant::now();
        let mut g = Clicks::default();
        for (ms, expected) in [
            (0, ClickEvent::Click(0)),
            (100, ClickEvent::DoubleClick(0)),
            (200, ClickEvent::Click(0)),
        ] {
            g.press(0, 10.0, 10.0);
            assert_eq!(g.release(10.0, 10.0, at(base, ms)), Some(expected));
        }
    }

    #[test]
    fn a_drag_does_not_jump_when_it_starts() {
        let mut g = Clicks::default();
        g.press(0, 0.0, 100.0);
        let Some(ClickEvent::Drag { delta, .. }) = g.moved(0.0, 90.0, 1.0) else {
            panic!("expected a drag");
        };
        assert!((delta.1 + 10.0).abs() < f64::EPSILON, "delta was {delta:?}");
    }

    #[test]
    fn the_scale_scales_the_movement() {
        let mut g = Clicks::default();
        g.press(0, 0.0, 100.0);
        let Some(ClickEvent::Drag { delta, .. }) = g.moved(0.0, 60.0, 0.25) else {
            panic!("expected a drag");
        };
        assert!((delta.1 + 10.0).abs() < 1e-9, "delta was {delta:?}");
    }

    #[test]
    fn cancelling_commits_nothing() {
        let now = Instant::now();
        let mut g = Clicks::default();
        g.press(0, 0.0, 0.0);
        g.moved(0.0, 50.0, 1.0);
        assert!(g.dragging());
        g.cancel();
        assert!(!g.dragging());
        assert_eq!(g.release(0.0, 50.0, now), None);
    }

    #[test]
    fn mods_and_modkey_round_trip() {
        for bits in 0..8 {
            let k = ModKey::from_bits(bits);
            assert_eq!(ModKey::of(Mods::from(k)), k);
        }
        assert_eq!(
            ModKey::of(Mods {
                shift: true,
                ctrl: false,
                alt: true
            })
            .notation(),
            "<S-A->"
        );
    }

    #[test]
    fn one_axis_waits_then_holds() {
        let mut lock = AxisLock::Commit(None);
        assert_eq!(
            lock.allows(2.0, 1.0),
            (false, false),
            "inside the dead zone"
        );
        assert_eq!(lock.allows(10.0, 3.0), (true, false));
        // Wandering the other way later does not re-decide.
        assert_eq!(lock.allows(10.0, 300.0), (true, false));
    }
}

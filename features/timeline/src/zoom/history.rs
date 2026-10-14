//! The views already been to: back and forward through them, and the
//! toggles that frame something and, pressed again while still framing
//! it, go back.

/// How many views back a history keeps: a long session of zooming is not
/// a history anyone walks back through all of.
const DEPTH: usize = 64;

/// The views a surface has framed, of whatever it calls a view (`V`).
#[derive(Clone, Debug)]
pub struct ZoomHistory<V> {
    back: Vec<V>,
    forward: Vec<V>,
    /// What the last toggle framed, so pressing it again while still
    /// there goes back instead.
    toggled: Option<V>,
}

impl<V> Default for ZoomHistory<V> {
    fn default() -> Self {
        Self {
            back: Vec::new(),
            forward: Vec::new(),
            toggled: None,
        }
    }
}

impl<V: Copy> ZoomHistory<V> {
    /// Back from `now`, if there is anywhere to go.
    pub fn back(&mut self, now: V) -> Option<V> {
        let to = self.back.pop()?;
        self.forward.push(now);
        self.toggled = None;
        Some(to)
    }

    /// Forward from `now`, if there is anywhere to go.
    pub fn forward(&mut self, now: V) -> Option<V> {
        let to = self.forward.pop()?;
        self.back.push(now);
        self.toggled = None;
        Some(to)
    }

    /// Frame something from `now`: where `framed` says, remembering where
    /// it was. A `toggle` pressed again while the view is still what it
    /// framed (`showing` says whether a view is that one) goes back
    /// instead. `None` when that back has nowhere to go.
    pub fn frame(
        &mut self,
        now: V,
        toggle: bool,
        showing: impl Fn(&V) -> bool,
        framed: impl FnOnce() -> V,
    ) -> Option<V> {
        if toggle && self.toggled.as_ref().is_some_and(&showing) {
            self.toggled = None;
            let to = self.back.pop()?;
            self.forward.push(now);
            return Some(to);
        }
        let to = framed();
        self.toggled = toggle.then_some(to);
        self.back.push(now);
        self.forward.clear();
        if self.back.len() > DEPTH {
            self.back.remove(0);
        }
        Some(to)
    }

    /// A move that is not a frame (a scale, the opening view): no toggle
    /// is showing any more.
    pub fn forget_toggle(&mut self) {
        self.toggled = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toggle_pressed_again_goes_back() {
        let mut h = ZoomHistory::default();
        assert_eq!(h.frame(1, true, |t| *t == 1, || 5), Some(5));
        assert_eq!(h.frame(5, true, |t| *t == 5, || 5), Some(1));
    }

    #[test]
    fn back_and_forward_walk_the_views() {
        let mut h = ZoomHistory::default();
        h.frame(1, false, |_| false, || 2);
        assert_eq!(h.back(2), Some(1));
        assert_eq!(h.forward(1), Some(2));
        assert_eq!(h.forward(2), None, "nothing ahead");
    }

    #[test]
    fn the_history_keeps_a_bounded_past() {
        let mut h = ZoomHistory::default();
        for i in 0..100 {
            h.frame(i, false, |_| false, || i + 1);
        }
        let mut walked = 0;
        let mut now = 100;
        while let Some(to) = h.back(now) {
            now = to;
            walked += 1;
        }
        assert_eq!(walked, DEPTH);
    }
}

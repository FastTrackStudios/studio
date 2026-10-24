//! The play cursor, the edit cursor and the time selection as the
//! arrangement draws them, and the window's edit cursor for the actions
//! to reach. The cursor types themselves are [`timeline::cursor`]'s.

use std::time::Duration;
pub use timeline::cursor::{
    EditCursor as Edit, MAX_EXTRAPOLATION, MIN_SELECTION, Playhead, TimeSpan as Span,
};
use web_time::Instant;

/// Draw the edit cursor and the time selection.
///
/// Under the play cursor, always. The play cursor is where the audio
/// IS and the edit cursor is where you left off — when they coincide,
/// which is every time you press play from the cursor, the one that
/// moves has to be the one you can see.
pub fn paint_edit(
    painter: &mut impl anyrender::PaintScene,
    palette: &crate::arrangement::Palette,
    edit: &Edit,
    view: crate::arrangement::Viewport,
    origin: (f64, f64),
    top: f64,
    bottom: f64,
) {
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::Fill;

    let (ox, _) = origin;
    // The lanes' left edge. Nothing here draws past it: scrolled right,
    // the cursor and the selection are off the left of the lanes, and
    // without the clamp they painted over the track panel instead.
    let left = ox + view.panel_w;
    let at = |seconds: f64| seconds.mul_add(view.pps, left - view.scroll_x);

    // The selection first, as a wash — it is a region, and a region
    // drawn over its own edges hides them.
    if let Some(span) = edit.selection {
        let (edge0, edge1) = (at(span.start), at(span.end));
        let (x0, x1) = (edge0.max(left), edge1.max(left));
        if x1 > x0 {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                // Strong enough to see at a glance. 0.14 was
                // invisible against the lane backgrounds — a selection
                // you cannot see is one you forget you made, and the
                // next command acts on it.
                palette.accent.with_alpha(0.22),
                None,
                &Rect::new(x0, top, x1, bottom),
            );
            for edge in [edge0, edge1].into_iter().filter(|&edge| edge >= left) {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    palette.accent.with_alpha(0.85),
                    None,
                    &Rect::new(edge, top, edge + 1.0, bottom),
                );
            }
        }
    }

    let x = at(edit.at);
    if x < left {
        return;
    }
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        EDIT_CURSOR,
        None,
        &Rect::new(x, top, x + 1.0, bottom),
    );
}

/// The edit cursor's colour: blue (blue-500), where YOU are — against the
/// play cursor's yellow, where the audio is.
pub const EDIT_CURSOR: vello::peniko::Color =
    vello::peniko::Color::from_rgba8(0x3b, 0x82, 0xf6, 0xff);

/// How the play cursor looks.
///
/// The feature set `ReaSmoothPlayhead` documents — line, trail, shadow,
/// glow — implemented here rather than ported: that extension is closed
/// source and ships as Windows and macOS binaries, so there is nothing
/// to take but the description of what it does.
///
/// The smoothness is not in this struct. It is in [`Playhead`], because
/// it is not a look — a cursor that glides is telling the truth about
/// where the audio is between two reports, and one that steps is
/// rounding that down to the last block.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub line: vello::peniko::Color,
    pub width: f64,
    /// How far the trail reaches BEHIND the cursor, in pixels.
    ///
    /// Behind, because the trail is where the audio has been. A trail
    /// ahead of the playhead would be claiming to know what has not
    /// been played.
    pub trail: f64,
    /// How far the shadow reaches, under everything else.
    pub shadow: f64,
    /// How far the glow bleeds behind the line.
    pub glow: f64,
    /// Opacity at the head of the trail (it falls off from there).
    pub trail_strength: f32,
    /// Opacity at the centre of the glow.
    pub glow_strength: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            // Yellow: where the audio is. The edit cursor is blue — see
            // `EDIT_CURSOR` — so the two never read as one another.
            line: vello::peniko::Color::from_rgba8(0xfa, 0xcc, 0x15, 0xff),
            width: 2.0,
            // A hint of where the audio has been, not a bar following
            // the line around.
            trail: 90.0,
            shadow: 0.0,
            glow: 6.0,
            trail_strength: 0.28,
            glow_strength: 0.5,
        }
    }
}

/// The play cursor's trail, from where it has actually been.
///
/// A trail drawn at a fixed length sits beside the line whether or not
/// anything moves — which is what this replaced. This one remembers the
/// play position against the wall clock and draws back to where the
/// cursor was a moment ago: so it drags out behind the cursor from where
/// it set off, reaches its full length once it has travelled that far,
/// and after a stop the tail keeps coming until it has caught the head.
/// A cursor at rest has no trail.
///
/// How far back "a moment" is depends on the zoom — the time it takes
/// the cursor to cover [`Look::trail`] pixels playing at speed — so the
/// trail is the same length on screen at any zoom, capped at
/// [`Trail::MAX_LAG`] so that far out it does not linger for seconds.
///
/// In project seconds rather than pixels, so a scroll or a zoom while
/// playing does not read as the cursor having moved.
#[derive(Clone, Debug, Default)]
pub struct Trail {
    /// (when, where), oldest first — back as far as `MAX_LAG` and no
    /// further.
    seen: std::collections::VecDeque<(Instant, f64)>,
}

impl Trail {
    /// The longest the tail ever lags the head, in wall-clock time.
    pub const MAX_LAG: Duration = Duration::from_millis(1500);

    /// Where the cursor is now. A jump — a seek, a loop back, a play
    /// from somewhere else — is not motion, so it starts the trail over
    /// rather than smearing it across the gap.
    ///
    /// The same position twice is NOT a stop: the engine moves the
    /// position once an audio block, and a frame can land between two.
    pub fn record(&mut self, now: Instant, at: f64) {
        if let Some(&(then, was)) = self.seen.back() {
            let elapsed = now.saturating_duration_since(then).as_secs_f64();
            // Faster than real time by more than a frame's slack, or
            // backwards at all: not the transport rolling.
            if at < was - 1e-4 || at - was > elapsed + 0.25 {
                self.seen.clear();
            }
        }
        self.seen.push_back((now, at));
        while self.seen.len() > 2
            && self
                .seen
                .get(1)
                .is_some_and(|&(t, _)| now.saturating_duration_since(t) >= Self::MAX_LAG)
        {
            self.seen.pop_front();
        }
    }

    /// Where the cursor was `lag` before `now` — or where the trail's
    /// memory starts, which is where the cursor set off from.
    #[must_use]
    pub fn tail(&self, now: Instant, lag: Duration) -> Option<f64> {
        let (&(t0, x0), rest) = (self.seen.front()?, self.seen.iter().skip(1));
        let Some(horizon) = now.checked_sub(lag) else {
            return Some(x0);
        };
        if t0 >= horizon {
            return Some(x0);
        }
        let mut before = (t0, x0);
        for &(t, x) in rest {
            if t >= horizon {
                let span = t.saturating_duration_since(before.0).as_secs_f64();
                let into = horizon.saturating_duration_since(before.0).as_secs_f64();
                let f = if span > 0.0 { into / span } else { 1.0 };
                return Some(f.mul_add(x - before.1, before.1));
            }
            before = (t, x);
        }
        Some(before.1)
    }

    /// How long the trail is on screen, in pixels, for a full trail of
    /// `full` pixels at `pps`.
    #[must_use]
    pub fn length(&self, now: Instant, pps: f64, full: f64) -> f64 {
        if pps <= 0.0 || full <= 0.0 {
            return 0.0;
        }
        let lag = Duration::from_secs_f64((full / pps).min(Self::MAX_LAG.as_secs_f64()));
        let (Some(&(_, head)), Some(tail)) = (self.seen.back(), self.tail(now, lag)) else {
            return 0.0;
        };
        ((head - tail) * pps).clamp(0.0, full)
    }

    /// Whether there is still a trail to draw — what keeps frames coming
    /// while the tail catches up after a stop.
    #[must_use]
    pub fn alive(&self, now: Instant) -> bool {
        let (Some(&(_, head)), Some(tail)) = (self.seen.back(), self.tail(now, Self::MAX_LAG))
        else {
            return false;
        };
        head - tail > 1e-6
    }
}

impl Look {
    /// This look with its trail `length` pixels long — what
    /// [`Trail::length`] says the cursor has dragged out behind it. The
    /// glow on the line is the line's and stays as it is.
    #[must_use]
    pub fn trailing(self, length: f64) -> Self {
        Self {
            trail: length.clamp(0.0, self.trail),
            ..self
        }
    }
}

/// How many stops a gradient is built from.
///
/// A quadratic falloff is a curve and a gradient is a polyline through
/// it, so this is how round the corner looks. Eight is past the point
/// where more of them changes the picture at these sizes.
const FALLOFF_STOPS: usize = 8;

/// The opacity of a trail `t` of the way from the cursor to its end.
///
/// **Quadratic**, not linear: `(1 - t)²`. A linear trail reads as a
/// solid bar with a hard end, because the eye tracks the end of a ramp
/// rather than its middle. Squaring puts most of the falloff near the
/// cursor, which is what makes it read as motion instead of as a
/// rectangle following the line around.
#[must_use]
pub fn falloff(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let remaining = 1.0 - t;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "an opacity in 0..1, which every gradient API takes as f32"
    )]
    let alpha = (remaining * remaining) as f32;
    alpha
}

/// Draw the play cursor at `x`, from `top` to `bottom`.
///
/// Order matters and is the reverse of how it is read: shadow, then
/// trail, then glow, then the line. Each is wider than the one over it,
/// so drawing them the other way round would bury the line under its
/// own halo.
pub fn paint(
    painter: &mut impl anyrender::PaintScene,
    look: Look,
    x: f64,
    top: f64,
    bottom: f64,
    // `left_bound` is the left edge of the lanes; nothing draws past
    // it. The trail reaches backwards, and at the start of a project
    // that is straight over the track panel — a red wash across the
    // names and faders, which reads as damage rather than as motion.
    // The cursor belongs to the timeline, so it stops where the
    // timeline does.
    left_bound: f64,
) {
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::{ColorStop, ColorStops, Fill, Gradient};

    if bottom <= top {
        return;
    }

    // A horizontal ramp from the line's colour at the cursor to nothing
    // at the far end, sampled off the quadratic.
    let mut ramp = |from: f64, to: f64, peak: f32| {
        let (lo, hi) = (from.min(to).max(left_bound), from.max(to).max(left_bound));
        if hi - lo < 0.5 {
            return;
        }
        let mut stops = ColorStops::new();
        for i in 0..=FALLOFF_STOPS {
            let t = crate::num::coord(i) / crate::num::coord(FALLOFF_STOPS);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::as_conversions,
                reason = "a gradient offset in 0..1, which peniko takes as f32"
            )]
            let offset = t as f32;
            stops.push(ColorStop {
                offset,
                color: look.line.with_alpha(falloff(t) * peak).into(),
            });
        }
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            // The gradient keeps its ORIGINAL span so the falloff is
            // the same shape when it is clipped — anchoring it to the
            // clipped rect instead would compress the whole trail into
            // whatever was left, and the cursor would look different
            // at the start of a project than in the middle of one.
            &Gradient::new_linear((from, top), (to, top)).with_stops(stops),
            None,
            &Rect::new(lo, top, hi, bottom),
        );
    };

    // The shadow first and widest, then the trail over it.
    if look.shadow > 0.0 {
        ramp(x, x - look.shadow, 0.35);
    }
    if look.trail > 0.0 {
        ramp(x, x - look.trail, look.trail_strength);
    }
    // The glow bleeds behind the line only: ahead of the cursor is
    // audio that has not played, and a halo there reads as the cursor
    // being further on than it is.
    if look.glow > 0.0 {
        ramp(x, x - look.glow, look.glow_strength);
    }

    let line_left = (x - look.width / 2.0).max(left_bound);
    let line_right = (x + look.width / 2.0).max(left_bound);
    if line_right > line_left {
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            look.line,
            None,
            &Rect::new(line_left, top, line_right, bottom),
        );
    }
}

/// The arrangement's edit cursor and time selection, as last drawn — for
/// what acts on them from outside the arrangement (the Organize toolbar's
/// inserts, which go through the engine and read ITS cursor and
/// selection, so they are handed these first).
static CURRENT: std::sync::Mutex<Option<Edit>> = std::sync::Mutex::new(None);

/// Publish the arrangement's edit state (every frame it draws).
pub fn publish(edit: Edit) {
    if let Ok(mut slot) = CURRENT.lock() {
        *slot = Some(edit);
    }
}

/// The arrangement's edit state as last drawn, if it has drawn.
#[must_use]
pub fn current() -> Option<Edit> {
    CURRENT.lock().ok().and_then(|slot| *slot)
}

/// An edit state asked for from outside the arrangement, until it takes
/// it ([`take_asked`]).
static ASKED: std::sync::Mutex<Option<Edit>> = std::sync::Mutex::new(None);

/// Put the arrangement's edit cursor and time selection at `edit`.
///
/// What an action that sets them (`TIME_SELECTION_SET`, an insert that moves
/// the cursor on) asks. It is [`current`] at once, and the arrangement
/// takes it on its next frame.
pub fn ask(edit: Edit) {
    publish(edit);
    if let Ok(mut slot) = ASKED.lock() {
        *slot = Some(edit);
    }
}

/// The edit state asked for since the arrangement last looked, once.
#[must_use]
pub fn take_asked() -> Option<Edit> {
    ASKED.lock().ok().and_then(|mut slot| slot.take())
}

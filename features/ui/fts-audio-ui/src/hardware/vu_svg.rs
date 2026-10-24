//! Geometry for an analog VU meter — portable, no Dioxus, unit-testable.
//!
//! Both hardware faces put a VU movement at their centre, so the needle math
//! lives here once. The scale is the standard VU arc: -20 VU at the left stop,
//! 0 VU at roughly three-quarters across, +3 at the right stop, with the
//! familiar non-linear crowding at the bottom of the range.

/// Design-space width of the meter face.
pub const VU_W: f64 = 100.0;
/// Design-space height of the meter face.
pub const VU_H: f64 = 58.0;

/// Needle pivot, well below the visible face, and a long needle from it.
///
/// This is what sets how much of the card the scale occupies, and it is the
/// whole character of the movement: a real VU's scale runs nearly the full
/// width of its card, in a shallow arc, with the needle emerging from a slot
/// at the bottom. A short needle on a close pivot draws a steep little arc
/// down the middle and crowds every number into it — which is what this was
/// doing.
pub const PIVOT_X: f64 = VU_W * 0.5;
pub const PIVOT_Y: f64 = VU_H * 1.52;
/// Needle length from the pivot.
pub const NEEDLE_LEN: f64 = VU_H * 1.40;

/// Half-angle of the sweep, in degrees either side of vertical.
///
/// With the pivot and length above, ±34° puts the stops within about 5% of
/// each edge — the scale uses the card, rather than sitting in the middle of
/// it. [`the_scale_uses_the_width_of_the_card`] is the guard.
pub const SWEEP_DEG: f64 = 34.0;

/// What a movement's card is printed with.
///
/// A VU is a *standard*: -20 to +3 with the crowding at the bottom. Plenty of
/// meters are not VUs at all — a dbx 160 prints a plain decibel scale from -40
/// to +20, evenly spaced — and drawing one on the other's arc is the kind of
/// wrong that a picture makes obvious and a spec sheet never does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VuScale {
    /// The VU standard.
    #[default]
    Vu,
    /// A linear decibel scale, -40 to +20.
    Decibels,
}

impl VuScale {
    /// The ticks this card prints, as (value, label, is_major).
    pub fn ticks(self) -> &'static [(f64, &'static str, bool)] {
        match self {
            Self::Vu => VU_TICKS,
            Self::Decibels => DB_TICKS,
        }
    }

    /// Where a value sits along the scale, 0 = left stop, 1 = right.
    pub fn fraction(self, value: f64) -> f64 {
        match self {
            Self::Vu => vu_to_fraction(value),
            Self::Decibels => ((value.clamp(-40.0, 20.0) + 40.0) / 60.0).clamp(0.0, 1.0),
        }
    }

    /// Needle angle in degrees for a value; 0° is straight up.
    pub fn angle(self, value: f64) -> f64 {
        (self.fraction(value) * 2.0 - 1.0) * SWEEP_DEG
    }

    /// The value a level maps to on this card.
    pub fn from_level_db(self, db: f64) -> f64 {
        match self {
            Self::Vu => db_to_vu(db),
            Self::Decibels => db.clamp(-40.0, 20.0),
        }
    }

    /// The value a gain reduction maps to.
    pub fn from_gain_reduction_db(self, gr_db: f64) -> f64 {
        match self {
            Self::Vu => gr_to_vu(gr_db),
            // A dbx reads reduction as the negative dB it is.
            Self::Decibels => -gr_db.clamp(0.0, 40.0),
        }
    }

    /// The stretch of scale printed in red, if any.
    pub fn hot_from(self) -> Option<f64> {
        match self {
            Self::Vu => Some(0.0),
            // A plain decibel card has no red: it is a readout, not a
            // reference level you are meant to stay under.
            Self::Decibels => None,
        }
    }
}

/// A plain decibel card, evenly spaced.
pub const DB_TICKS: &[(f64, &str, bool)] = &[
    (-40.0, "-40", true),
    (-35.0, "", false),
    (-30.0, "-30", true),
    (-25.0, "", false),
    (-20.0, "-20", true),
    (-15.0, "", false),
    (-10.0, "-10", true),
    (-5.0, "", false),
    (0.0, "0", true),
    (5.0, "", false),
    (10.0, "+10", true),
    (15.0, "", false),
    (20.0, "+20", true),
];

/// The labelled ticks on a VU face, as (VU value, label, is_major).
pub const VU_TICKS: &[(f64, &str, bool)] = &[
    (-20.0, "20", true),
    (-10.0, "10", true),
    (-7.0, "7", false),
    (-5.0, "5", true),
    (-3.0, "3", false),
    (-2.0, "2", false),
    (-1.0, "1", false),
    (0.0, "0", true),
    (1.0, "1", false),
    (2.0, "2", false),
    (3.0, "3", true),
];

/// Map a VU value to its fraction along the scale (0 = left stop, 1 = right).
///
/// Real VU faces are not linear in dB: the bottom of the range is compressed
/// so the -20..-10 stretch takes far less arc than -3..+3. This reproduces
/// that with a power curve on the normalized dB position, which is what makes
/// a drawn face read as a VU rather than as a generic gauge.
pub fn vu_to_fraction(vu: f64) -> f64 {
    const MIN_VU: f64 = -20.0;
    const MAX_VU: f64 = 3.0;
    let t = ((vu.clamp(MIN_VU, MAX_VU) - MIN_VU) / (MAX_VU - MIN_VU)).clamp(0.0, 1.0);
    // Exponent > 1 pushes the low end together and opens up the top.
    t.powf(1.9)
}

/// Needle angle in degrees for a VU value; 0° is straight up.
pub fn vu_to_angle(vu: f64) -> f64 {
    (vu_to_fraction(vu) * 2.0 - 1.0) * SWEEP_DEG
}

/// Tip of the needle for a VU value, in design space.
pub fn needle_tip(vu: f64) -> (f64, f64) {
    let rad = vu_to_angle(vu).to_radians();
    (
        PIVOT_X + NEEDLE_LEN * rad.sin(),
        PIVOT_Y - NEEDLE_LEN * rad.cos(),
    )
}

/// Horizontal extent of the printed scale, as a fraction of the card's width.
pub fn scale_width_fraction() -> f64 {
    let (left, _) = tick_point(-20.0, 0.0);
    let (right, _) = tick_point(3.0, 0.0);
    (right - left) / VU_W
}

/// A point on the tick arc for a scale position, at `inset` from the needle.
pub fn scale_point(scale: VuScale, value: f64, inset: f64) -> (f64, f64) {
    let rad = scale.angle(value).to_radians();
    let r = NEEDLE_LEN - inset;
    (PIVOT_X + r * rad.sin(), PIVOT_Y - r * rad.cos())
}

/// The needle tip for a scale position.
pub fn scale_needle_tip(scale: VuScale, value: f64) -> (f64, f64) {
    let rad = scale.angle(value).to_radians();
    (
        PIVOT_X + NEEDLE_LEN * rad.sin(),
        PIVOT_Y - NEEDLE_LEN * rad.cos(),
    )
}

/// The scale arc as an SVG path for a given card.
pub fn scale_arc_for(scale: VuScale, inset: f64) -> String {
    let ticks = scale.ticks();
    let (first, last) = (ticks[0].0, ticks[ticks.len() - 1].0);
    let (x0, y0) = scale_point(scale, first, inset);
    let (x1, y1) = scale_point(scale, last, inset);
    let r = NEEDLE_LEN - inset;
    format!("M {x0:.2} {y0:.2} A {r:.2} {r:.2} 0 0 1 {x1:.2} {y1:.2}")
}

/// A point on the tick arc for a VU value, at `inset` from the needle length.
pub fn tick_point(vu: f64, inset: f64) -> (f64, f64) {
    let rad = vu_to_angle(vu).to_radians();
    let r = NEEDLE_LEN - inset;
    (PIVOT_X + r * rad.sin(), PIVOT_Y - r * rad.cos())
}

/// The scale arc as an SVG path, at `inset` from the needle length.
pub fn scale_arc_path(inset: f64) -> String {
    let (x0, y0) = tick_point(-20.0, inset);
    let (x1, y1) = tick_point(3.0, inset);
    let r = NEEDLE_LEN - inset;
    // Small sweep, so a single arc segment is exact enough.
    format!("M {x0:.2} {y0:.2} A {r:.2} {r:.2} 0 0 1 {x1:.2} {y1:.2}")
}

/// Gain reduction (dB, positive) shown on a GR-mode VU.
///
/// Hardware GR meters read backwards: no reduction rests at 0 on the right,
/// and the needle swings left as the compressor works. 20 dB of reduction is
/// the left stop.
pub fn gr_to_vu(gr_db: f64) -> f64 {
    -gr_db.clamp(0.0, 20.0)
}

/// Level (dBFS) shown on a level-mode VU, referenced so that -18 dBFS reads
/// 0 VU — the usual digital alignment.
pub fn db_to_vu(db: f64) -> f64 {
    (db + 18.0).clamp(-20.0, 3.0)
}

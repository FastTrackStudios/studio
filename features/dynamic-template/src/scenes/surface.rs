//! Where a size class becomes a number.
//!
//! The scene table says [`Size::Working`], never 133 pixels: the same
//! scene is applied to two panels and to screens from 1080p to a 5120
//! ultrawide, and a rule that stated pixels would need a template per
//! monitor. This is the one place the class is turned into a number, and
//! it lives beside the scenes rather than in a surface so that both
//! surfaces — and the REAPER applier — answer the same table.

use facet::Facet;

use super::types::Size;

/// How a focused track is sized, and what it costs the panel.
#[derive(Facet, Debug, Clone, Copy, PartialEq)]
#[facet(rename_all = "kebab-case")]
pub struct Focus {
    /// How much of a 16:9 screen one focused track takes.
    pub fraction: f64,
    /// What comes off the window's width before the share is taken: the
    /// rails, which are not part of the panel that has to hold four.
    pub gutter: f64,
    /// Never narrower than this, whatever the fraction says — below it a
    /// rack is ornament rather than a set of controls.
    pub legible: f64,
}

impl Focus {
    /// How wide a focused strip opens, for a panel `height` tall.
    ///
    /// A share of what a 16:9 screen of this height would leave between
    /// the rails: 618 at 1440p, 938 at 4K, 458 at 1080p.
    ///
    /// Derived from the HEIGHT rather than the actual width so a wider
    /// ASPECT gets more tracks instead of wider ones. A quarter of the
    /// real width would put four on a 32:9 screen too, each enormous,
    /// when the reason for that screen is that it holds eight. Physical
    /// size follows the display; how many fit follows its shape.
    #[must_use]
    pub fn width(self, height: f64) -> f64 {
        let sixteen_by_nine = height * 16.0 / 9.0;
        let panel = self.gutter.mul_add(-2.0, sixteen_by_nine).max(0.0);
        (panel * self.fraction).max(self.legible)
    }
}

/// The class-to-pixel tables, one per surface, plus the focus
/// parameters.
///
/// Two tables rather than one, because the axes are not the same
/// question: a strip's width buys controls, a row's height buys
/// waveform. That is why the arrangement has no `Focus` that differs
/// from `Working` — past the point where the controls are all drawn at
/// their authored size, more height buys a bigger picture of the same
/// thing rather than another control.
#[derive(Facet, Debug, Clone, Copy, PartialEq)]
#[facet(rename_all = "kebab-case")]
pub struct SurfaceTables {
    /// Mixer strip width per size class, in [`Size::ALL`] order. The
    /// `Focus` slot is a floor: the real width comes from [`Focus`].
    ///
    /// Whole pixels, in the type, because that is what they are — and
    /// because a surface that has to hand REAPER a `u32` should not have
    /// to round a float back into one.
    pub strip_width: [u32; 5],
    /// Arrangement row height per size class, same order.
    pub row_height: [u32; 5],
    /// What a focused strip costs.
    pub focus: Focus,
}

/// The tables the window and the REAPER applier both read.
///
/// The four fixed widths are the ones the mixer already has names for:
/// a rail, REAPER's own strip, and the width at which a rack's curves
/// are readable — which is what "working on this track" means in a mix
/// pass. The heights are multiples of the smallest row that holds a
/// control.
pub const TABLES: SurfaceTables = SurfaceTables {
    // minimum, compact, normal, working, focus
    strip_width: [30, 86, 133, 133, 133],
    row_height: [14, 32, 64, 96, 96],
    focus: Focus {
        fraction: 0.25,
        gutter: 44.0,
        legible: 96.0,
    },
};

impl SurfaceTables {
    /// How wide a size class is in the mixer, for a panel `height` tall.
    ///
    /// Only `Focus` depends on the display, and it depends on the
    /// panel's HEIGHT rather than its width — see [`Focus::width`].
    #[must_use]
    pub fn width(&self, size: Size, height: f64) -> f64 {
        if size == Size::Focus {
            return self.focus.width(height);
        }
        self.strip_width
            .get(size.slot())
            .copied()
            .map_or(self.focus.legible, f64::from)
    }

    /// And how tall one is in the arrangement.
    #[must_use]
    pub fn height(&self, size: Size) -> f64 {
        f64::from(self.height_px(size))
    }

    /// The same, as the whole pixels REAPER's own `TRACKHEIGHT` wants.
    #[must_use]
    pub fn height_px(&self, size: Size) -> u32 {
        self.row_height
            .get(size.slot())
            .copied()
            .unwrap_or_default()
    }
}

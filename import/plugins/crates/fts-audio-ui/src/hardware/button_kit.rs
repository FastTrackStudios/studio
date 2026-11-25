//! What a panel button is *made of*, as data.
//!
//! The third kit, after [`knob_kit`](crate::hardware::knob_kit) and
//! [`vu_kit`](crate::hardware::vu_kit), and the same bargain: one
//! [`ButtonSpec`] const per part, and a renderer that walks it without
//! knowing which unit it is drawing.
//!
//! Before this, a button was one drawing with a colour prop. The faces asked
//! for fourteen different colours and got fourteen recolours of the same
//! part — which is not what tells an 1176's ratio button from an SSL's
//! illuminated square. The *shape* is: whether the cap lights from within or
//! has a jewel under it, whether it sits in a bezel, how far it travels.
//!
//! # Adding a button
//!
//! Add a [`ButtonStyle`](crate::hardware::button::ButtonStyle) variant and one
//! `ButtonSpec` in [`button_parts`](crate::hardware::button_parts). Then look
//! at it:
//!
//! ```sh
//! cargo test -p fts-audio-ui --test button_sheet
//! ```
//!
//! which paints every button in the kit, up and down, wired and unwired.
//!
//! Sizes here are **design px**, scaled by the panel like every other
//! dimension in this crate — not fractions of the cap.
//!
//! That is deliberate and was learned the hard way: a legend is a type size, a
//! throw is a distance, a corner radius is a radius. They are properties of
//! the *part*, not proportions of whatever box a face asks for. Expressed as
//! fractions, a face that wanted a short wide button (comp-ui asks for 40x22)
//! got a legend scaled down with the height until it was unreadable.

/// How the cap takes the light, which is what makes it read as a material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapFinish {
    /// Moulded plastic: lit across the top, matte below.
    Matte,
    /// A glossy cap, with a hard sheen in the upper third.
    Gloss,
    /// Brushed metal — a sweep rather than a wash.
    Metal,
}

impl CapFinish {
    /// The cap in `color`.
    pub fn css(self, color: &str) -> String {
        match self {
            Self::Matte => {
                format!("linear-gradient(180deg, color-mix(in oklab, {color} 88%, white), {color})")
            }
            Self::Gloss => format!(
                "linear-gradient(180deg, color-mix(in oklab, {color} 66%, white) 0%, \
                 color-mix(in oklab, {color} 92%, white) 34%, {color} 52%, \
                 color-mix(in oklab, {color} 82%, black) 100%)"
            ),
            Self::Metal => format!(
                "linear-gradient(152deg, color-mix(in oklab, {color} 62%, white) 0%, \
                 {color} 34%, color-mix(in oklab, {color} 76%, black) 66%, \
                 color-mix(in oklab, {color} 70%, white) 100%)"
            ),
        }
    }
}

/// The cap you press.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cap {
    /// Corner rounding in design px.
    pub radius: f64,
    pub finish: CapFinish,
    /// The line around the cap's edge.
    pub border: &'static str,
}

/// Where the button's light comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lit {
    /// A separate jewel below the cap — the console idiom, and how you read a
    /// channel's state from two feet away.
    Jewel {
        /// Diameter in design px.
        d: f64,
        /// Gap between cap and jewel, in design px.
        gap: f64,
    },
    /// The cap itself lights, and the legend glows through it. An SSL bus
    /// compressor's IN button, and most modern console switching.
    Backlit {
        /// How far the light spills past the cap, in design px.
        bloom: f64,
    },
    /// No light at all. An 1176's ratio buttons are read by which one is
    /// *down*, not by a lamp.
    Unlit,
}

/// A surround the cap sits inside.
///
/// Not every button has one — a bare cap on a panel is the common case — but
/// an illuminated switch usually does, and the bezel is most of what stops
/// the glow bleeding into the panel around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surround {
    pub color: &'static str,
    /// How far it extends past the cap, in design px.
    pub pad: f64,
    /// Corner rounding, in design px.
    pub radius: f64,
}

/// A button, as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonSpec {
    pub cap: Cap,
    pub lit: Lit,
    pub surround: Option<Surround>,
    /// How far the cap sinks when pressed, in design px.
    ///
    /// A latching button that does not visibly travel is read as broken: the
    /// whole point of a mechanical switch is that its *position* is the state.
    pub travel: f64,
    /// Legend type size, in design px.
    pub legend: f64,
}

impl ButtonSpec {
    /// Whether this button lights at all.
    pub fn is_lit(&self) -> bool {
        !matches!(self.lit, Lit::Unlit)
    }

    /// The shadow under the cap, or the one inside it once pressed.
    ///
    /// Up, the cap sits proud and casts downward. Down, the light is cut off
    /// and the shadow is *inside* the opening — the same inversion that tells
    /// a sunken bezel from a raised boss.
    pub fn shadow(&self, pressed: bool, scale: f64) -> String {
        let drop = self.travel * scale;
        let blur = (drop * 2.0).max(2.0);
        let lip = (1.0 * scale).clamp(0.6, 1.6);
        if pressed {
            // Down in its well: the light that was on the top lip is now on
            // the bottom one, and the cast shadow becomes a shadow falling
            // INTO the button. Swapping only the drop shadow, which is what
            // this did, left the cap looking identical at a glance — and a
            // latching switch whose position you cannot see is read as
            // broken.
            format!(
                "inset 0 {lip:.1}px {:.1}px rgba(0,0,0,0.55), \
                 inset 0 -{lip:.1}px 0 rgba(255,255,255,0.10), \
                 inset 0 {:.1}px {blur:.1}px rgba(0,0,0,0.45)",
                lip * 1.5,
                drop.max(1.0),
            )
        } else {
            // Standing proud: a lit chamfer along the top edge, a dark one
            // under it, and the cast shadow on the panel. The chamfer is what
            // was missing — a flat fill with a drop shadow is a sticker.
            format!(
                "inset 0 {lip:.1}px 0 rgba(255,255,255,0.34), \
                 inset 0 -{lip:.1}px 0 rgba(0,0,0,0.34), \
                 0 {:.1}px {blur:.1}px rgba(0,0,0,0.45)",
                drop.max(1.0),
            )
        }
    }

    /// How far the cap has physically moved, in px. A button that only
    /// changes shading has not been pressed; it has been recoloured.
    #[must_use]
    pub fn press_offset(&self, pressed: bool, scale: f64) -> f64 {
        if pressed {
            (self.travel * scale * 0.6).max(0.5)
        } else {
            0.0
        }
    }
}

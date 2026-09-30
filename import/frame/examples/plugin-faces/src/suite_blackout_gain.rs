//! Suite: **Blackout Gain** (`suite-blackout-gain`) — a single-amp
//! high-gain suite in the plugin's black "blackout" skin, recreated unit by
//! unit as frame surfaces: three pedals, the head, a 4×12, a tabletop
//! graphic EQ, and two post-FX boxes.
//!
//! | file | unit | form |
//! |---|---|---|
//! | `01-pre-standard-ward-gate` | Ward: one-knob noise gate, petal rosette, twin skulls, hollow name | Pedal Standard 156 × 286 |
//! | `02-pre-standard-omen-drive` | Omen: three-knob drive, skulls face to face, hexagram round the red LED | Pedal Standard |
//! | `03-pre-standard-maul-boost` | Maul: one-knob boost, knot lattice, star medallion | Pedal Standard |
//! | `04-amp-head-blackout` | Blackout head: black leather, black panel, green jewel, seven knobs, I/II inputs | Head 1200 × 460 (the plugin's 2.9 : 1 faceplate plus its handle) |
//! | `05-cab-4x12` | 4×12 cabinet with two mics and room | Cab 900 × 900 |
//! | `06-eq-tabletop-nine-band` | Nine-band graphic EQ, blue-lit faders, HPF/LPF | Tabletop 1000 × 302 (3.3 : 1) |
//! | `07-post-large-crypt-delay` | Crypt Delay: LCD, SYNC/PING-PONG, ENGAGE + TAP | Pedal Large 286 × 286 (the original is near square) |
//! | `08-post-standard-tomb-verb` | Tomb Verb: MIX / DECAY / TONE | Pedal Standard |
//!
//! No real maker, artist or product name is printed: the maker's badge
//! reads an invented "VANTOM", and each unit has an invented name set in
//! the original's lettering style (tall condensed display capitals, the
//! gate's hollow block letters, the badge's squared extended caps).

pub mod amp;
pub mod art;
pub mod cab;
pub mod eq;
pub mod post;
pub mod pre;

use crate::face::Face;

/// The suite's plugin id.
pub const PLUGIN: &str = "suite-blackout-gain";

/// The component every pedal knob of the suite is placed as — one line to
/// switch the suite to another knob (the GPU `knob.3d` when it lands).
pub const KNOB_ID: &str = "knob.render";

/// The pedals' knob: glossy black fluted plastic with a white index.
#[must_use]
pub const fn pedal_knob(d: f64) -> crate::rigkit::KnobStyle<'static> {
    crate::rigkit::KnobStyle::new(KNOB_ID, "fluted", d, art::INK).opts(&["color=#121213", "pointer=#f2efe8", "ticks=none", "spec=1.3", "bevel=1.2"])
}

/// The component for a knob of kind `id` (the amp's, the EQ's, the cab
/// strip's) — every other knob of the suite is placed through this.
#[must_use]
pub const fn knob_id(id: &str) -> &str {
    id
}

/// Every unit of the suite, in signal order.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v = pre::faces();
    v.push(amp::face());
    v.push(cab::face());
    v.push(eq::face());
    v.extend(post::faces());
    v
}

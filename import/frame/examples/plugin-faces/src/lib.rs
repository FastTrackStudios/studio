//! Hardware-style plugin faces built from `surface-kit`.
//!
//! Each module returns its [`Face`]s; [`all`] collects them and
//! `cargo run -p plugin-faces -- OUT_DIR` writes `OUT_DIR/<plugin>/<file>.fm`
//! plus `OUT_DIR/<plugin>/<plugin>.catalog.json`, so `frame OUT_DIR/<plugin>`
//! plays a plugin's faces as a gallery.

pub mod animate;
pub mod comp_fet;
pub mod comp_tube;
pub mod comp_vca;
pub mod delay;
pub mod eq;
pub mod face;
pub mod modulation;
pub mod reverb;
pub mod saturate;

pub use face::{Face, canvas, catalogs};

#[must_use]
pub fn all() -> Vec<Face> {
    let mut v = Vec::new();
    v.extend(comp_fet::faces());
    v.extend(comp_tube::faces());
    v.extend(comp_vca::faces());
    v.extend(eq::faces());
    v.extend(delay::faces());
    v.extend(reverb::faces());
    v.extend(saturate::faces());
    v.extend(modulation::faces());
    v
}

//! Oil Can: placeholder until its machine is drawn.

use crate::face::Face;
use crate::rig_time::kit;

#[must_use]
pub fn face() -> Face {
    kit::delay_face("69-dly-oil-can", kit::stub(kit::DLY, "Oil Can", "#c08040"))
}

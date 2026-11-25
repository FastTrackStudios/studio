//! Scenes: what a scene stores for each fixture, and the `SCENE` element.
//!
//! A scene holds one `FIXTUREDATA` per fixture it touches, `DATA` being a raw
//! (sync-flushed) zlib stream, base64:
//!
//! ```text
//! uint16be  channel count
//! uint16be  value per channel, 0..255, or 0xFFFF for "not in this scene"
//! uint32be  cell count (1 for a single-cell fixture)
//! per cell  13 x float16be, then one byte (0)
//! ```
//!
//! The per-cell block is what the desk draws on its stage view: slots 0-2
//! the cell's red, green and blue, slot 3 its warm white, each 0..1 or -1
//! when the scene does not set it. Slots 8 and 11 are 1.0 in most scenes
//! read and are written so; the rest are -1. It is a picture of the scene,
//! not its output: the values above are what goes to DMX.

use crate::Error;
use crate::container::{Encoding, pack_raw};
use crate::patch::{Patch, stable_uid};
use crate::profile::{Kind, cells, channels};
use crate::xml::Element;

/// The value meaning "this scene does not touch the channel".
pub const UNSET: u16 = 0xFFFF;

const HALF_ONE: u16 = 0x3c00;
const HALF_MINUS_ONE: u16 = 0xbc00;

/// One scene: a name, a button colour, and values per fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    pub name: String,
    /// `#aarrggbb`, the button's colour on the desk.
    pub color: String,
    /// `(fixture DASUID, one value per channel; None leaves it alone)`.
    pub fixtures: Vec<(String, Vec<Option<u8>>)>,
    /// The scene's steps. Empty is one step holding `fixtures`, which is
    /// how the desk stores a static scene.
    pub steps: Vec<Step>,
    /// Effect racks written as given ([`crate::fx`]: Color FX, Move FX).
    /// A scene with racks and no steps is an effect scene: `fixtures` are
    /// its static values (dimmers, shutters) and no steps rack is written,
    /// as the desk stores one.
    pub racks: Vec<Element>,
    /// `RELEASE_MODE`: what starting this scene stops. 2 (the desk's
    /// default) the rest of its bank; 0 nothing, for scenes a Super Scene
    /// layers on top of each other.
    pub release_mode: u8,
    /// `LTPPRIORITY`, the scene priority: a scene only overrides another
    /// one's channels at an equal or higher priority. 2 is the desk's
    /// normal; a Super Scene's base looks go under (1) the hits laid over
    /// them (3), or the looks' every step would win back the channels.
    pub priority: u8,
}

/// One step of a Steps scene: a look, held `wait` then faded into the
/// next over `fade`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Hold, in the desk's 1/25 s ticks (25 is one second).
    pub wait: u32,
    /// Fade into the next step, in the same ticks.
    pub fade: u32,
    pub fixtures: Vec<(String, Vec<Option<u8>>)>,
}

/// A step's or a scene's `FIXTUREDATAS`.
fn fixture_datas(
    scene: &str,
    fixtures: &[(String, Vec<Option<u8>>)],
    patch: &Patch,
) -> Result<Element, Error> {
    let mut datas = Element::new("FIXTUREDATAS").with("NB", fixtures.len());
    for (uid, values) in fixtures {
        let fixture = patch
            .fixtures
            .iter()
            .find(|f| &f.uid == uid)
            .ok_or_else(|| Error::Patch(format!("scene {scene}: no fixture {uid}")))?;
        let profile = patch
            .profile(&fixture.profile)
            .ok_or_else(|| Error::Patch(format!("{} has no profile", fixture.name)))?;
        let body = fixture_data(profile, values).ok_or_else(|| {
            Error::Patch(format!(
                "scene {scene}: {} needs {} values",
                fixture.name,
                channels(profile).len()
            ))
        })?;
        datas.push(
            Element::new("FIXTUREDATA")
                .with("FIXTURE", uid)
                .with("DATA", pack_raw(&body, Encoding::Base64)?),
        );
    }
    Ok(datas)
}

impl Scene {
    #[must_use]
    pub fn new(name: &str, color: &str) -> Self {
        Self {
            name: name.to_owned(),
            color: color.to_owned(),
            fixtures: Vec::new(),
            steps: Vec::new(),
            racks: Vec::new(),
            release_mode: 2,
            priority: 2,
        }
    }

    /// The scene's `DASUID` in bank `bank`: what a Super Scene's block
    /// names it by (`SCENEUUID`).
    #[must_use]
    pub fn uid(&self, bank: &str) -> String {
        stable_uid(&format!("scene:{bank}/{}", self.name))
    }

    /// The `SCENE` element, against the patch it was written for.
    ///
    /// # Errors
    /// A fixture is not in the patch, or its values do not match its
    /// channel count.
    pub fn to_element(&self, bank: &str, patch: &Patch) -> Result<Element, Error> {
        // The desk keeps the scene's own values and a steps rack; a
        // static scene is a rack of one step holding the same values.
        let held = [Step {
            wait: 25,
            fade: 0,
            fixtures: self.fixtures.clone(),
        }];
        let steps: &[Step] = if self.steps.is_empty() {
            &held
        } else {
            &self.steps
        };
        let top = if self.fixtures.is_empty() {
            steps.first().map_or(&self.fixtures, |s| &s.fixtures)
        } else {
            &self.fixtures
        };
        let datas = fixture_datas(&self.name, top, patch)?;
        let mut racks = Element::new("RACKS");
        if !self.steps.is_empty() || self.racks.is_empty() {
            let mut list = Element::new("STEPS").with("NB", steps.len());
            for step in steps {
                list.push(
                    Element::new("STEP")
                        .with("WAITTIME", step.wait)
                        .with("FADETIME", step.fade)
                        .child(fixture_datas(&self.name, &step.fixtures, patch)?),
                );
            }
            racks.push(Element::new("RACK").with("TYPE", 9).child(list));
        }
        for rack in &self.racks {
            racks.push(rack.clone());
        }
        Ok(Element::new("SCENE")
            .with("DASUID", self.uid(bank))
            .with("NAME", &self.name)
            .with("COLOR", &self.color)
            .with("ENABLE", 1)
            .with("VISIBLE", 1)
            .with("LOOP_MODE", 0)
            .with("LOOP", 0)
            .with("PLAY_DIRECTION", 1)
            .with("PLAY_MODE", 0)
            .with("FLASH", 0)
            .with("BOUNCE_MODE", 0)
            .with("PLAY_TRIGGER", 0)
            .with("PLAY_DIVISION", 8)
            .with("DIMMER", 1)
            .with("D_FEATURE", 1)
            .with("SPEED", 0.5)
            .with("PHASE", 0)
            .with("SIZE", 1)
            .with("PHASE_IN", 0)
            .with("PHASE_OUT", 0)
            .with("AUTO_RELEASE_END", 1)
            .with("JUMP_MODE", -1)
            // 2 = Bank (the manual's default): starting this scene stops
            // the others in its bank, so a bank plays one scene at a time.
            .with("RELEASE_MODE", self.release_mode)
            .with("RELEASE_PROTECT_MODE", 0)
            .with("FADE_IN", 0)
            .with("FADE_OUT", 0)
            .with("ATTRIBUTEVALUE_MODE", 0)
            .with("LTPPRIORITY", self.priority)
            .with("PAUSE", 0)
            .with("SAIMAGEPATH", "")
            .child(datas)
            .child(racks))
    }
}

/// The plaintext of one `FIXTUREDATA`: values, then the per-cell picture.
/// `None` when `values` does not have one entry per channel.
#[must_use]
pub fn fixture_data(profile: &Element, values: &[Option<u8>]) -> Option<Vec<u8>> {
    let chans = channels(profile);
    if chans.len() != values.len() {
        return None;
    }
    let mut out = Vec::new();
    out.extend(u16::try_from(values.len()).ok()?.to_be_bytes());
    for v in values {
        out.extend(v.map_or(UNSET, u16::from).to_be_bytes());
    }
    let cell_count = cells(profile).len().max(1);
    out.extend(u32::try_from(cell_count).ok()?.to_be_bytes());
    for cell in 0..cell_count {
        // red, green, blue, warm white as seen on this cell.
        let mut seen: [Option<u8>; 4] = [None; 4];
        for (c, v) in chans.iter().zip(values) {
            let on_cell = c.beams.is_empty() || c.beams.iter().any(|b| usize::from(*b) == cell);
            let slot = match c.kind {
                Kind::Red => 0,
                Kind::Green => 1,
                Kind::Blue => 2,
                Kind::WarmWhite => 3,
                _ => continue,
            };
            if on_cell && let (Some(v), Some(s)) = (v, seen.get_mut(slot)) {
                *s = Some(*v);
            }
        }
        let mut halves = [HALF_MINUS_ONE; 13];
        for (h, s) in halves.iter_mut().zip(seen) {
            if let Some(v) = s {
                *h = half_of_level(v);
            }
        }
        for i in [8, 11] {
            if let Some(h) = halves.get_mut(i) {
                *h = HALF_ONE;
            }
        }
        for h in halves {
            out.extend(h.to_be_bytes());
        }
        out.push(0);
    }
    Some(out)
}

/// `v / 255` as an IEEE half float, truncated as the desk does.
#[must_use]
pub fn half_of_level(v: u8) -> u16 {
    if v == 0 {
        return 0;
    }
    if v == 255 {
        return HALF_ONE;
    }
    let x = f32::from(v) / 255.0;
    let bits = x.to_bits();
    // 0 < x < 1, so x is a normal float with a negative exponent that a
    // half float also holds as a normal number (v >= 1 gives x >= 2^-8).
    let exp32 = (bits >> 23) & 0xff;
    let mant32 = bits & 0x007f_ffff;
    let exp16 = exp32.saturating_sub(127 - 15);
    // The desk truncates the dropped bits rather than rounding.
    let half = (exp16 << 10) | (mant32 >> 13);
    u16::try_from(half).unwrap_or(HALF_ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_match_the_desk() {
        // Values read from real scenes.
        assert_eq!(half_of_level(255), 0x3c00);
        assert_eq!(half_of_level(0), 0);
        assert_eq!(half_of_level(0x50), 0x3505);
        // (A pink stored 0xfc as 0x3beb: the desk's picture there came from
        // its colour picker, not from the DMX value.)
        assert_eq!(half_of_level(0x6e), 0x36e6);
        assert_eq!(half_of_level(0xbe), 0x39f5);
    }
}

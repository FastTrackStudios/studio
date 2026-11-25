//! What a recording from Ignition needs before the desk plays it on the
//! mobile rig: the places where Ignition's fixture types and the real
//! fixtures in their DMX modes disagree.
//!
//! - RockStrip colour ([`recolour_strips`]): Ignition sends the strip a
//!   level per section and no colour; the hue comes from the wall bars.
//! - Tower colour ([`spread_tower_colour`]): Ignition colours a tower
//!   through its bar-wide RGB, the real lamps answer to their own.
//! - Mover shutters ([`open_shutters`]): Ignition leaves the shutter
//!   closed and drives the dimmer.
//!
//! [`Rig::recoloured`] applies all three to every frame.

use crate::patch::{Fixture, Patch};

/// Start addresses, left to right as the audience sees them, of the
/// fixtures each fixup touches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rig {
    pub strips: Vec<u16>,
    pub bars: Vec<u16>,
    pub towers: Vec<u16>,
    pub movers: Vec<u16>,
}

impl Rig {
    /// The mobile rig's fixtures by name: `RockStrip`, `Wall Bar`,
    /// `Tower`, `Mover`, universe 1.
    #[must_use]
    pub fn of(patch: &Patch) -> Self {
        Self {
            strips: starts_by_x(patch, "RockStrip"),
            bars: starts_by_x(patch, "Wall Bar"),
            towers: starts_by_x(patch, "Tower"),
            movers: starts_by_x(patch, "Mover"),
        }
    }

    /// A recording with the strips of every frame recoloured
    /// ([`recolour_strips`]), every tower's colour on its lamps
    /// ([`spread_tower_colour`]) and the movers' shutters open
    /// ([`open_shutters`]).
    #[must_use]
    pub fn recoloured(&self, frames: &[Vec<u8>]) -> Vec<Vec<u8>> {
        let mut out = frames.to_vec();
        for frame in &mut out {
            recolour_strips(frame, &self.strips, &self.bars);
            spread_tower_colour(frame, &self.towers);
            open_shutters(frame, &self.movers);
        }
        out
    }
}

/// Start addresses of the universe-1 fixtures whose name starts with
/// `prefix`, left to right as the audience sees them.
#[must_use]
pub fn starts_by_x(patch: &Patch, prefix: &str) -> Vec<u16> {
    let mut list: Vec<&Fixture> = patch
        .fixtures
        .iter()
        .filter(|f| f.universe == 1 && f.name.starts_with(prefix))
        .collect();
    list.sort_by_key(|f| f.x);
    list.into_iter().map(|f| f.address).collect()
}

/// The strips' colour, taken from the wall bars.
///
/// A RockStrip 252 in its 28ch mode is eight R, G, B cells, then master
/// dimmer, strobe, chase and function. Ignition's own type for that mode
/// reads 1-24 as 24 single-colour section levels, so what it sends there
/// is a level per section and no colour (a blue look sends 217 on all 24:
/// white on the real strip).
///
/// Ignition writes its one Dimmer value to all 25 of those bytes. So
/// each strip cell keeps that level (the brightest of its three sections;
/// the master byte is the same value again, not a second level) and takes its hue from the back-wall
/// Solena bars, whose cells Ignition does colour: the bars' cells, left to
/// right, are spread over the strips' cells left to right, so a roll on
/// the wall rolls along the front too. A dark bar cell borrows the bars'
/// average hue, and with the bars all dark the strip is white. Master
/// dimmer full (the level is in the cells), strobe, chase and function 0.
pub fn recolour_strips(frame: &mut [u8], strips: &[u16], bars: &[u16]) {
    const CELLS: usize = 8;
    let byte = |frame: &[u8], at: usize| frame.get(at).copied().unwrap_or(0);
    let rgb_at = |frame: &[u8], at: usize| -> [u8; 3] {
        [byte(frame, at), byte(frame, at.saturating_add(1)), byte(frame, at.saturating_add(2))]
    };
    let hue: Vec<[u8; 3]> = bars
        .iter()
        .flat_map(|a| {
            let start = usize::from(*a).saturating_sub(1);
            (0..CELLS).map(move |c| start.saturating_add(c.saturating_mul(3)))
        })
        .map(|at| rgb_at(frame, at))
        .collect();
    let lit: Vec<&[u8; 3]> = hue.iter().filter(|c| c.iter().any(|v| *v > 0)).collect();
    let n = u32::try_from(lit.len()).unwrap_or(1);
    let average: [u8; 3] = [0, 1, 2].map(|k| {
        let sum: u32 = lit.iter().map(|c| u32::from(c.get(k).copied().unwrap_or(0))).sum();
        u8::try_from(sum.checked_div(n).unwrap_or(255)).unwrap_or(255)
    });
    let total = strips.len().saturating_mul(CELLS);
    for (s, address) in strips.iter().enumerate() {
        let start = usize::from(*address).saturating_sub(1);
        for c in 0..CELLS {
            let at = start.saturating_add(c.saturating_mul(3));
            let level = rgb_at(frame, at).into_iter().max().map_or(0, u32::from);
            let global = s.saturating_mul(CELLS).saturating_add(c);
            let source = global
                .saturating_mul(hue.len())
                .checked_div(total)
                .and_then(|i| hue.get(i))
                .filter(|h| h.iter().any(|v| *v > 0))
                .copied()
                .unwrap_or(average);
            let peak = source.iter().copied().max().map_or(0, u32::from);
            for (k, v) in source.into_iter().enumerate() {
                let out = u32::from(v)
                    .saturating_mul(level)
                    .checked_div(peak)
                    .unwrap_or(0);
                if let Some(b) = frame.get_mut(at.saturating_add(k)) {
                    *b = u8::try_from(out).unwrap_or(255);
                }
            }
        }
        for (k, v) in [(24_usize, 255_u8), (25, 0), (26, 0), (27, 0)] {
            if let Some(b) = frame.get_mut(start.saturating_add(k)) {
                *b = v;
            }
        }
    }
}

/// A ZQ01334 tower's colour on every lamp.
///
/// Ignition colours a tower
/// through its bar-wide RGB (channels 9-11), but on the real fixture in
/// 33ch mode the colour LEDs answer to each lamp's own R, G, B (16-33).
/// Where a frame leaves every lamp dark and sets the bar-wide colour, each
/// lamp takes that colour; the bar-wide channels are kept as they were.
pub fn spread_tower_colour(frame: &mut [u8], towers: &[u16]) {
    const BAR_RGB: usize = 8;
    const LAMPS: usize = 15;
    for a in towers {
        let start = usize::from(*a).saturating_sub(1);
        let at = |k: usize| start.saturating_add(k);
        let lamps_dark = (0..18_usize).all(|k| frame.get(at(LAMPS.saturating_add(k))).is_none_or(|v| *v == 0));
        let rgb: Vec<u8> = (0..3_usize).map(|k| frame.get(at(BAR_RGB.saturating_add(k))).copied().unwrap_or(0)).collect();
        if !lamps_dark || rgb.iter().all(|v| *v == 0) {
            continue;
        }
        for lamp in 0..6_usize {
            for (k, v) in rgb.iter().enumerate() {
                if let Some(b) = frame.get_mut(at(LAMPS.saturating_add(lamp.saturating_mul(3)).saturating_add(k))) {
                    *b = *v;
                }
            }
        }
    }
}

/// The Mini Gobo's shutter (channel 7) held open.
///
/// Ignition drives the
/// mover's level through its dimmer (channel 8) and leaves the shutter at
/// 0, which is closed: the beam moved but never lit. The Gearbox show's
/// mover scenes hold it at 255, so the dimmer alone decides the light.
pub fn open_shutters(frame: &mut [u8], movers: &[u16]) {
    const SHUTTER: usize = 6;
    for a in movers {
        let at = usize::from(*a).saturating_sub(1).saturating_add(SHUTTER);
        if let Some(b) = frame.get_mut(at) {
            *b = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mover_shutter_opens_and_a_dark_tower_takes_its_bar_colour() {
        let mut frame = vec![0_u8; 64];
        open_shutters(&mut frame, &[1]);
        assert_eq!(frame.get(6), Some(&255));
        let mut frame = vec![0_u8; 64];
        // Tower at 1: bar-wide RGB is channels 9-11 (bytes 8-10).
        for (i, v) in [(8, 10_u8), (9, 20), (10, 30)] {
            if let Some(b) = frame.get_mut(i) {
                *b = v;
            }
        }
        spread_tower_colour(&mut frame, &[1]);
        assert_eq!(frame.get(15..18), Some(&[10_u8, 20, 30][..]));
        assert_eq!(frame.get(30..33), Some(&[10_u8, 20, 30][..]));
    }
}

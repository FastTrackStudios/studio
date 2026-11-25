//! Profiles Daslight's own library lacks, built from each fixture's manual
//! (`features/lighting/data/fixtures/*.json` holds the charts and sources).

use crate::Error;
use crate::profile::{Body, Channel, Kind, Preset, append_channels, library};
use crate::xml::Element;

/// The 11-channel "Mini Gobo Moving Head Light", the desk's own profile.
///
/// X, µX, Y, µY, colour wheel, gobo, shutter, dimmer, speed, function,
/// dimmer modes; two beams. `_varied/Mini Gobo Moving Head Light.ssl2`,
/// copied verbatim out of a showfile that patched it. Its µ channels'
/// names are stored double-encoded (`Âµ`), as the desk wrote them.
///
/// # Errors
/// Never in practice: the profile is part of the crate.
pub fn mini_gobo_moving_head_11ch() -> Result<Element, Error> {
    Element::parse(include_str!("profiles/mini-gobo-moving-head-11ch.xml"))
}

/// A stable `SSLFIXUID` for a profile this crate writes: the same name
/// always gives the same id, so a regenerated showfile does not look like a
/// new fixture type to the desk.
#[must_use]
pub fn profile_uid(name: &str) -> String {
    let id = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("ignition-daslight:{name}").as_bytes(),
    );
    let tail: String = id.simple().to_string().chars().take(8).collect();
    format!("{id}-{tail}")
}

/// U'King ZQ01334 in its 33-channel mode, made from the desk's own 15-channel
/// profile (`UKing/ZQ01334.ssl2`), so channels 1-15 stay byte for byte what
/// the desk ships.
///
/// Channels 16-33 are R, G, B of cells 1-6. The profile's cell grid already
/// has twelve cells: 0-5 the warm-white COBs, 6-11 the RGB surround behind
/// them, so cell `n`'s colour is drawn on beam `6 + n`.
#[must_use]
pub fn zq01334_33ch(desk_15ch: &Element) -> Element {
    let mut lib = desk_15ch.clone();
    lib.set("SSLFIXUID", profile_uid("UKing ZQ01334 33CH"));
    lib.set("SSLNAME", "UKing/ZQ01334 33CH.ssl2");
    // Six lamps 21 cm apart: the bar EasyView draws is as long as the
    // tower's stack (the desk's profile says 30 cm).
    if let Some(p) = lib.find_mut("SSLPROPERTIES") {
        p.set("SSLSIZEX", 128);
    }
    let mut extra = Vec::new();
    for cell in 0..6_u16 {
        let beam = cell.saturating_add(6);
        let n = cell.saturating_add(1);
        // type_index 1.. so they never collide with the global R/G/B (0).
        extra.push(
            Channel::new(Kind::Red, &format!("{n}. Red"))
                .index(n)
                .beam(beam),
        );
        extra.push(
            Channel::new(Kind::Green, &format!("{n}. Green"))
                .index(n)
                .beam(beam),
        );
        extra.push(
            Channel::new(Kind::Blue, &format!("{n}. Blue"))
                .index(n)
                .beam(beam),
        );
    }
    append_channels(&mut lib, &extra);
    lib
}

/// Rockville RockStrip 252 in its 28-channel mode: eight cells of R, G, B
/// (1-24), then master dimmer, strobe/speed, chase mode and function
/// (0: dimmer mode, the cells as set).
///
/// The 1-24 layout is not confirmed against Rockville's manual; the
/// showfile's ROCKSTRIP CHANNELS bank lights each channel on its own to
/// check it.
#[must_use]
pub fn rockstrip_252_28ch() -> Element {
    let body = Body {
        fixture_type: 3,
        size_cm: (100, 6, 6),
        beam_deg: 30,
        pan_tilt_deg: (360, 220),
        manufacturer: "Rockville".into(),
        model: "RockStrip 252".into(),
        icon: "ScanLibrary/icon/Fixtures/_Generic/Par.ico".into(),
        // The desk's own LED Bar's model: EasyView draws each cell.
        object_3d: ":/FIXTURES/LEDS/LED".into(),
        cells: (0..8).map(|i: i32| (i.saturating_mul(100), 0)).collect(),
    };
    let mut ch = Vec::new();
    for s in 0..8_u16 {
        let n = s.saturating_add(1);
        for (kind, colour) in [(Kind::Red, "Red"), (Kind::Green, "Green"), (Kind::Blue, "Blue")] {
            ch.push(
                Channel::new(kind, &format!("{n}. {colour}"))
                    .index(s)
                    .beam(s),
            );
        }
    }
    ch.push(Channel::new(Kind::Dimmer, "Dimmer"));
    ch.push(Channel::new(Kind::Strobe, "Strobe/Speed"));
    ch.push(Channel::new(Kind::Macro, "Chase Mode"));
    ch.push(Channel::new(Kind::Other, "Function").presets(vec![
        Preset::range("Dimmer mode", 0, 0),
        Preset::range("Functions", 1, 255),
    ]));
    library(
        &profile_uid("Rockville RockStrip 252 28CH"),
        "Rockville/Rockville Rockstrip 252 28CH.ssl2",
        &body,
        &ch,
    )
}

/// Solena Max Bar 28 RGB in its 24-channel mode: eight sections, each an
/// R, G, B triplet, no master dimmer or strobe (solena-max-bar-28-rgb.json).
#[must_use]
pub fn solena_max_bar_24ch() -> Element {
    let body = Body {
        fixture_type: 3,
        size_cm: (100, 8, 8),
        beam_deg: 30,
        pan_tilt_deg: (360, 220),
        manufacturer: "Solena".into(),
        model: "Max Bar 28 RGB".into(),
        icon: "ScanLibrary/icon/Fixtures/_Generic/Par.ico".into(),
        // The desk's own LED Bar's model: EasyView draws each cell.
        object_3d: ":/FIXTURES/LEDS/LED".into(),
        cells: (0..8).map(|i: i32| (i.saturating_mul(100), 0)).collect(),
    };
    let mut ch = Vec::new();
    for s in 0..8_u16 {
        let n = s.saturating_add(1);
        ch.push(
            Channel::new(Kind::Red, &format!("{n}. Red"))
                .index(s)
                .beam(s),
        );
        ch.push(
            Channel::new(Kind::Green, &format!("{n}. Green"))
                .index(s)
                .beam(s),
        );
        ch.push(
            Channel::new(Kind::Blue, &format!("{n}. Blue"))
                .index(s)
                .beam(s),
        );
    }
    library(
        &profile_uid("Solena Max Bar 28 RGB 24CH"),
        "Solena/Max Bar 28 RGB 24CH.ssl2",
        &body,
        &ch,
    )
}

/// Betopper LB150 150 W LED beam moving head, 12-channel mode
/// (betopper-150w-beam.json).
#[must_use]
pub fn betopper_150w_beam_12ch() -> Element {
    let body = Body {
        fixture_type: 0,
        size_cm: (24, 17, 33),
        beam_deg: 2,
        pan_tilt_deg: (540, 270),
        manufacturer: "Betopper".into(),
        model: "LB150 150W LED Beam".into(),
        icon: String::new(),
        object_3d: ":/FIXTURES/SCANS/HEAD".into(),
        cells: Vec::new(),
    };
    let colours: Vec<Preset> = std::iter::once(Preset::range("White", 0, 10))
        .chain((1..=11_u8).map(|i| {
            let start = i.saturating_mul(5).saturating_add(6);
            Preset::range(&format!("Color {i}"), start, start.saturating_add(4))
        }))
        .chain([
            Preset::range("White ", 66, 191),
            Preset::range("Scroll CW", 192, 225),
            Preset::range("Scroll CCW", 226, 255),
        ])
        .collect();
    let gobos: Vec<Preset> = std::iter::once(Preset::range("Open", 0, 10))
        .chain((1..=13_u8).map(|i| {
            let start = i.saturating_mul(5).saturating_add(6);
            Preset::range(&format!("Gobo {i}"), start, start.saturating_add(4))
        }))
        .chain(std::iter::once(Preset::range("Open ", 76, 80)))
        .chain((1..=13_u8).map(|i| {
            let start = i.saturating_mul(7).saturating_add(74);
            Preset::range(&format!("Gobo {i} shake"), start, start.saturating_add(6))
        }))
        .chain([
            Preset::range("Open  ", 172, 180),
            Preset::range("Gobo scroll CW", 181, 221),
            Preset::range("Gobo scroll CCW", 222, 255),
        ])
        .collect();
    let ch = vec![
        Channel::new(Kind::Pan, "Pan"),
        Channel::new(Kind::PanFine, "Pan fine"),
        Channel::new(Kind::Tilt, "Tilt"),
        Channel::new(Kind::TiltFine, "Tilt fine"),
        Channel::new(Kind::Speed, "Pan/Tilt speed"),
        Channel::new(Kind::Dimmer, "Dimmer"),
        Channel::new(Kind::Strobe, "Strobe").presets(vec![
            Preset::range("Closed", 0, 10),
            Preset::range("Strobe slow-fast", 11, 99),
            Preset::range("Open", 100, 109),
            Preset::range("Pulse strobe", 110, 179),
            Preset::range("Open ", 180, 189),
            Preset::range("Random strobe", 190, 250),
            Preset::range("Open  ", 251, 255),
        ]),
        Channel::new(Kind::Other, "Color wheel").presets(colours),
        Channel::new(Kind::Gobo, "Gobo").presets(gobos),
        Channel::new(Kind::Other, "Prism").index(1).presets(vec![
            Preset::range("Prism off", 0, 10),
            Preset::range("Prism in", 11, 155),
            Preset::range("Stop", 156, 157),
            Preset::range("Spin CCW", 158, 205),
            Preset::range("Stop ", 206, 207),
            Preset::range("Spin CW", 208, 255),
        ]),
        Channel::new(Kind::Other, "Reserved").index(2),
        Channel::new(Kind::Other, "Reset / settings")
            .index(3)
            .presets(vec![
                Preset::range("No function", 0, 40),
                Preset::range("Settings (PWM, dimmer curve)", 41, 130),
                Preset::range("Reset pan/tilt", 131, 140),
                Preset::range("Reset effects", 141, 150),
                Preset::range("Reset all", 151, 160),
                Preset::range("No function ", 161, 255),
            ]),
    ];
    library(
        &profile_uid("Betopper LB150 12CH"),
        "Betopper/150W LED Beam 12CH.ssl2",
        &body,
        &ch,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::channels;

    #[test]
    fn built_profiles_have_their_manual_footprints() {
        assert_eq!(channels(&solena_max_bar_24ch()).len(), 24);
        let rock = channels(&rockstrip_252_28ch());
        assert_eq!(rock.len(), 28);
        assert_eq!(rock.get(23).map(|c| (c.kind, c.beams.clone())), Some((Kind::Blue, vec![7])));
        assert_eq!(rock.get(24).map(|c| c.kind), Some(Kind::Dimmer));
        let mover = channels(&betopper_150w_beam_12ch());
        assert_eq!(mover.len(), 12);
        assert_eq!(mover.first().map(|c| c.kind), Some(Kind::Pan));
        assert_eq!(mover.get(1).map(|c| c.kind), Some(Kind::PanFine));
    }

    #[test]
    fn the_mini_gobo_is_the_desks_eleven_channels() {
        let lib = mini_gobo_moving_head_11ch().expect("bundled profile parses");
        assert_eq!(
            lib.get("SSLNAME"),
            Some("_varied/Mini Gobo Moving Head Light.ssl2")
        );
        let kinds: Vec<Kind> = channels(&lib).iter().map(|c| c.kind).collect();
        assert_eq!(kinds.len(), 11);
        assert_eq!(kinds.first(), Some(&Kind::Pan));
        assert_eq!(kinds.get(2), Some(&Kind::Tilt));
        assert_eq!(kinds.get(6), Some(&Kind::Strobe));
        assert_eq!(kinds.get(7), Some(&Kind::Dimmer));
    }

    #[test]
    fn uids_are_stable_and_desk_shaped() {
        let a = profile_uid("x");
        assert_eq!(a, profile_uid("x"));
        assert_eq!(a.len(), 36 + 1 + 8);
    }
}

//! The desk's own effects: Color FX and Move FX racks, which a scene
//! carries beside (or instead of) its steps rack and which the desk runs
//! live, so their speed and size follow its faders.
//!
//! ```text
//! RACK EXPAND_RACK="1" TYPE="2"                 2 Color FX, 4 Move FX
//!   EFFECT TYPE="2" ID="130" DURATION="5000"    ID: which effect
//!     PARAMS NB=
//!       PARAM TYPE="4" ID="1" > COLORS > COLOR VAL="r/g/b/…"   the palette
//!       PARAM TYPE="5" ID="1" > POINTS > POINT X= Y=           a Move FX shape
//!       PARAM TYPE= ID= VAL=                    the effect's own settings
//!   BEAMS NB=
//!     BEAM FIXTURE=uid BEAMID=cell IDSELECTION=1..n          in effect order
//! ```
//!
//! Every parameter set here is copied from scenes programmed on a real desk
//! (the Gearbox showfile): which parameter is which is only partly known,
//! so they are written as the desk wrote them, not modelled.

use crate::xml::Element;

/// One cell an effect runs over: a fixture's `DASUID` and its `SSLBEAM`
/// index (0 for a single-cell fixture).
pub type Beam = (String, u16);

/// The palette the desk's rainbow-type effects ship with: white, red,
/// orange, yellow, green, cyan, blue, violet, magenta.
const WHEEL: [&str; 9] = [
    "1/1/1",
    "1/0/0",
    "1/0.498039/0",
    "1/1/0",
    "0/1/0",
    "0/1/1",
    "0/0/1",
    "0.545098/0/1",
    "1/0/1",
];

/// A Color FX the desk ships, with the settings a real show used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorEffect {
    /// 130: the palette rolled across the cells, fading between colours.
    Rainbow,
    /// 127: one colour swept back and forth over a dark field.
    KnightRider,
    /// 121: random cells flashing palette colours.
    Sparkle,
    /// 128: Perlin noise through the palette.
    Perlin,
}

impl ColorEffect {
    const fn id(self) -> u16 {
        match self {
            Self::Sparkle => 121,
            Self::KnightRider => 127,
            Self::Perlin => 128,
            Self::Rainbow => 130,
        }
    }

    /// `(palette, (TYPE, ID, VAL) of every other parameter)`, verbatim.
    fn params(self) -> (Vec<&'static str>, Vec<(u8, u8, &'static str)>) {
        // IDs 2 and 3 are on every colour effect, always 0.
        let common = [(2, 2, "0"), (6, 3, "0")];
        let (palette, own): (Vec<&str>, Vec<(u8, u8, &str)>) = match self {
            // '7 Main | Rainbow '
            Self::Rainbow => (
                WHEEL.to_vec(),
                vec![(1, 10, "0.028"), (0, 11, "0"), (1, 12, "1")],
            ),
            // '8 WALL Knight Rider': black and red.
            Self::KnightRider => (
                vec!["0/0/0", "1/0/0"],
                vec![
                    (0, 10, "9"),
                    (2, 11, "1"),
                    (2, 12, "1"),
                    (2, 13, "0"),
                    (0, 14, "50"),
                ],
            ),
            // '7 Sparkle': the wheel and a deep red.
            Self::Sparkle => (
                WHEEL.iter().copied().chain(["0.937499/0/0"]).collect(),
                vec![(0, 10, "10"), (1, 11, "1")],
            ),
            // '8 Perlin'
            Self::Perlin => (
                WHEEL.to_vec(),
                vec![
                    (0, 10, "4"),
                    (0, 11, "75"),
                    (0, 12, "2"),
                    (0, 13, "1"),
                    (0, 14, "48"),
                ],
            ),
        };
        (palette, common.into_iter().chain(own).collect())
    }
}

/// A palette entry: `r/g/b` (0..1) and the fifteen fields the desk writes
/// after it on every colour read.
fn color(rgb: &str) -> Element {
    Element::new("COLOR").with("VAL", format!("{rgb}/0/0/0/0/0/1/1/1/1/0/0/0/0/0/1"))
}

fn beams(cells: &[Beam]) -> Element {
    let mut el = Element::new("BEAMS").with("NB", cells.len());
    for (n, (fixture, beam)) in (1_usize..).zip(cells) {
        el.push(
            Element::new("BEAM")
                .with("FIXTURE", fixture)
                .with("BEAMID", beam)
                .with("IDSELECTION", n),
        );
    }
    el
}

/// A Color FX rack running `effect` over `cells`, in that order.
#[must_use]
pub fn color_fx(effect: ColorEffect, cells: &[Beam]) -> Element {
    let (palette, rest) = effect.params();
    let mut colors = Element::new("COLORS").with("NB", palette.len());
    for c in palette {
        colors.push(color(c));
    }
    let mut params = Element::new("PARAMS")
        .with("NB", rest.len().saturating_add(1))
        .child(
            Element::new("PARAM")
                .with("TYPE", 4)
                .with("ID", 1)
                .child(colors),
        );
    for (kind, id, val) in rest {
        params.push(
            Element::new("PARAM")
                .with("TYPE", kind)
                .with("ID", id)
                .with("VAL", val),
        );
    }
    rack(2, effect.id(), params, cells)
}

/// A Move FX the desk ships. Which shape each ID draws is not confirmed;
/// the points are the shape's control points on the pan/tilt grid, 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveEffect {
    /// 224, as 'Movers Slow NEW' set it.
    Slow,
    /// 221, as 'Movers Fast NEW' set it.
    Fast,
}

/// A Move FX rack over `cells` (movers, BEAMID 0).
///
/// `PARAM ID 2` (`TYPE 1`) reads as the speed (0.085 on the slow scene,
/// 0.114 on the fast one); `ID 3` (`TYPE 2`, 0 on both) is probably the
/// absolute/relative switch. Both unconfirmed.
#[must_use]
pub fn move_fx(effect: MoveEffect, cells: &[Beam]) -> Element {
    let (id, points, speed): (u16, [(&str, &str); 4], &str) = match effect {
        MoveEffect::Slow => (
            224,
            [
                ("0.481856", "0.882456"),
                ("0.549756", "0.581234"),
                ("0.0528198", "0.614023"),
                ("0.0554559", "0.938989"),
            ],
            "0.085",
        ),
        MoveEffect::Fast => (
            221,
            [
                ("0.256638", "0.409606"),
                ("0.542327", "0.401597"),
                ("0.826719", "0.411341"),
                ("0.569757", "0.660055"),
            ],
            "0.114",
        ),
    };
    let mut list = Element::new("POINTS").with("NB", points.len());
    for (x, y) in points {
        list.push(Element::new("POINT").with("X", x).with("Y", y));
    }
    let params = Element::new("PARAMS")
        .with("NB", 3)
        .child(
            Element::new("PARAM")
                .with("TYPE", 5)
                .with("ID", 1)
                .child(list),
        )
        .child(
            Element::new("PARAM")
                .with("TYPE", 1)
                .with("ID", 2)
                .with("VAL", speed),
        )
        .child(
            Element::new("PARAM")
                .with("TYPE", 2)
                .with("ID", 3)
                .with("VAL", 0),
        );
    rack(4, id, params, cells)
}

fn rack(kind: u8, id: u16, params: Element, cells: &[Beam]) -> Element {
    Element::new("RACK")
        .with("EXPAND_RACK", 1)
        .with("TYPE", kind)
        .child(
            Element::new("EFFECT")
                .with("TYPE", kind)
                .with("ID", id)
                .with("DURATION", 5000)
                .child(params),
        )
        .child(beams(cells))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rainbow_is_written_as_the_desk_wrote_it() {
        let rack = color_fx(ColorEffect::Rainbow, &[("a".into(), 0), ("a".into(), 1)]);
        let xml = rack.to_xml();
        assert!(xml.contains(r#"<EFFECT TYPE="2" ID="130" DURATION="5000">"#));
        assert!(xml.contains(r#"<PARAMS NB="6">"#));
        assert!(xml.contains(r#"VAL="1/0.498039/0/0/0/0/0/0/1/1/1/1/0/0/0/0/0/1""#));
        assert!(xml.contains(r#"<BEAM FIXTURE="a" BEAMID="1" IDSELECTION="2""#));
    }

    #[test]
    fn a_move_has_its_points_and_speed() {
        let xml = move_fx(MoveEffect::Fast, &[("m".into(), 0)]).to_xml();
        assert!(xml.contains(r#"<RACK EXPAND_RACK="1" TYPE="4">"#));
        assert!(xml.contains(r#"<POINT X="0.826719" Y="0.411341""#));
        assert!(xml.contains(r#"VAL="0.114""#));
    }
}

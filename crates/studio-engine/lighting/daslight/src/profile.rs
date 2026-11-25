//! Fixture profiles as Daslight embeds them (`SSLLIBRARY`).
//!
//! A showfile carries a full copy of every profile it patches, so a profile
//! written here only has to be valid in the file, not in the laptop's
//! library. The type codes and flag bits come from real showfiles
//! (README.md, "What the codes mean").

use crate::xml::Element;

/// What a channel does, as far as Daslight's own tools need to know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pan,
    PanFine,
    Tilt,
    TiltFine,
    Dimmer,
    Strobe,
    Red,
    Green,
    Blue,
    White,
    WarmWhite,
    Macro,
    Speed,
    Gobo,
    /// Anything else: colour wheels, prisms, modes, resets.
    Other,
}

impl Kind {
    /// `SSLCHANNELTYPE`.
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Pan | Self::PanFine => 1,
            Self::Tilt | Self::TiltFine => 2,
            Self::Dimmer => 7,
            Self::Gobo => 8,
            Self::Strobe => 15,
            Self::Speed => 18,
            Self::Red => 25,
            Self::Green => 26,
            Self::Blue => 27,
            Self::White => 31,
            Self::Macro => 37,
            Self::WarmWhite => 43,
            Self::Other => 0,
        }
    }

    /// The kind a `SSLCHANNELTYPE` code means.
    #[must_use]
    pub const fn from_code(code: u16, fine: bool) -> Self {
        match (code, fine) {
            (1, false) => Self::Pan,
            (1, true) => Self::PanFine,
            (2, false) => Self::Tilt,
            (2, true) => Self::TiltFine,
            (7, _) => Self::Dimmer,
            (8, _) => Self::Gobo,
            (15, _) => Self::Strobe,
            (18, _) => Self::Speed,
            (25, _) => Self::Red,
            (26, _) => Self::Green,
            (27, _) => Self::Blue,
            (31, _) => Self::White,
            (37, _) => Self::Macro,
            (43, _) => Self::WarmWhite,
            _ => Self::Other,
        }
    }

    /// The per-channel byte in a fixture's `FLAG`: `0xc0` for the dimmer
    /// (highest takes precedence, and it fades), `0x80` for channels that fade
    /// (colour, white, position), `0` for channels that snap (strobe, macros).
    #[must_use]
    pub const fn flag(self) -> u8 {
        match self {
            Self::Dimmer => 0xc0,
            Self::Red
            | Self::Green
            | Self::Blue
            | Self::White
            | Self::WarmWhite
            | Self::Pan
            | Self::PanFine
            | Self::Tilt
            | Self::TiltFine => 0x80,
            Self::Strobe | Self::Macro | Self::Speed | Self::Gobo | Self::Other => 0,
        }
    }

    const fn coarse(self) -> bool {
        matches!(self, Self::Pan | Self::Tilt)
    }

    const fn fine(self) -> bool {
        matches!(self, Self::PanFine | Self::TiltFine)
    }

    /// The default preset for a plain 0..255 channel of this kind:
    /// `(SSLPRESETTYPE, icon)`.
    const fn default_preset(self) -> (u16, &'static str) {
        match self {
            Self::Dimmer => (4, "ScanLibrary/icon/Presets/Dimmer/Dimmer"),
            Self::Red => (65, "ScanLibrary/icon/Presets/Color/Red"),
            Self::Green => (66, "ScanLibrary/icon/Presets/Color/Green"),
            Self::Blue => (67, "ScanLibrary/icon/Presets/Color/Blue"),
            Self::WarmWhite => (79, "ScanLibrary/icon/Presets/Color/warmwhite"),
            Self::Speed => (28, "ScanLibrary/icon/Presets/Speed/Speed"),
            _ => (0, "ScanLibrary/icon/Presets/Undef/NoFunction"),
        }
    }
}

/// A named DMX range on a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    pub start: u8,
    pub end: u8,
    pub default: u8,
    /// `SSLPRESETTYPE`; 0 is a plain function.
    pub kind: u16,
}

impl Preset {
    #[must_use]
    pub fn range(name: &str, start: u8, end: u8) -> Self {
        Self {
            name: name.to_owned(),
            start,
            end,
            default: start,
            kind: 0,
        }
    }
}

/// One DMX channel of a mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub kind: Kind,
    pub name: String,
    /// Tells apart several channels of one type (`Red` of cell 1 and 2).
    pub type_index: u16,
    /// The cells (`SSLBEAM` indexes) this channel drives; empty for all.
    pub beams: Vec<u16>,
    /// Empty means one full-range preset of the kind's default.
    pub presets: Vec<Preset>,
}

impl Channel {
    #[must_use]
    pub fn new(kind: Kind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            type_index: 0,
            beams: Vec::new(),
            presets: Vec::new(),
        }
    }

    #[must_use]
    pub const fn index(mut self, type_index: u16) -> Self {
        self.type_index = type_index;
        self
    }

    #[must_use]
    pub fn beam(mut self, beam: u16) -> Self {
        self.beams = vec![beam];
        self
    }

    #[must_use]
    pub fn presets(mut self, presets: Vec<Preset>) -> Self {
        self.presets = presets;
        self
    }

    /// The `SSLCHANNEL` element.
    #[must_use]
    pub fn to_element(&self) -> Element {
        let mut ch = Element::new("SSLCHANNEL")
            .with("SSLCHANNELTYPE", self.kind.code())
            .with("SSLCHANNELNAME", &self.name)
            .with("SSLCHANNELTYPEINDEX", self.type_index)
            .with("SSLCHANNELICON", "");
        if self.kind == Kind::WarmWhite {
            // The swatch the desk draws for the channel at full.
            ch.set("SSLCHANNELCOLORMIN", 0);
            ch.set("SSLCHANNELCOLORMAX", 11_730_942);
        }
        ch.set("SSLCHANNELMSB", u8::from(self.kind.coarse()));
        ch.set("SSLCHANNELLSB", u8::from(self.kind.fine()));
        ch.set("SSLCHANNEL2PRESETS", 0);
        if !self.beams.is_empty() {
            let mut beams = Element::new("SSLBEAMS").with("SSLNBBEAM", self.beams.len());
            for b in &self.beams {
                beams.push(Element::new("SSLBEAM").with("SSLBEAMINDEX", b));
            }
            ch.push(beams);
        }
        let presets = if self.presets.is_empty() {
            let (kind, _) = self.kind.default_preset();
            vec![Preset {
                name: self.name.clone(),
                start: 0,
                end: 255,
                default: 255,
                kind,
            }]
        } else {
            self.presets.clone()
        };
        let mut list = Element::new("SSLPRESETS").with("SSLNBPRESET", presets.len());
        for (i, p) in presets.iter().enumerate() {
            let icon = if p.kind == self.kind.default_preset().0 {
                self.kind.default_preset().1
            } else {
                "ScanLibrary/icon/Presets/Undef/macro"
            };
            list.push(
                Element::new("SSLPRESET")
                    .with("SSLPRESETTYPE", p.kind)
                    .with("SSLPRESETNAME", &p.name)
                    .with("SSLPRESETICON", icon)
                    .with("SSLPRESETDMXSTART", p.start)
                    .with("SSLPRESETDMXEND", p.end)
                    .with("SSLPRESETDMXDEFAULT", p.default)
                    .with(
                        "SSLPRESETSHOWDIMMER",
                        u8::from(matches!(
                            self.kind,
                            Kind::Dimmer
                                | Kind::Red
                                | Kind::Green
                                | Kind::Blue
                                | Kind::WarmWhite
                                | Kind::White
                        )),
                    )
                    .with("SSLPRESETID", "")
                    .with("SSLPRESETDEFAULTPRESET", u8::from(i == 0)),
            );
        }
        ch.push(list);
        ch
    }

    /// Reads an `SSLCHANNEL` element back (presets are not kept).
    #[must_use]
    pub fn from_element(el: &Element) -> Self {
        let num = |k: &str| el.get(k).and_then(|v| v.parse::<u16>().ok()).unwrap_or(0);
        let fine = el.get("SSLCHANNELLSB") == Some("1");
        let beams = el
            .find("SSLBEAMS")
            .map(|b| {
                b.elements()
                    .filter_map(|e| e.get("SSLBEAMINDEX")?.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            kind: Kind::from_code(num("SSLCHANNELTYPE"), fine),
            name: el.get("SSLCHANNELNAME").unwrap_or_default().to_owned(),
            type_index: num("SSLCHANNELTYPEINDEX"),
            beams,
            presets: Vec::new(),
        }
    }
}

/// The physical description in `SSLPROPERTIES`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    /// `SSLFIXTTYPE`: 2 a par or bar, 3 a multi-cell LED fixture. A moving
    /// head's code is not confirmed from a real file; 0 is used.
    pub fixture_type: u8,
    pub size_cm: (u16, u16, u16),
    pub beam_deg: u16,
    /// Pan and tilt range in degrees; only read for moving heads.
    pub pan_tilt_deg: (u16, u16),
    pub manufacturer: String,
    pub model: String,
    pub icon: String,
    pub object_3d: String,
    /// Cell positions in the desk's 0..100-per-cell grid.
    pub cells: Vec<(i32, i32)>,
}

impl Body {
    fn to_element(&self) -> Element {
        let mut p = Element::new("SSLPROPERTIES")
            .with("SSLFIXTFAMILY", 0)
            .with("SSLFIXTTYPE", self.fixture_type)
            .with("SSLSIZEX", self.size_cm.0)
            .with("SSLSIZEY", self.size_cm.1)
            .with("SSLSIZEZ", self.size_cm.2)
            .with("SSLWEIGHT", 0)
            .with("SSLBEAMOPENING", self.beam_deg)
            .with("SSLBEAMOPENING2", 60)
            .with("SSLAMPLIPAN", self.pan_tilt_deg.0)
            .with("SSLAMPLITILT", self.pan_tilt_deg.1)
            .with("SSLTIMEPAN", 1000)
            .with("SSLTIMETILT", 1000)
            .with("SSLPANCENTER", 127)
            .with("SSLTILTCENTER", 127)
            .with("SSLLAMPPOWER", 0)
            .with("SSLLAMPLUX", -1)
            .with("SSLLAMPLIFE", -1)
            .with("SSLLAMPTEMP", 7000)
            .with("SSLLAMPTYPE", "LED")
            .with("SSLLAMPREF", "")
            .with("SSLBEAMTYPE", 0)
            .with("SSLICON", &self.icon)
            .with("SSL3DOBJECTFILE", "Heads/Head.x1")
            .with("SSL3DOBJECTFILE2", &self.object_3d)
            .with("SSLCOLOR", 255)
            .with("SSLRDMMANUFACTURERNAME", &self.manufacturer)
            .with("SSLRDMMANUFACTURERID", 0)
            .with("SSLRDMFIXTURENAME", &self.model)
            .with("SSLRDMFIXTUREID", 0)
            .with("SSLTYPEDRAW", -1)
            .with("SSLINVPAN", 0)
            .with("SSLINVTILT", 0)
            .with("SSLCREATOR", "ignition-daslight");
        if !self.cells.is_empty() {
            let mut beams = Element::new("SSLBEAMS").with("SSLNBBEAM", self.cells.len());
            for (x, y) in &self.cells {
                beams.push(
                    Element::new("SSLBEAM")
                        .with("SSLBEAMPOSX", x)
                        .with("SSLBEAMPOSY", y),
                );
            }
            p.push(beams);
        }
        p
    }
}

/// A whole profile with one mode: the `SSLLIBRARY` element.
#[must_use]
pub fn library(uid: &str, library_name: &str, body: &Body, channels: &[Channel]) -> Element {
    let mut mode = Element::new("SSLMODE")
        .with("SSLMODEINDEX", 0)
        .with("SSLNBCHANNEL", channels.len());
    for c in channels {
        mode.push(c.to_element());
    }
    Element::new("SSLLIBRARY")
        .with("SSLFIXUID", uid)
        .with("SSLNAME", library_name)
        .child(body.to_element())
        .child(Element::new("SSLMODES").with("SSLNBMODE", 1).child(mode))
}

/// The channels of a profile's first mode, in DMX order.
#[must_use]
pub fn channels(library: &Element) -> Vec<Channel> {
    library
        .find("SSLMODES")
        .and_then(|m| m.find("SSLMODE"))
        .map(|m| {
            m.elements()
                .filter(|e| e.name == "SSLCHANNEL")
                .map(Channel::from_element)
                .collect()
        })
        .unwrap_or_default()
}

/// The cell grid of a profile (`SSLPROPERTIES > SSLBEAMS`), empty for a
/// single-cell fixture.
#[must_use]
pub fn cells(library: &Element) -> Vec<(i32, i32)> {
    library
        .find("SSLPROPERTIES")
        .and_then(|p| p.find("SSLBEAMS"))
        .map(|b| {
            b.elements()
                .filter_map(|e| {
                    Some((
                        e.get("SSLBEAMPOSX")?.parse().ok()?,
                        e.get("SSLBEAMPOSY")?.parse().ok()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Appends channels to a profile's first mode and fixes its channel count.
pub fn append_channels(library: &mut Element, extra: &[Channel]) {
    if let Some(mode) = library
        .find_mut("SSLMODES")
        .and_then(|m| m.find_mut("SSLMODE"))
    {
        for c in extra {
            mode.push(c.to_element());
        }
        let n = mode.elements().filter(|e| e.name == "SSLCHANNEL").count();
        mode.set("SSLNBCHANNEL", n);
    }
}

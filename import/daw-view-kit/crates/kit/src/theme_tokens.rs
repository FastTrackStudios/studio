//! The theme tokens a view draws with: what each view crate says it reads
//! of a theme, so a theme pack knows what to style.
//!
//! A view reads its look two ways, both through the loaded theme
//! ([`crate::skin`]):
//!
//! - the [`Palette`](crate::palette::Palette), whose fields a theme
//!   recolours by name ([`crate::skin::palette`]): the kit's own tokens,
//!   [`TOKENS`];
//! - a view's own named colours ([`ThemeToken::color`]): the theme's
//!   value for the token when it names one, the token's default when not.
//!
//! Each view crate lists every token it uses as a `THEME_TOKENS` slice of
//! [`ThemeToken`]s (its palette tokens by name, its own with their
//! defaults), so the theme contract can be checked against what the views
//! really draw.

use frame::theme::runtime::Value;
use vello::peniko::Color;

/// One colour a view draws with, as a theme names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeToken {
    /// The token's name in a theme (`text.dim`, `arrangement.drum.kick`).
    pub name: &'static str,
    /// Its colour when no theme names it, `#rrggbb` or `#rrggbbaa`.
    pub default: &'static str,
    /// What it paints, for the person writing a theme.
    pub what: &'static str,
}

impl ThemeToken {
    /// The colour to draw: the loaded theme's for this token, or the
    /// default.
    #[must_use]
    pub fn color(&self) -> Color {
        themed(self.name).unwrap_or_else(|| css(self.default))
    }
}

/// A colour as a theme writes it (`#rrggbb`, `#rrggbbaa`); transparent
/// when it is not one.
#[must_use]
pub fn css(text: &str) -> Color {
    vello::peniko::color::parse_color(text).map_or(Color::TRANSPARENT, |c| c.to_alpha_color())
}

/// The loaded theme's colour for `name`, if it names one.
fn themed(name: &str) -> Option<Color> {
    let theme = crate::skin::loaded()?;
    let [r, g, b, a] = theme.token(name).and_then(Value::color)?.to_rgba8();
    Some(Color::from_rgba8(r, g, b, a))
}

/// The kit's tokens: the [`Palette`](crate::palette::Palette)'s fields a
/// theme recolours by name ([`crate::skin::palette`]), with the base
/// theme's values as defaults (`daw_ui::theming::Theme::dark`, the theme
/// a palette is made from when none is loaded).
pub const TOKENS: &[ThemeToken] = &[
    ThemeToken {
        name: "text",
        default: "#e4e4e7",
        what: "text, and the chrome's",
    },
    ThemeToken {
        name: "text.dim",
        default: "#a1a1aa",
        what: "secondary text",
    },
    ThemeToken {
        name: "text.faint",
        default: "#71717a",
        what: "tertiary text, disabled labels",
    },
    ThemeToken {
        name: "accent",
        default: "#38bdf8",
        what: "selection, focus, the active control",
    },
    ThemeToken {
        name: "mute",
        default: "#ef4444",
        what: "a muted track's mute button",
    },
    ThemeToken {
        name: "solo",
        default: "#eab308",
        what: "a soloed track's solo button",
    },
    ThemeToken {
        name: "rec",
        default: "#ef4444",
        what: "record arm",
    },
    ThemeToken {
        name: "pan",
        default: "#eab308",
        what: "the pan control's value",
    },
    ThemeToken {
        name: "meter.safe",
        default: "#22c55e",
        what: "a meter below the warning level",
    },
    ThemeToken {
        name: "meter.warn",
        default: "#f59e0b",
        what: "a meter near full scale",
    },
    ThemeToken {
        name: "meter.danger",
        default: "#ef4444",
        what: "a meter at or over full scale",
    },
    ThemeToken {
        name: "field",
        default: "#0a0a0c",
        what: "a track panel's text fields",
    },
    ThemeToken {
        name: "combo",
        default: "#0a0a0c",
        what: "a track panel's drop-downs",
    },
    ThemeToken {
        name: "rack.well",
        default: "#0a0a0c",
        what: "the well a panel's meter sits in",
    },
    ThemeToken {
        name: "rail.ground",
        default: "#09090b",
        what: "the track panel's gutter",
    },
    ThemeToken {
        name: "rail.rule",
        default: "#27272a",
        what: "rules between panel rows",
    },
    ThemeToken {
        name: "rail.button",
        default: "#09090b",
        what: "a track panel's buttons",
    },
    ThemeToken {
        name: "hardware",
        default: "#18181b",
        what: "the chrome's raised hardware (control faces)",
    },
    ThemeToken {
        name: "hardware.edge",
        default: "#27272a",
        what: "a control face's edge",
    },
    ThemeToken {
        name: "hardware.mark",
        default: "#c2c2c8",
        what: "the ink printed on a control face (text.dim and text mixed)",
    },
    ThemeToken {
        name: "surface",
        default: "#09090b",
        what: "the chrome's ground",
    },
    ThemeToken {
        name: "surface.sunken",
        default: "#0a0a0c",
        what: "a sunken surface (lanes, wells)",
    },
    ThemeToken {
        name: "border",
        default: "#27272a",
        what: "borders between surfaces",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_is_a_colour() {
        for token in TOKENS {
            let d = token.default.trim_start_matches('#');
            assert!(
                token.default.starts_with('#')
                    && (d.len() == 6 || d.len() == 8)
                    && d.chars().all(|c| c.is_ascii_hexdigit()),
                "{}: {}",
                token.name,
                token.default
            );
        }
    }

    #[test]
    fn the_palette_tokens_default_to_the_base_palette() {
        use crate::palette::Palette;
        let p = Palette::from_theme(&daw_ui::theming::Theme::default());
        let field = |name: &str| match name {
            "text" => Some(p.text),
            "text.dim" => Some(p.text_dim),
            "text.faint" => Some(p.text_faint),
            "accent" => Some(p.accent),
            "mute" => Some(p.mute),
            "solo" => Some(p.solo),
            "rec" => Some(p.rec),
            "pan" => Some(p.pan),
            "meter.safe" => Some(p.meter_safe),
            "meter.warn" => Some(p.meter_warn),
            "meter.danger" => Some(p.meter_danger),
            "field" => Some(p.tcp_field),
            "combo" => Some(p.tcp_combo),
            "rack.well" => Some(p.tcp_meter_well),
            "rail.ground" => Some(p.tcp_gutter),
            "rail.rule" => Some(p.tcp_rule),
            "rail.button" => Some(p.tcp_button),
            _ => None,
        };
        for token in TOKENS {
            if let Some(colour) = field(token.name) {
                assert_eq!(colour, css(token.default), "{}", token.name);
            }
        }
    }

    #[test]
    fn with_no_theme_a_token_is_its_default() {
        let token = ThemeToken {
            name: "nobody.names.this",
            default: "#102030",
            what: "a test",
        };
        assert_eq!(token.color(), Color::from_rgba8(0x10, 0x20, 0x30, 0xff));
    }
}

//! Parameters: what a block or the instance exposes, in plain units.
//!
//! A value is always the plain value (dB, Hz, ms, a 0–1 amount, a choice
//! index), never normalised; [`ParamSpec::normalize`] maps it for a host.
//! A parameter parses the string it prints ([`ParamSpec::format`] →
//! [`ParamSpec::parse`]), so a host's text entry and a preset's text agree.

use core::fmt::{self, Write};

/// What a parameter's numbers mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// A bare number with this many decimals.
    Plain(u8),
    Db,
    Hz,
    Ms,
    /// 0–1, shown as 0–100 %.
    Percent,
    /// One of these names; the value is the index.
    Choice(&'static [&'static str]),
    /// Off (0) or on (1).
    Toggle,
}

/// One parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    /// Stable id, the key a preset stores the value under.
    pub id: &'static str,
    pub name: &'static str,
    pub unit: Unit,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ParamSpec {
    #[must_use]
    pub const fn new(
        id: &'static str,
        name: &'static str,
        unit: Unit,
        min: f32,
        max: f32,
        default: f32,
    ) -> Self {
        Self {
            id,
            name,
            unit,
            min,
            max,
            default,
        }
    }

    /// A choice among `names`, defaulting to the first.
    #[must_use]
    pub const fn choice(
        id: &'static str,
        name: &'static str,
        names: &'static [&'static str],
    ) -> Self {
        let max = dsp_core::num::count_to_f32(names.len().saturating_sub(1));
        Self::new(id, name, Unit::Choice(names), 0.0, max, 0.0)
    }

    /// `value` inside the range (and on a step for choices and toggles).
    #[must_use]
    pub fn clamp(&self, value: f32) -> f32 {
        let v = if value.is_nan() {
            self.default
        } else {
            value.clamp(self.min, self.max)
        };
        match self.unit {
            Unit::Choice(_) | Unit::Toggle => libm::roundf(v),
            _ => v,
        }
    }

    /// The value as 0–1 along the range.
    #[must_use]
    pub fn normalize(&self, value: f32) -> f32 {
        let span = self.max - self.min;
        if span <= 0.0 {
            0.0
        } else {
            ((self.clamp(value) - self.min) / span).clamp(0.0, 1.0)
        }
    }

    /// The plain value at `normalized` (0–1) along the range.
    #[must_use]
    pub fn denormalize(&self, normalized: f32) -> f32 {
        self.clamp(normalized.clamp(0.0, 1.0) * (self.max - self.min) + self.min)
    }

    /// The value a parameter runs at under modulation: `base` (plain)
    /// offset by `modulation` in normalized units, clamped to the range.
    /// This is `CLAP_EVENT_PARAM_MOD`'s meaning in nice-plug, so a block
    /// modulated in Signal and its CLAP form modulated by Signal's host run
    /// at the same value.
    #[must_use]
    pub fn modulated(&self, base: f32, modulation: f32) -> f32 {
        if modulation == 0.0 {
            return self.clamp(base);
        }
        self.denormalize(self.normalize(base) + modulation)
    }

    /// Print `value` the way [`parse`](Self::parse) reads it back.
    ///
    /// # Errors
    ///
    /// Whatever `out` returns.
    pub fn format(&self, value: f32, out: &mut dyn Write) -> fmt::Result {
        let v = self.clamp(value);
        match self.unit {
            Unit::Plain(decimals) => write!(out, "{v:.prec$}", prec = usize::from(decimals)),
            Unit::Db => write!(out, "{v:.1} dB"),
            Unit::Hz => {
                // Decide on the printed (rounded) value, so 999.96 is
                // "1.00 kHz" both ways.
                if libm::roundf(v * 10.0) >= 10_000.0 {
                    write!(out, "{:.2} kHz", v / 1000.0)
                } else {
                    write!(out, "{v:.1} Hz")
                }
            }
            Unit::Ms => write!(out, "{v:.1} ms"),
            Unit::Percent => write!(out, "{:.1} %", v * 100.0),
            Unit::Choice(names) => {
                let i = dsp_core::num::f32_to_index(v);
                out.write_str(names.get(i).copied().unwrap_or("?"))
            }
            Unit::Toggle => out.write_str(if v >= 0.5 { "On" } else { "Off" }),
        }
    }

    /// Read a value from text: what [`format`](Self::format) prints, with
    /// or without the unit. `None` if it isn't one.
    #[must_use]
    pub fn parse(&self, text: &str) -> Option<f32> {
        let t = text.trim();
        let value = match self.unit {
            Unit::Choice(names) => names
                .iter()
                .position(|n| n.eq_ignore_ascii_case(t))
                .map(dsp_core::num::count_to_f32)
                .or_else(|| t.parse::<f32>().ok())?,
            Unit::Toggle => match t.to_ascii_lowercase().as_str() {
                "on" | "1" | "true" => 1.0,
                "off" | "0" | "false" => 0.0,
                _ => return None,
            },
            Unit::Hz => {
                let lower = t.to_ascii_lowercase();
                if let Some(k) = lower.strip_suffix("khz") {
                    k.trim().parse::<f32>().ok()? * 1000.0
                } else {
                    lower.trim_end_matches("hz").trim().parse::<f32>().ok()?
                }
            }
            Unit::Percent => t.trim_end_matches('%').trim().parse::<f32>().ok()? / 100.0,
            Unit::Db => strip_unit(t, "db").parse::<f32>().ok()?,
            Unit::Ms => strip_unit(t, "ms").parse::<f32>().ok()?,
            Unit::Plain(_) => t.parse::<f32>().ok()?,
        };
        value.is_finite().then(|| self.clamp(value))
    }
}

fn strip_unit<'a>(t: &'a str, unit: &str) -> &'a str {
    let n = t.len();
    let u = unit.len();
    if n >= u
        && t.get(n.saturating_sub(u)..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(unit))
    {
        t.get(..n.saturating_sub(u)).unwrap_or(t).trim()
    } else {
        t
    }
}

/// A fixed-capacity string a value is formatted into without allocating.
#[derive(Default)]
pub struct ValueText {
    buf: [u8; 32],
    len: usize,
}

impl ValueText {
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.buf
            .get(..self.len)
            .and_then(|b| core::str::from_utf8(b).ok())
            .unwrap_or("")
    }

    /// `spec` printing `value`.
    #[must_use]
    pub fn of(spec: &ParamSpec, value: f32) -> Self {
        let mut t = Self::default();
        let _ = spec.format(value, &mut t);
        t
    }
}

impl Write for ValueText {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(fmt::Error)?;
        let dst = self.buf.get_mut(self.len..end).ok_or(fmt::Error)?;
        dst.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPECS: &[ParamSpec] = &[
        ParamSpec::new("gain", "Gain", Unit::Db, -60.0, 24.0, 0.0),
        ParamSpec::new("on", "On", Unit::Toggle, 0.0, 1.0, 1.0),
        ParamSpec::new("f", "Freq", Unit::Hz, 20.0, 20_000.0, 1000.0),
        ParamSpec::new("t", "Time", Unit::Ms, 1.0, 2000.0, 250.0),
        ParamSpec::new("m", "Mix", Unit::Percent, 0.0, 1.0, 0.5),
        ParamSpec::new("q", "Q", Unit::Plain(2), 0.1, 10.0, 0.71),
        ParamSpec::choice("mode", "Mode", &["Kick", "Snare", "Tom"]),
    ];

    #[test]
    fn every_param_parses_what_it_prints() {
        for spec in SPECS {
            for step in 0..=20u8 {
                let v = spec.denormalize(f32::from(step) / 20.0);
                let text = ValueText::of(spec, v);
                let back = spec.parse(text.as_str());
                let again = back.map(|b| ValueText::of(spec, b));
                assert_eq!(
                    again.as_ref().map(ValueText::as_str),
                    Some(text.as_str()),
                    "{}: {v} printed {:?}",
                    spec.id,
                    text.as_str()
                );
            }
        }
    }

    #[test]
    fn modulation_offsets_in_normalized_units_and_clamps() {
        let freq = ParamSpec::new("f", "Freq", Unit::Hz, 20.0, 20_020.0, 1000.0);
        assert!((freq.modulated(1000.0, 0.0) - 1000.0).abs() < 1e-3);
        // +0.25 of the range is +5000 Hz.
        assert!((freq.modulated(1000.0, 0.25) - 6000.0).abs() < 1e-2);
        assert!((freq.modulated(19_000.0, 0.5) - 20_020.0).abs() < 1e-3);
        assert!((freq.modulated(1000.0, -1.0) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn units_are_optional_when_parsing() {
        assert_eq!(SPECS[0].parse("-3"), Some(-3.0));
        assert_eq!(SPECS[0].parse("-3 dB"), Some(-3.0));
        assert_eq!(SPECS[2].parse("1.5 kHz"), Some(1500.0));
        assert_eq!(SPECS[6].parse("snare"), Some(1.0));
        assert_eq!(SPECS[1].parse("maybe"), None);
    }
}

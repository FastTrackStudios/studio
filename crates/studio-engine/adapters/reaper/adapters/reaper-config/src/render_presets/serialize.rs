//! Serialization logic for writing REAPER render preset files.

use super::types::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format a preset name for output: quote it when it contains spaces.
fn format_name(name: &str) -> String {
    if name.contains(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

/// Format a string token (filename / directory) for output.
///
/// - Empty string  → `""`
/// - Contains spaces → `"value"`
/// - Otherwise     → bare value
fn format_string_token(s: &str) -> String {
    if s.is_empty() || s.contains(' ') {
        format!("\"{}\"", s)
    } else {
        s.to_string()
    }
}

/// Format a floating-point number for output.
///
/// REAPER writes integers without a decimal point when the fractional part is
/// zero, and uses the shortest representation otherwise.
fn format_f64(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        // Use Rust's default Display, which gives a reasonable shortest form.
        format!("{v}")
    }
}

// ---------------------------------------------------------------------------
// Entry serializers
// ---------------------------------------------------------------------------

fn serialize_renderpreset(p: &RenderPreset, out: &mut String) {
    out.push_str(&format!(
        "<RENDERPRESET {} {} {} {} {} {} {} {}\r\n",
        format_name(&p.name),
        p.sample_rate,
        p.channels,
        p.offline_mode,
        p.use_project_sample_rate,
        p.resample_mode,
        p.various_checkboxes,
        p.various_checkboxes2,
    ));
    out.push_str(&format!("  {}\r\n", p.render_cfg));
    out.push_str(">\r\n");
}

fn serialize_renderpreset2(p: &RenderPreset2, out: &mut String) {
    out.push_str(&format!("<RENDERPRESET2 {}\r\n", format_name(&p.name)));
    out.push_str(&format!("  {}\r\n", p.render_cfg));
    out.push_str(">\r\n");
}

fn serialize_renderpreset_output(o: &RenderPresetOutput, out: &mut String) {
    out.push_str(&format!(
        "RENDERPRESET_OUTPUT {} {} {} {} {} {} {} {} {} {}\r\n",
        format_name(&o.name),
        o.bounds,
        format_f64(o.start),
        format_f64(o.end),
        o.source,
        o.unknown,
        format_string_token(&o.filename),
        o.tail,
        format_string_token(&o.directory),
        o.tail_ms,
    ));
}

fn serialize_renderpreset_ext(e: &RenderPresetExt, out: &mut String) {
    out.push_str(&format!(
        "RENDERPRESET_EXT {} {} {} {} {} {} {} {} {} {} {} {}\r\n",
        format_name(&e.name),
        e.normalize_mode,
        format_f64(e.normalize_target),
        format_f64(e.brickwall_target),
        format_f64(e.fadein_length),
        format_f64(e.fadeout_length),
        e.fadein_shape,
        e.fadeout_shape,
        format_f64(e.trim_lead),
        format_f64(e.trim_trail),
        format_f64(e.pad_start),
        format_f64(e.pad_end),
    ));
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl RenderPresetFile {
    /// Serialize this file back to the `reaper-render.ini` text format.
    ///
    /// Each line is terminated with `\r\n` (CR-LF) to match REAPER's format.
    pub fn serialize(&self) -> String {
        let mut output = String::new();
        for entry in &self.entries {
            match entry {
                RenderPresetEntry::Preset(p) => serialize_renderpreset(p, &mut output),
                RenderPresetEntry::Preset2(p) => serialize_renderpreset2(p, &mut output),
                RenderPresetEntry::Output(o) => serialize_renderpreset_output(o, &mut output),
                RenderPresetEntry::Ext(e) => serialize_renderpreset_ext(e, &mut output),
            }
        }
        output
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

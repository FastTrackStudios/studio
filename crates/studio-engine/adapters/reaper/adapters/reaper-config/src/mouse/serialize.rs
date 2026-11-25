//! Serializer: converts a [`MouseConfig`] back to the `reaper-mouse.ini` /
//! `.ReaperMouseMap` text format.

use super::types::{MouseConfig, MouseContext};

impl MouseConfig {
    /// Serialize this config to the INI text format.
    pub fn serialize(&self) -> String {
        let mut out = String::new();

        if let Some(ref entries) = self.has_imported {
            out.push_str("[hasimported]\r\n");
            for entry in entries {
                out.push_str(&format!("{}={}\r\n", entry.key, entry.value));
            }
            out.push_str("\r\n");
        }

        for ctx in &self.contexts {
            serialize_context(ctx, &mut out);
        }

        out
    }
}

fn serialize_context(ctx: &MouseContext, out: &mut String) {
    out.push_str(&format!("[{}]\r\n", ctx.kind.to_reaper_string()));

    for binding in &ctx.bindings {
        out.push_str(&format!(
            "mm_{}={}\r\n",
            binding.modifier.0,
            binding.action.to_ini_string(),
        ));
    }

    if let Some(ref name) = ctx.name {
        out.push_str(&format!("name={name}\r\n"));
    }

    out.push_str("\r\n");
}

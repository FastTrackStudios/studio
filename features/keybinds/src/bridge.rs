//! Studio's keybind files ([`crate::proto`]) as an architect-input
//! [`KeymapConfig`].
//!
//! Converts:
//! - `KeybindDef` lists → `KeymapConfig` sequence bindings, a context
//!   (`Main`, `Midi`, `Custom(..)`) becoming a `keymap_context` layer
//! - `WhichKeyTreeDef` trees → flat key sequences
//!   ([`architect_input::which_key::bindings`])
//! - `WheelBindDef` lists → scroll bindings
//!
//! Key notation (`<C-s>`, `gg` → `Ctrl+s`, `g g`) is
//! [`architect_input::notation`]. REAPER's own coupling (action routing
//! marks, VK codes) stays in reaper-input.

use std::collections::HashMap;

use architect_input::{ContextLayerConfig, KeymapConfig, notation, which_key};

use crate::proto::{KeybindContext, KeybindDef, SectionConfig, WheelBindDef, WhichKeyTreeDef};

// ---------------------------------------------------------------------------
// Config → KeymapConfig
// ---------------------------------------------------------------------------

/// Convert a parsed [`SectionConfig`] (bindings + which-key trees + wheel
/// bindings) into a [`KeymapConfig`].
#[must_use]
pub fn section_to_keymap_config(section: &SectionConfig) -> KeymapConfig {
    keymap_config_from_defs(section.bindings(), section.which_key(), section.wheel())
}

/// Convert raw def slices into a [`KeymapConfig`].
///
/// All keybinds go into the "normal" mode keymap. Context-specific bindings
/// (`KeybindContext::Main`, `Midi`, `Custom(..)`, …) are placed in
/// `keymap_context` with `WhenExpr` conditions from [`context_to_when_expr`].
#[must_use]
pub fn keymap_config_from_defs(
    bindings: &[KeybindDef],
    trees: &[WhichKeyTreeDef],
    wheel: &[WheelBindDef],
) -> KeymapConfig {
    let mut global_bindings: HashMap<String, String> = HashMap::new();
    let mut context_bindings: HashMap<KeybindContext, HashMap<String, String>> = HashMap::new();

    for binding in bindings {
        let input_seq = notation::translate_sequence(&binding.keys);
        let ctx = binding.context.clone().unwrap_or(KeybindContext::Global);
        if ctx == KeybindContext::Global {
            global_bindings.insert(input_seq, binding.action.clone());
        } else {
            context_bindings
                .entry(ctx)
                .or_default()
                .insert(input_seq, binding.action.clone());
        }
    }

    // Which-key trees flatten into global sequence bindings.
    for (seq, action) in which_key::bindings(trees) {
        global_bindings.insert(seq, action);
    }

    let mut keymap: HashMap<String, HashMap<String, String>> = HashMap::new();
    keymap.insert("normal".to_string(), global_bindings);

    let mut keymap_context: HashMap<String, Vec<ContextLayerConfig>> = HashMap::new();
    for (ctx, bindings) in context_bindings {
        let when = context_to_when_expr(&ctx);
        keymap_context
            .entry("normal".to_string())
            .or_default()
            .push(ContextLayerConfig { when, bindings });
    }

    let scroll = convert_wheel_bindings(wheel);

    KeymapConfig {
        modes: HashMap::new(),
        keymap,
        keymap_context,
        mouse: HashMap::new(),
        scroll,
    }
}

// ---------------------------------------------------------------------------
// Context → WhenExpr string
// ---------------------------------------------------------------------------

/// Convert a [`KeybindContext`] to a when-expression string for
/// `keymap_context`. `Custom(name)` maps to `context:<name>`.
#[must_use]
pub fn context_to_when_expr(ctx: &KeybindContext) -> String {
    match ctx {
        KeybindContext::Global => "true".to_string(),
        KeybindContext::Main => "context:main".to_string(),
        KeybindContext::Midi => "context:midi".to_string(),
        KeybindContext::MidiInline => "context:midi_inline".to_string(),
        KeybindContext::MediaExplorer => "context:media_explorer".to_string(),
        KeybindContext::Custom(name) => format!("context:{name}"),
    }
}

// ---------------------------------------------------------------------------
// Wheel bindings → scroll config
// ---------------------------------------------------------------------------

/// Convert wheel bindings into scroll config entries (mode "normal").
fn convert_wheel_bindings(wheel: &[WheelBindDef]) -> HashMap<String, HashMap<String, String>> {
    if wheel.is_empty() {
        return HashMap::new();
    }

    let mut scroll_map: HashMap<String, String> = HashMap::new();
    for w in wheel {
        let axis = if w.horizontal.unwrap_or(false) {
            "ScrollX"
        } else {
            "Scroll"
        };
        let mods = notation::translate_modifiers(&w.modifiers);
        let pattern = if mods.is_empty() {
            axis.to_string()
        } else {
            format!("{mods}+{axis}")
        };
        scroll_map.insert(pattern, w.action.clone());
    }

    let mut result = HashMap::new();
    if !scroll_map.is_empty() {
        result.insert("normal".to_string(), scroll_map);
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::WhichKeyEntryDef;

    fn wheel(modifiers: &str, horizontal: Option<bool>, action: &str) -> WheelBindDef {
        WheelBindDef {
            modifiers: modifiers.into(),
            action: action.into(),
            horizontal,
            desc: None,
            context: None,
            mnemonic: None,
            why: None,
        }
    }

    #[test]
    fn wheel_bindings_become_normal_mode_scroll_patterns() {
        let config = keymap_config_from_defs(
            &[],
            &[],
            &[
                wheel("", None, "vscroll"),
                wheel("<S->", Some(true), "hscroll_shift"),
            ],
        );
        let scroll = config.scroll.get("normal").unwrap();
        assert_eq!(scroll.get("Scroll"), Some(&"vscroll".to_string()));
        assert_eq!(
            scroll.get("Shift+ScrollX"),
            Some(&"hscroll_shift".to_string())
        );
    }

    #[test]
    fn custom_context_when_expr() {
        assert_eq!(
            context_to_when_expr(&KeybindContext::Custom("editor".into())),
            "context:editor"
        );
    }

    #[test]
    fn defs_bridge_basic() {
        let bindings = vec![
            KeybindDef {
                keys: "a".into(),
                action: "action_a".into(),
                desc: None,
                context: None,
                passthrough: None,
                mnemonic: None,
                why: None,
            },
            KeybindDef {
                keys: "<C-s>".into(),
                action: "save".into(),
                desc: None,
                context: None,
                passthrough: None,
                mnemonic: None,
                why: None,
            },
        ];
        let config = keymap_config_from_defs(&bindings, &[], &[]);
        let normal = config.keymap.get("normal").unwrap();
        assert_eq!(normal.get("a"), Some(&"action_a".to_string()));
        assert_eq!(normal.get("Ctrl+s"), Some(&"save".to_string()));
    }

    #[test]
    fn which_key_tree_flattens() {
        let trees = vec![WhichKeyTreeDef {
            prefix: "v".into(),
            label: "Visibility".into(),
            case_insensitive: None,
            entries: vec![
                WhichKeyEntryDef {
                    key: "d".into(),
                    label: "Drums".into(),
                    action: Some("vis:drums".into()),
                    children: None,
                },
                WhichKeyEntryDef {
                    key: "e".into(),
                    label: "EQ".into(),
                    action: None,
                    children: Some(vec![WhichKeyEntryDef {
                        key: "r".into(),
                        label: "Rescue".into(),
                        action: Some("fx:rescue".into()),
                        children: None,
                    }]),
                },
            ],
        }];
        let config = keymap_config_from_defs(&[], &trees, &[]);
        let normal = config.keymap.get("normal").unwrap();
        assert_eq!(normal.get("v d"), Some(&"vis:drums".to_string()));
        assert_eq!(normal.get("v e r"), Some(&"fx:rescue".to_string()));
        assert_eq!(normal.get("v"), None);
    }

    #[test]
    fn context_binding_goes_to_layer() {
        let bindings = vec![KeybindDef {
            keys: "j".into(),
            action: "nav_down".into(),
            desc: None,
            context: Some(KeybindContext::Custom("editor".into())),
            passthrough: None,
            mnemonic: None,
            why: None,
        }];
        let config = keymap_config_from_defs(&bindings, &[], &[]);
        assert!(!config.keymap.get("normal").unwrap().contains_key("j"));
        let layers = config.keymap_context.get("normal").unwrap();
        assert_eq!(layers[0].when, "context:editor");
        assert_eq!(layers[0].bindings.get("j"), Some(&"nav_down".to_string()));
    }
}

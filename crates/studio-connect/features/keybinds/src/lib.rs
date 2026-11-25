//! Studio's keybind files, on top of [`architect_input`].
//!
//! The keymap engine (chords, trie, processor, which-key model, vim key
//! notation) is `architect-input`. What stays here is studio's: the
//! styx keybind format written for REAPER ([`proto`]: `KeybindDef`,
//! contexts, sections, profiles, overlays, workflows and the
//! `InputConfigService`), its conversion into a
//! [`architect_input::KeymapConfig`] ([`bridge`]), profile loading
//! ([`loader`], [`embedded`]), the shipped default keymap and the user's
//! keymap file ([`defaults`]), and, behind feature `dioxus`, the rendered
//! which-key overlay and the keymap visualizer.

#![deny(unsafe_code)]

pub mod bridge;
pub mod defaults;
#[cfg(feature = "dioxus")]
pub mod dioxus;
pub mod embedded;
pub mod loader;
pub mod proto;

pub use bridge::{context_to_when_expr, keymap_config_from_defs, section_to_keymap_config};
pub use defaults::{LoadError, default_user_config_path, load_default_config, load_user_config};
pub use loader::{
    Profile, keymap_from_section_str, list_profiles, load_profile_dir, load_profile_from,
    load_profile_keymap,
};

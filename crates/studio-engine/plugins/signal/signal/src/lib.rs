//! FTS Signal, the plugin: `signal-core` as a CLAP/VST3 (signal#26 phase 2).
//!
//! The plugin itself is [`signal_clap`], which Studio's engine also hosts
//! in-process, with no editor. This crate adds the editor ([`editor`]) and
//! exports it as a bundle, for other DAWs and as Studio's hot-reloadable
//! development copy on the desktop.

pub mod editor;

pub use signal_clap::*;

use std::sync::Arc;

use nice_plug::prelude::*;

/// The bundle's editor: the kit's mixer, a Dioxus window ([`editor::App`]).
pub struct Dioxus;

impl SignalEditor for Dioxus {
    type Editor = plugin_core::nice_plug_dioxus::editor::DioxusEditor;
    type State = Arc<plugin_core::prelude::DioxusState>;

    fn state() -> Self::State {
        plugin_core::prelude::DioxusState::new(|| editor::SIZE)
    }

    fn editor(state: &Self::State, shared: Arc<Shared>) -> Option<Self::Editor> {
        plugin_core::prelude::create_dioxus_editor_with_state(
            state.clone(),
            Arc::new(editor::EditorState { shared }),
            editor::App,
        )
    }
}

/// The plugin as the bundle ships it: with its editor.
pub type FtsSignal = signal_clap::FtsSignal<Dioxus>;

nice_export_clap!(FtsSignal);
nice_export_vst3!(FtsSignal);

//! `studio.rig.macros`: the macro bar (`guitar::ui::panels::Macros`, the iPad layout's `TouchMacroBar`), in an Architect pane.
//!
//! Nothing is redrawn: the pane is a vox client of the rig engine
//! (`studio_rig_link::RigPane`) around Signal's own panel.

use architect_extensions::linked::{Builder, Extension};
use architect_extensions_dioxus::BuilderExt as _;
use dioxus::prelude::*;
use studio_rig_link::{Owner, RigPane};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id: what its panes ask of the host, they ask as it.
pub const ID: &str = "studio.rig.macros";

pub struct Macros;

impl Extension for Macros {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        studio_rig_link::set_handle(ID, ext.handle());
        ext.view("macros", MacrosPane)
            .action("STUDIO_RIG_MACROS_STATE", |_| async {
                Ok(studio_rig_link::state())
            });
    }
}

#[component]
fn MacrosPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::Macros {} }
    }
}

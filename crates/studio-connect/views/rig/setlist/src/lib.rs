//! `studio.rig.setlist`: the sidebar (`guitar_ui::panels::Sidebar`: the iPad layout's `TabletSetlist`, `ProfileView` or `PresetView`, by footswitch mode), in an Architect pane.
//!
//! Nothing is redrawn: the pane is a vox client of the rig engine
//! (`studio_rig_link::RigPane`) around Signal's own panel.

use architect_extensions::linked::{Builder, Extension};
use architect_extensions_dioxus::BuilderExt as _;
use dioxus::prelude::*;
use studio_rig_link::{Owner, RigPane};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id: what its panes ask of the host, they ask as it.
pub const ID: &str = "studio.rig.setlist";

pub struct Setlist;

impl Extension for Setlist {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        studio_rig_link::set_handle(ID, ext.handle());
        ext.view("setlist", SetlistPane)
            .action("STUDIO_RIG_SETLIST_STATE", |_| async {
                Ok(studio_rig_link::state())
            });
    }
}

#[component]
fn SetlistPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar_ui::panels::Sidebar {} }
    }
}

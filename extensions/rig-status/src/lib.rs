//! `studio.rig.status`: the engine's three small readouts (`guitar::ui::panels::{Status, Tuner, DiPlayer}`), in an Architect pane.
//!
//! Nothing is redrawn: the pane is a vox client of the rig engine
//! (`studio_rig_link::RigPane`) around Signal's own panel.

use architect_extensions::linked::{Builder, Extension};
use architect_extensions_dioxus::BuilderExt as _;
use dioxus::prelude::*;
use studio_rig_link::{Owner, RigPane};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id: what its panes ask of the host, they ask as it.
pub const ID: &str = "studio.rig.status";

pub struct Status;

impl Extension for Status {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        studio_rig_link::set_handle(ID, ext.handle());
        ext.view("status", StatusPane)
            .view("tuner", TunerPane)
            .view("player", PlayerPane)
            .action("STUDIO_RIG_STATUS_STATE", |_| async {
                Ok(studio_rig_link::state())
            });
    }
}

#[component]
fn StatusPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::Status {} }
    }
}

#[component]
fn TunerPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::Tuner {} }
    }
}

#[component]
fn PlayerPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::DiPlayer {} }
    }
}

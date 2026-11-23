//! `studio.rig.browser`: the browser (`guitar::ui::panels::Tones`, the iPad layout's `Browser` with its captures and TONE3000 panels), in an Architect pane.
//!
//! Nothing is redrawn: the pane is a vox client of the rig engine
//! (`studio_rig_link::RigPane`) around Signal's own panel.

use architect_extensions::linked::{Builder, Extension};
use architect_extensions_dioxus::BuilderExt as _;
use dioxus::prelude::*;
use guitar::ui::panels::Focus;
use serde_json::Value as Json;
use studio_rig_link::{Owner, RigPane};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id: what its panes ask of the host, they ask as it.
pub const ID: &str = "studio.rig.browser";

pub struct Browser;

impl Extension for Browser {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        studio_rig_link::set_handle(ID, ext.handle());
        ext.view("browser", BrowserPane)
            .action("STUDIO_RIG_BROWSER_STATE", |_| async {
                Ok(studio_rig_link::state())
            })
            .action("STUDIO_RIG_BROWSER_FOCUS", |args| async move {
                let text = |key: &str| {
                    args.get(key)
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                let kind = text("kind");
                if kind.is_empty() {
                    return Err("no kind (block:delay, module:Amp, profiles, …)".to_owned());
                }
                let focus = Focus {
                    kind,
                    preset: text("preset"),
                    variation: text("variation"),
                    search: args.get("search").and_then(Json::as_bool).unwrap_or(false),
                };
                studio_rig_link::bus::put(|b| b.focus = Some(focus));
                Ok(studio_rig_link::state())
            });
    }
}

#[component]
fn BrowserPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::Tones {} }
    }
}

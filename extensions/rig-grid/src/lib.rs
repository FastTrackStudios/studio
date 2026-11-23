//! `studio.rig.grid`: the routing grid, the FX row and Edit (`guitar::ui::panels::{Grid, Fx, Edit}`). One extension: the FX row is the grid's selection drawn as controls, and Edit is the two over a section's pending changes, in an Architect pane.
//!
//! Nothing is redrawn: the pane is a vox client of the rig engine
//! (`studio_rig_link::RigPane`) around Signal's own panel.

use architect_extensions::linked::{Builder, Extension};
use architect_extensions_dioxus::BuilderExt as _;
use dioxus::prelude::*;
use guitar::ui::panels::Selected;
use serde_json::Value as Json;
use studio_rig_link::{Owner, RigPane};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id: what its panes ask of the host, they ask as it.
pub const ID: &str = "studio.rig.grid";

pub struct Grid;

impl Extension for Grid {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        studio_rig_link::set_handle(ID, ext.handle());
        ext.view("grid", GridPane)
            .view("fx", FxPane)
            .view("edit", EditPane)
            .action("STUDIO_RIG_GRID_STATE", |_| async {
                Ok(studio_rig_link::state())
            })
            .action("STUDIO_RIG_GRID_SELECT", |args| async move {
                let selection = selection(&args)?;
                studio_rig_link::bus::put(|b| b.selection = selection);
                Ok(studio_rig_link::state())
            });
    }
}

#[component]
fn GridPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::Grid {} }
    }
}

#[component]
fn FxPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::Fx {} }
    }
}

#[component]
fn EditPane() -> Element {
    rsx! {
        RigPane { owner: Owner(ID), guitar::ui::panels::Edit {} }
    }
}

/// What `STUDIO_RIG_GRID_SELECT`'s arguments select: a block, a module
/// with its blocks, or nothing.
fn selection(args: &Json) -> Result<Option<Selected>, String> {
    let text = |key: &str| args.get(key).and_then(Json::as_str).map(str::to_owned);
    match (text("block"), text("module")) {
        (Some(block), None) => Ok(Some(Selected::Block(block))),
        (None, Some(module)) => {
            let blocks = args
                .get("blocks")
                .and_then(Json::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            Ok(Some(Selected::Module(module, blocks)))
        }
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err("a block or a module, not both".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Selected, selection};
    use serde_json::json;

    #[test]
    fn the_arguments_name_a_block_a_module_or_nothing() {
        assert_eq!(
            selection(&json!({ "block": "amp-1" })),
            Ok(Some(Selected::Block("amp-1".to_owned())))
        );
        assert_eq!(
            selection(&json!({ "module": "Time", "blocks": ["d", "r"] })),
            Ok(Some(Selected::Module(
                "Time".to_owned(),
                vec!["d".to_owned(), "r".to_owned()]
            )))
        );
        assert_eq!(selection(&json!({})), Ok(None));
        assert!(selection(&json!({ "block": "a", "module": "b" })).is_err());
    }
}

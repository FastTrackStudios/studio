//! `studio.navigator`: Studio's song progress bar and set navigator in
//! Architect panes.
//!
//! Nothing is redrawn: the panes are a Studio window on Studio's engine
//! (`studio_view_link`) showing `studio_panels::progress::ProgressBar` (the
//! bar across the top of `studio-desktop`'s window) and
//! `studio_panels::navigator::Navigator` (its left column: the set as a
//! vertical progress bar), over the set the engine has.
//!
//! **In a browser** (studio-web) the same components draw over the page's
//! own engine (`studio_panels::web_host`): the set it has open, a song
//! picked there gone to ([`web`]).

use architect_extensions::linked::{Builder, Extension};
use architect_extensions_dioxus::BuilderExt as _;
use serde_json::json;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
use native as views;
#[cfg(target_arch = "wasm32")]
use web as views;

pub const MANIFEST: &str = include_str!("../extension.toml");

pub struct Navigator;

impl Extension for Navigator {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        ext.view("progress", views::ProgressPane)
            .view("navigator", views::NavigatorPane)
            .view("songs", views::SongsPane)
            .action("STUDIO_NAVIGATOR_STATE", |_| async {
                let set = views::set();
                Ok(json!({
                    "linked": studio_view_link::current().is_some(),
                    "problem": studio_view_link::problem(),
                    "songs": set.as_ref().map(|s| s.songs.len()),
                    "at": set.as_ref().map(|s| s.at),
                    "pending": set.as_ref().map(|s| s.pending.clone()),
                }))
            });
    }
}

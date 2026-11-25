//! The arrangement in a browser.
//!
//! The same panel over the page's own engine
//! (`studio_panels::web_host::HostedView`), drawn under dioxus-web with its
//! widget in a WebGL2 canvas: what the native process's pane is
//! (`crate::host`), with the page in place of the engine's socket.

use dioxus::prelude::*;
use studio_panels::web_host::{HostedPanel, HostedView};

/// The pane: the song the page's engine has current, or what it waits
/// for (`HostedView` says which).
#[component]
pub fn ArrangementPane() -> Element {
    rsx! {
        {studio_view_link::studio_style()}
        div {
            class: studio_view_link::ROOT_CLASS,
            style: "position:relative; width:100%; height:100%; overflow:hidden;",
            HostedView { panel: HostedPanel::Arrangement }
        }
    }
}

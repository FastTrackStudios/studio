//! The mixer in a browser.
//!
//! The same widget over the page's own engine
//! (`session_ui::web_host::HostedView`), drawn under dioxus-web with its
//! widget in a WebGL2 canvas.

use dioxus::prelude::*;
use session_ui::web_host::{HostedPanel, HostedView};

/// Every strip of the song the page's engine has current.
#[component]
pub fn MixerPane() -> Element {
    rsx! {
        {studio_view_link::studio_style()}
        div {
            class: studio_view_link::ROOT_CLASS,
            style: "position:relative; width:100%; height:100%; overflow:hidden;",
            HostedView { panel: HostedPanel::Mixer }
        }
    }
}

/// The selected track's strip: the desktop's for now (the browser's
/// mixer shows every strip).
#[component]
pub fn StripPane() -> Element {
    not_here("The selected track's strip is in the desktop app; the mixer shows every strip here.")
}

/// A strip's close-up: the desktop's for now.
#[component]
pub fn CloseupPane() -> Element {
    not_here("A strip's close-up is in the desktop app for now.")
}

fn not_here(why: &str) -> Element {
    let surface = studio_view_link::surface();
    rsx! {
        div { style: "padding:16px; font-size:13px; opacity:0.7; color:#ccc; background:{surface}; height:100%;",
            "{why}"
        }
    }
}

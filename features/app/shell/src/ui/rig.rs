//! The rig half of the performer screen's dock, behind a [`RigClient`]
//! (performer mounts spec STA-376 §6, §7.3).
//!
//! Signal always has a rig: its own. Session has one only when a Signal
//! peer is paired, and that peer can drop off the network mid-set. The
//! dock reads the rig through this trait so both cases draw from one
//! place, and so a test can drive every state with a fake:
//!
//! | [`RigLink`]   | rig half                                        | app bar                    |
//! |---------------|-------------------------------------------------|----------------------------|
//! | `Unpaired`    | not drawn; the transport takes the whole width | **Pair a rig**             |
//! | `Reachable`   | the rig's ten switches, live                    | rig crumb (REMOTE if a peer) |
//! | `Offline`     | the same switches at 45%, inert; same rect      | **Rig offline · Reconnect** |
//!
//! The switches themselves are the client's: the live Signal link (the
//! connect-anywhere work, C3/C7) hands back Signal's own `TouchSwitches`
//! at its `Gig` size, so the tiles are never redrawn here. Offline keeps
//! the box and its contents exactly where they were — no other region of
//! the screen moves when a rig drops.

use std::rc::Rc;

use dioxus::prelude::*;

use super::performer::HIT;
use crate::tokens::{FONT, INK, INK_2, RULE_STRONG, SHEET_2, tint};

/// A switch is pressed by its footswitch number, 1–10 (row B is 1–5 under
/// the feet, row A 6–10 the hold layer), as Signal numbers them.
pub type SwitchNo = u8;

/// How many switches a rig's dock shows.
pub const SWITCHES: usize = 10;

/// The dimmed rig's opacity: readable, plainly not live.
pub const OFFLINE_OPACITY: f64 = 0.45;

/// Where the rig is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RigLink {
    /// No rig has ever been paired with this device.
    Unpaired,
    /// The rig answers. `peer` names it when it is on another device
    /// (`Studio Mac`): the crumb says REMOTE.
    Reachable { peer: Option<String> },
    /// Paired, but not answering. The tiles stay, dimmed and inert.
    Offline { peer: Option<String> },
}

impl RigLink {
    /// Whether the dock draws a rig half at all.
    #[must_use]
    pub const fn paired(&self) -> bool {
        !matches!(self, Self::Unpaired)
    }

    /// Whether presses go through.
    #[must_use]
    pub const fn live(&self) -> bool {
        matches!(self, Self::Reachable { .. })
    }

    /// The peer the rig is on, when it is not this device.
    #[must_use]
    pub fn peer(&self) -> Option<&str> {
        match self {
            Self::Reachable { peer } | Self::Offline { peer } => peer.as_deref(),
            Self::Unpaired => None,
        }
    }
}

/// A rig the performer screen can show and play.
///
/// `link` is read in render, so a client backed by signals re-renders the
/// dock when the rig comes and goes. The actions are fire-and-forget.
pub trait RigClient {
    /// Where the rig is, now.
    fn link(&self) -> RigLink;
    /// Press a footswitch, as a foot would.
    fn press(&self, switch: SwitchNo);
    /// Try the paired rig again (the offline crumb's Reconnect).
    fn reconnect(&self) {}
    /// Start pairing a rig (the app bar's Pair a rig).
    fn pair(&self) {}
    /// The rig's ten switches at the dock's `Gig` size: the rig's own
    /// component (Signal's `TouchSwitches`), wired to this client.
    fn switches(&self) -> Element;
}

/// The app's [`RigClient`], shared through context. Equal only to itself.
#[derive(Clone)]
pub struct Rig(pub Rc<dyn RigClient>);

impl PartialEq for Rig {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::ops::Deref for Rig {
    type Target = dyn RigClient;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// Provide a rig to the dock and the app bar below.
pub fn provide_rig(client: impl RigClient + 'static) -> Rig {
    use_context_provider(|| Rig(Rc::new(client)))
}

/// The rig provided above, if any. No provider is the same as a rig
/// never paired.
#[must_use]
pub fn use_rig() -> Option<Rig> {
    try_use_context::<Rig>()
}

/// The rig's link as the screen sees it: [`RigLink::Unpaired`] without a
/// provider.
#[must_use]
pub fn rig_link() -> RigLink {
    try_consume_context::<Rig>().map_or(RigLink::Unpaired, |r| r.link())
}

/// The rig half's contents: the client's switches, live or — offline —
/// dimmed to [`OFFLINE_OPACITY`] with every press stopped, in the same box.
/// `None` when no rig is paired (the dock then gives the transport the
/// whole width).
#[must_use]
pub fn rig_half() -> Option<Element> {
    let rig = try_consume_context::<Rig>()?;
    let link = rig.link();
    if !link.paired() {
        return None;
    }
    let live = link.live();
    let opacity = if live { 1.0 } else { OFFLINE_OPACITY };
    let switches = rig.switches();
    Some(rsx! {
        div {
            "data-rig": if live { "live" } else { "offline" },
            "aria-disabled": "{!live}",
            style: "position: relative; display: flex; flex-direction: column; flex: 1; min-width: 0; opacity: {opacity};",
            {switches}
            if !live {
                // Over the tiles, the same size: a press lands here and
                // nowhere else, so an offline rig takes no input.
                div {
                    "data-rig": "shield",
                    style: "position: absolute; top: 0; left: 0; right: 0; bottom: 0;",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                }
            }
        }
    })
}

/// The rig's place in the app bar: **Pair a rig** with none paired, the
/// rig's crumb (with REMOTE when it is a peer's) when it answers, and
/// **Rig offline · Reconnect** when it doesn't. Every one is 48 tall.
#[component]
pub fn RigCrumb() -> Element {
    let rig = use_rig();
    let link = rig.as_ref().map_or(RigLink::Unpaired, |r| r.link());
    let style = |accent: Option<&str>| {
        let (edge, bg) = accent.map_or((RULE_STRONG.to_string(), SHEET_2.to_string()), |a| {
            (a.to_string(), tint(a, 14))
        });
        format!(
            "display: flex; align-items: center; gap: 8px; height: {HIT}px; padding: 0 14px; \
             box-sizing: border-box; border-radius: 8px; border: 1px solid {edge}; background: {bg}; \
             color: {INK}; font-family: {FONT}; font-size: 16px; font-weight: 650; white-space: nowrap; cursor: pointer;"
        )
    };
    match link {
        RigLink::Unpaired => {
            let pair = rig.clone();
            rsx! {
                button {
                    "data-rig": "pair",
                    "aria-label": "Pair a rig",
                    style: style(None),
                    onclick: move |_| if let Some(r) = pair.as_ref() { r.pair() },
                    "Pair a rig"
                }
            }
        }
        RigLink::Reachable { peer } => rsx! {
            div { "data-rig": "crumb", "aria-label": "Rig", style: "{style(None)} cursor: default;",
                "Rig"
                if let Some(peer) = peer {
                    span { style: "padding: 1px 6px; border-radius: 4px; font-size: 11px; font-weight: 750; letter-spacing: 0.08em; background: {tint(INK_2, 20)}; color: {INK_2};",
                        "REMOTE"
                    }
                    span { style: "font-size: 14px; color: {INK_2};", "{super::perform::budget(&peer, 16)}" }
                }
            }
        },
        RigLink::Offline { .. } => {
            let again = rig.clone();
            rsx! {
                button {
                    "data-rig": "reconnect",
                    "aria-label": "Rig offline. Reconnect",
                    style: style(Some("#f59e0b")),
                    onclick: move |_| if let Some(r) = again.as_ref() { r.reconnect() },
                    "Rig offline · Reconnect"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_say_what_the_dock_draws() {
        assert!(!RigLink::Unpaired.paired());
        assert!(!RigLink::Unpaired.live());
        let remote = RigLink::Reachable {
            peer: Some("Studio Mac".into()),
        };
        assert!(remote.paired() && remote.live());
        assert_eq!(remote.peer(), Some("Studio Mac"));
        let off = RigLink::Offline { peer: None };
        assert!(off.paired() && !off.live());
        assert_eq!(off.peer(), None);
    }
}

//! The web build's front door: who gets the app and who gets the stand-in
//! page, and where the app reaches its Drive node.
//!
//! The Drive node serves Studio under `/studio` (its `--app /studio=…`), so
//! it answers Studio's calls under `/studio/api`. That holds wherever the
//! pages themselves are served: under `/studio` on the node, or at the
//! root of fasttrackstudio.app, whose front proxies `/studio/api/*` to the
//! node and serves everything else from the web bundle.

// Only the web build opens a door; the tests run natively.
#![cfg_attr(not(feature = "web"), allow(dead_code))]

/// The path the node serves Studio under, and so where the app calls it
/// (`<mount>/api`), for pages served under `base_path` (`--base-path`).
pub fn node_mount(base_path: &str) -> &str {
    match base_path.trim_matches('/') {
        "" => "studio",
        p => p,
    }
}

/// What this browser already knows before it asks anyone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Here {
    /// The page is the issuer coming back (`<mount>/auth/callback`).
    pub callback: bool,
    /// A sign-in's token is stored and still good (no refresh needed).
    pub token: bool,
}

/// The node's answer to `/orgs` asked with no token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer<I> {
    /// It did not answer.
    NoAnswer,
    /// It lets this browser in as it is (a node with no issuer).
    In,
    /// It wants a sign-in, through this issuer (when it names one).
    SignedOut(Option<I>),
}

/// Which page this visit gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Door<I> {
    /// The app (Drive's boot then signs in, refreshes or explains).
    App,
    /// The signed-out stand-in, signing in through this issuer.
    StandIn(I),
}

impl<I> Door<I> {
    /// The door when no question needs asking: a browser signed in (or
    /// coming back from signing in) gets the app at once. `None`: ask the
    /// node ([`Door::from_answer`]).
    pub fn without_asking(here: Here) -> Option<Self> {
        (here.callback || here.token).then_some(Door::App)
    }

    /// The door from the node's answer: the stand-in only when it wants a
    /// sign-in and says through whom; anything else is the app, whose
    /// boot shows what went wrong.
    pub fn from_answer(answer: Answer<I>) -> Self {
        match answer {
            Answer::SignedOut(Some(issuer)) => Door::StandIn(issuer),
            Answer::SignedOut(None) | Answer::In | Answer::NoAnswer => Door::App,
        }
    }
}

#[cfg(feature = "web")]
pub use web::Gate;

#[cfg(feature = "web")]
mod web {
    use dioxus::prelude::*;
    use drive_dioxus::auth;

    use super::{Answer, Door, Here};

    /// The node's answer to `/orgs` with no token, as a door question.
    async fn ask() -> Answer<auth::Issuer> {
        match auth::fetch("GET", "/orgs", &[], None).await {
            None => Answer::NoAnswer,
            Some((_, v)) if v["signed_in"] == serde_json::Value::Bool(false) => {
                Answer::SignedOut(serde_json::from_value(v["auth"].clone()).ok())
            }
            Some(_) => Answer::In,
        }
    }

    /// The app for a signed-in browser (at once, nothing asked); for a
    /// signed-out one, the stand-in page in its place.
    #[component]
    pub fn Gate(children: Element) -> Element {
        let mut door = use_signal(|| {
            Door::without_asking(Here {
                callback: auth::is_callback(),
                token: matches!(auth::settled_token(MARGIN_MS), Some(Some(_))),
            })
        });
        use_future(move || async move {
            if door.peek().is_some() {
                return;
            }
            // A token that runs out soon is refreshed first: still signed
            // in, it is the app. Refused (signed out at the issuer), or
            // none stored, the node says which page it is.
            let next = if auth::fresh_token(MARGIN_MS).await.is_some() {
                Door::App
            } else {
                Door::from_answer(ask().await)
            };
            door.set(Some(next));
        });
        match door() {
            Some(Door::App) => children,
            Some(Door::StandIn(issuer)) => rsx! { StandIn { issuer } },
            None => rsx! { div { style: PAGE } },
        }
    }

    /// How long a token must still be good to count as signed in: what
    /// Drive's boot asks of it before refreshing.
    const MARGIN_MS: f64 = 60_000.0;

    const PAGE: &str = "min-height:100vh;display:grid;place-items:center;background:var(--desk,#000);color:var(--ink,#f4f4f5);font:16px/1.5 var(--font,system-ui),sans-serif;padding:24px;box-sizing:border-box;";
    const PANEL: &str =
        "max-width:420px;width:100%;display:flex;flex-direction:column;gap:16px;text-align:center;";
    const BUTTON: &str = "height:48px;border-radius:24px;border:0;background:var(--accent,#2dd4bf);color:var(--accent-ink,#042f2a);font:650 16px var(--font,system-ui),sans-serif;cursor:pointer;";

    /// What a signed-out visitor sees at fasttrackstudio.app until the
    /// landing page (studio#706) takes its place: what Studio is, and the
    /// way in.
    #[component]
    fn StandIn(issuer: auth::Issuer) -> Element {
        rsx! {
            main { style: PAGE,
                div { style: PANEL,
                    h1 { style: "margin:0;font-size:40px;font-weight:700;letter-spacing:-0.03em;",
                        "Studio"
                    }
                    p { style: "margin:0;color:var(--ink-2,#a1a1aa);",
                        "Setlists, charts and the mix for your band, church or school, on every screen."
                    }
                    button { style: BUTTON, onclick: move |_| auth::begin(&issuer), "Sign in" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_the_root_the_node_is_still_reached_under_studio() {
        assert_eq!(node_mount(""), "studio");
        assert_eq!(node_mount("/"), "studio");
    }

    #[test]
    fn under_a_path_the_node_is_reached_under_that_path() {
        assert_eq!(node_mount("studio"), "studio");
        assert_eq!(node_mount("/beta/"), "beta");
    }

    const NOBODY: Here = Here {
        callback: false,
        token: false,
    };

    #[test]
    fn a_browser_with_a_token_goes_straight_to_the_app() {
        let here = Here {
            token: true,
            ..NOBODY
        };
        assert_eq!(Door::<()>::without_asking(here), Some(Door::App));
    }

    #[test]
    fn the_issuer_coming_back_goes_to_the_app_to_finish_signing_in() {
        let here = Here {
            callback: true,
            ..NOBODY
        };
        assert_eq!(Door::<()>::without_asking(here), Some(Door::App));
    }

    #[test]
    fn a_browser_with_no_token_has_to_ask_the_node() {
        assert_eq!(Door::<()>::without_asking(NOBODY), None);
    }

    #[test]
    fn signed_out_at_a_node_with_an_issuer_is_the_stand_in() {
        let answer = Answer::SignedOut(Some("issuer"));
        assert_eq!(Door::from_answer(answer), Door::StandIn("issuer"));
    }

    #[test]
    fn a_node_that_lets_anyone_in_is_the_app() {
        assert_eq!(Door::<()>::from_answer(Answer::In), Door::App);
    }

    #[test]
    fn a_node_that_does_not_answer_or_names_no_issuer_is_the_app() {
        // The app's own boot shows why it cannot open.
        assert_eq!(Door::<()>::from_answer(Answer::NoAnswer), Door::App);
        assert_eq!(Door::<()>::from_answer(Answer::SignedOut(None)), Door::App);
    }
}

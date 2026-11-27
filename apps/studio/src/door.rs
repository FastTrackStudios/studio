//! The web build's front door: who gets the app and who gets the landing
//! page (`crate::landing`), and where the app reaches its Drive node.
//!
//! The app lives under real paths (`/home`, `/records`, `/setlists`, …;
//! `crate::Route`). Signed out, every path is the landing page, with a
//! room's link (`/join/<room>`) offering that room. Signed in, `/` is
//! replaced by `/home` (the router's redirect) and `/landing` is the
//! landing page with one way in, Open Studio ([`LandingPage`]).
//! Going into the app from the root or `/landing` (Open in browser,
//! signing in) lands on `/home`; from any other path, on that path.
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

/// The app's home screen.
pub const HOME: &str = "/home";

/// The landing page, signed in or out.
pub const LANDING: &str = "/landing";

/// A page's path within the app: `path` without the `base` it is served
/// under, always starting with `/`.
pub fn in_app<'a>(path: &'a str, base: &str) -> &'a str {
    let base = base.trim_matches('/');
    let rest = if base.is_empty() {
        path
    } else {
        path.trim_start_matches('/')
            .strip_prefix(base)
            .filter(|r| r.is_empty() || r.starts_with('/'))
            .unwrap_or(path)
    };
    if rest.is_empty() { "/" } else { rest }
}

/// Where going into the app from the landing page at `path` puts the
/// browser (Open in browser, signing in, which comes back to it): `/home`
/// from the root or `/landing`, as a full path under `base`; `None` stays
/// on the page it is (a room's link, a deep link).
pub fn into_app(path: &str, base: &str) -> Option<String> {
    match in_app(path, base).trim_end_matches('/') {
        "" | LANDING => Some(match base.trim_matches('/') {
            "" => HOME.to_owned(),
            b => format!("/{b}{HOME}"),
        }),
        _ => None,
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
    /// The signed-out landing page (`crate::landing`), signing in through
    /// this issuer.
    Landing(I),
}

impl<I> Door<I> {
    /// The door when no question needs asking: a browser signed in (or
    /// coming back from signing in) gets the app at once. `None`: ask the
    /// node ([`Door::from_answer`]).
    pub fn without_asking(here: Here) -> Option<Self> {
        (here.callback || here.token).then_some(Door::App)
    }

    /// The door from the node's answer: the landing page only when it wants a
    /// sign-in and says through whom; anything else is the app, whose
    /// boot shows what went wrong.
    pub fn from_answer(answer: Answer<I>) -> Self {
        match answer {
            Answer::SignedOut(Some(issuer)) => Door::Landing(issuer),
            Answer::SignedOut(None) | Answer::In | Answer::NoAnswer => Door::App,
        }
    }
}

#[cfg(feature = "web")]
pub use web::{Gate, enter_app};

/// `/landing` signed in (or at a node that lets anyone in): the landing
/// page, its one way in Open Studio, to `/home`. Signed out the door shows
/// the landing page itself ([`Gate`]), before the router is drawn.
#[cfg(feature = "web")]
#[dioxus::prelude::component]
pub fn LandingPage() -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    let nav = navigator();
    rsx! {
        crate::landing::Landing {
            issuer: None,
            open: move |_| {
                nav.push(crate::Route::Home {});
            },
        }
    }
}

/// The desktop and iOS builds have no landing page: their home.
#[cfg(not(feature = "web"))]
#[dioxus::prelude::component]
pub fn LandingPage() -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    let nav = navigator();
    use_effect(move || {
        nav.replace(crate::Route::Home {});
    });
    rsx! {}
}

#[cfg(feature = "web")]
mod web {
    use dioxus::prelude::*;
    use drive_dioxus::auth;

    use super::{Answer, Door, Here};

    /// Going into the app from the landing page: the root and `/landing`
    /// become `/home` in place (`history.replaceState`, so Back does not
    /// come back to a landing page that sends it on), the query kept. Any
    /// other path is left as it is.
    pub fn enter_app() {
        let base = dioxus::cli_config::web_base_path().unwrap_or_default();
        let Some(home) = super::into_app(&drive_dioxus::env::pathname(), &base) else {
            return;
        };
        let url = format!("{home}{}", drive_dioxus::env::search());
        if let Some(history) = web_sys::window().and_then(|w| w.history().ok()) {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        }
    }

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
    /// signed-out one, the landing page in its place.
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
            // Signed out: the landing page (studio#706), at every path;
            // Open in browser takes this visit into the app (at `/home`
            // from the root or `/landing`), whose boot signs in.
            Some(Door::Landing(issuer)) => rsx! {
                crate::landing::Landing {
                    issuer: Some(issuer),
                    open: move |_| {
                        enter_app();
                        door.set(Some(Door::App));
                    },
                }
            },
            None => rsx! { div { style: PAGE } },
        }
    }

    /// How long a token must still be good to count as signed in: what
    /// Drive's boot asks of it before refreshing.
    const MARGIN_MS: f64 = 60_000.0;

    const PAGE: &str = "min-height:100vh;background:var(--desk,#000);";
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

    #[test]
    fn a_path_in_the_app_is_the_path_under_its_base() {
        assert_eq!(in_app("/", ""), "/");
        assert_eq!(in_app("/home", ""), "/home");
        assert_eq!(in_app("/studio", "studio"), "/");
        assert_eq!(in_app("/studio/", "/studio/"), "/");
        assert_eq!(in_app("/studio/landing", "studio"), "/landing");
        // Not under the base: the path as it is.
        assert_eq!(in_app("/studiox/home", "studio"), "/studiox/home");
    }

    #[test]
    fn going_in_from_the_root_or_the_landing_page_lands_on_home() {
        assert_eq!(into_app("/", "").as_deref(), Some("/home"));
        assert_eq!(into_app("", "").as_deref(), Some("/home"));
        assert_eq!(into_app("/landing", "").as_deref(), Some("/home"));
        assert_eq!(into_app("/landing/", "").as_deref(), Some("/home"));
        assert_eq!(
            into_app("/studio/", "studio").as_deref(),
            Some("/studio/home")
        );
        assert_eq!(
            into_app("/studio/landing", "studio").as_deref(),
            Some("/studio/home")
        );
    }

    #[test]
    fn going_in_from_a_deep_link_stays_on_it() {
        assert_eq!(into_app("/join/adult-jam", ""), None);
        assert_eq!(into_app("/home", ""), None);
        assert_eq!(into_app("/records", ""), None);
        assert_eq!(into_app("/studio/set/main", "studio"), None);
    }

    #[test]
    fn the_root_is_home_at_its_own_path_and_landing_is_a_page() {
        use std::str::FromStr as _;
        let at = |p: &str| crate::Route::from_str(p).unwrap_or_else(|e| panic!("{p}: {e}"));
        // `/` is a redirect: the router writes `/home` in its place.
        assert_eq!(at("/"), crate::Route::Home {});
        assert_eq!(crate::Route::Home {}.to_string(), HOME);
        assert_eq!(at(HOME), crate::Route::Home {});
        assert_eq!(at(LANDING), crate::Route::LandingPage {});
        // The deep links that were there before keep their paths.
        assert_eq!(at("/records"), crate::Route::Records {});
        assert_eq!(at("/setlists"), crate::Route::Setlists {});
        assert_eq!(at("/orgs"), crate::Route::AllOrgs {});
        assert_eq!(
            at("/join/adult-jam"),
            crate::Route::JoinRoom {
                room: "adult-jam".into()
            }
        );
        assert_eq!(at("/now/chart"), crate::Route::NowChart {});
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
    fn signed_out_at_a_node_with_an_issuer_is_the_landing_page() {
        let answer = Answer::SignedOut(Some("issuer"));
        assert_eq!(Door::from_answer(answer), Door::Landing("issuer"));
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

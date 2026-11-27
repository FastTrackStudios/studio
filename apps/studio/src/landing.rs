//! The landing page: what a signed-out visitor to fasttrackstudio.app sees
//! (studio#706). What Studio is (setlists, charts, rooms, mixing), the real
//! app's screens, a download for each platform from the newest release,
//! and the ways in: Open in browser, Sign in, Join a room.
//!
//! It is the web build's alone (the component is compiled only with the
//! `web` feature): the desktop and iOS builds open straight into the app.
//! The door (`door::Gate`) shows it to a signed-out visitor at any path;
//! `/landing` shows it signed in too (`door::LandingPage`), its two ways in
//! (Open in browser, Sign in) then one, Open Studio. This module is what
//! it shows.
//!
//! The downloads are read from Studio's GitHub Releases when the page
//! opens (the browser asks GitHub's API; `?releases=<url>` names another
//! list, for tests). Until it answers, or if it cannot, every button goes
//! to the releases page, which always has the newest.

// The logic is tested natively; only the web build draws the page.
#![cfg_attr(not(feature = "web"), allow(dead_code))]

use serde_json::Value;

/// Studio's releases, newest first, pre-releases included (Studio's alpha
/// releases are pre-releases, which `/releases/latest` skips).
pub const RELEASES_API: &str =
    "https://api.github.com/repos/FastTrackStudios/studio/releases?per_page=30";

/// Where every release is listed: the buttons' place until the list is read.
pub const RELEASES_PAGE: &str = "https://github.com/FastTrackStudios/studio/releases";

/// What a TestFlight public link starts with, found in a release's notes.
const TESTFLIGHT: &str = "https://testflight.apple.com/join/";

/// A platform Studio is downloaded for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// macOS on Apple silicon: the notarized `.dmg`.
    Macos,
    /// Linux on `x86_64`: the `AppImage`.
    Linux,
    /// iPhone and iPad: TestFlight.
    Ios,
}

impl Platform {
    /// The landing page's order.
    pub const ALL: [Self; 3] = [Self::Macos, Self::Linux, Self::Ios];

    /// What the page and its tests call it (`data-download`).
    pub const fn key(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Ios => "ios",
        }
    }

    /// The button's words.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Macos => "macOS",
            Self::Linux => "Linux",
            Self::Ios => "iPhone & iPad",
        }
    }

    /// What it is, under the label.
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Macos => "Apple silicon, .dmg",
            Self::Linux => "x86_64, AppImage",
            Self::Ios => "TestFlight",
        }
    }

    /// The release asset for `version`, as the release workflow names it
    /// (studio-shell's `updates::Platform::asset`, which the desktop's
    /// self-update reads). iOS has none: it is TestFlight's.
    fn asset(self, version: &str) -> Option<String> {
        match self {
            Self::Macos => Some(format!("Studio-{version}-macos-arm64.dmg")),
            Self::Linux => Some(format!("Studio-{version}-linux-x86_64.AppImage")),
            Self::Ios => None,
        }
    }
}

/// One download button: where it goes, and the version it gets (`None`:
/// the releases page, no release read yet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    pub platform: Platform,
    pub url: String,
    pub version: Option<String>,
}

/// Every button before the release list is read: the releases page.
pub fn unread() -> Vec<Download> {
    Platform::ALL
        .iter()
        .map(|&platform| Download {
            platform,
            url: RELEASES_PAGE.to_owned(),
            version: None,
        })
        .collect()
}

/// A release's version, from its tag: `studio-vYY.M.MICRO`, or the
/// `vYY.M.MICRO` Studio was released under before the monorepo.
fn version_of(tag: &str) -> Option<(u32, u32, u32)> {
    let v = tag
        .strip_prefix("studio-v")
        .or_else(|| tag.strip_prefix('v'))?;
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// A release that counts: published (not a draft), with a release's tag.
struct Release<'a> {
    version: (u32, u32, u32),
    /// Its tag without the `studio-v`/`v`: what its assets are named by.
    name: &'a str,
    json: &'a Value,
}

fn releases(list: &Value) -> Vec<Release<'_>> {
    let mut out: Vec<Release<'_>> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["draft"].as_bool() != Some(true))
        .filter_map(|json| {
            let tag = json["tag_name"].as_str()?;
            let version = version_of(tag)?;
            let name = tag.strip_prefix("studio-v").unwrap_or(&tag[1..]);
            Some(Release {
                version,
                name,
                json,
            })
        })
        .collect();
    // Newest first, whatever order the list came in.
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

/// The asset named `name` in `release`: its download link.
fn asset_url(release: &Value, name: &str) -> Option<String> {
    release["assets"]
        .as_array()?
        .iter()
        .find(|a| a["name"].as_str() == Some(name))?["browser_download_url"]
        .as_str()
        .map(str::to_owned)
}

/// The TestFlight public link a release's notes give, if any.
fn testflight_in(release: &Value) -> Option<String> {
    let body = release["body"].as_str()?;
    let at = body.find(TESTFLIGHT)?;
    let link: String = body[at..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '.' | '-' | '_'))
        .collect();
    (link.len() > TESTFLIGHT.len()).then_some(link)
}

/// Each platform's button from GitHub's release list (`/releases`):
/// - macOS and Linux: the newest release that carries the platform's
///   file, downloaded directly;
/// - iPhone and iPad: the TestFlight link in the newest release's notes
///   that has one, else the newest release's page.
///
/// A platform no release has yet goes to the releases page.
pub fn downloads(list: &Value) -> Vec<Download> {
    let releases = releases(list);
    Platform::ALL
        .iter()
        .map(|&platform| {
            let found = releases.iter().find_map(|r| {
                let url = match platform.asset(r.name) {
                    Some(asset) => asset_url(r.json, &asset)?,
                    None => testflight_in(r.json)?,
                };
                Some((url, r.name))
            });
            let found = found.or_else(|| match platform {
                Platform::Ios => releases.first().and_then(|r| {
                    let page = r.json["html_url"].as_str()?;
                    Some((page.to_owned(), r.name))
                }),
                _ => None,
            });
            match found {
                Some((url, version)) => Download {
                    platform,
                    url,
                    version: Some(version.to_owned()),
                },
                None => Download {
                    platform,
                    url: RELEASES_PAGE.to_owned(),
                    version: None,
                },
            }
        })
        .collect()
}

/// The room a visitor typed: its link (`https://fasttrackstudio.app/join/<room>`,
/// with or without the scheme), the app's link (`fasttrackstudio://join/<room>`),
/// or the room's id alone. The same rule as the room's own
/// (`room::parse_link`, which the native builds use): an id is lower-case
/// letters, digits and dashes. `None`: not a room.
pub fn room_in(typed: &str) -> Option<String> {
    let typed = typed.trim();
    let rest = typed
        .strip_prefix("https://")
        .or_else(|| typed.strip_prefix("http://"))
        .unwrap_or(typed);
    let id = match rest
        .strip_prefix("fasttrackstudio.app/join/")
        .or_else(|| typed.strip_prefix("fasttrackstudio://join/"))
    {
        Some(id) => id.trim_end_matches('/'),
        None if rest.contains('/') => return None,
        None => rest,
    };
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    ok.then(|| id.to_owned())
}

/// The room a page's path names: `<base>/join/<room>`, the room's link on
/// this site (`base` is the path the page is served under).
pub fn room_on_path(path: &str, base: &str) -> Option<String> {
    let base = base.trim_matches('/');
    let path = path.trim_start_matches('/');
    let path = if base.is_empty() {
        path
    } else {
        path.strip_prefix(base)?.trim_start_matches('/')
    };
    let id = path.strip_prefix("join/")?.trim_end_matches('/');
    room_in(id)
}

/// Where a room's link goes on this site: the room link flow
/// (`<base>/join/<room>`).
pub fn join_path(room: &str, base: &str) -> String {
    match base.trim_matches('/') {
        "" => format!("/join/{room}"),
        b => format!("/{b}/join/{room}"),
    }
}

/// The room in the app's own scheme: opens Studio on a computer, iPhone or
/// iPad that has it, straight into the room.
pub fn app_link(room: &str) -> String {
    format!("fasttrackstudio://join/{room}")
}

/// What Studio is, in four parts, under the hero: from the spec's story,
/// the musician's side.
pub const POINTS: [(&str, &str); 4] = [
    (
        "Setlists",
        "Build the running order from your sessions, with each song's key and who leads it. Plan it on your phone, in a browser or on the desktop.",
    ),
    (
        "Charts",
        "Every song has its chart and the chart follows the playhead. A song without one gets a chart generated from its sections and bars.",
    ),
    (
        "Rooms",
        "One link your band keeps. Whatever the leader loads, everyone follows, with no reconnecting, even on a venue's Wi-Fi with no internet.",
    ),
    (
        "Mixing",
        "The whole session on every platform: arrangement, mixer, first-party FX and Signal. A mix sounds the same everywhere.",
    ),
];

/// The real app's screens (made by the web build's screen tests,
/// `just test-web-screens`, from `target/web-shots`: `phone-entry`,
/// `phone-chart`, `ipad-arrangement`, `ipad-mixer`), kept in
/// `public/landing/`: what each shows. One not there yet is hidden.
pub const SCREENS: [(&str, &str); 4] = [
    ("setlist.png", "A setlist's running order on a phone"),
    ("chart.png", "A song's chart following the playhead"),
    ("arrangement.png", "The session's arrangement on an iPad"),
    ("mixer.png", "The session's mixer on an iPad"),
];

#[cfg(feature = "web")]
pub use web::Landing;

#[cfg(feature = "web")]
mod web {
    use dioxus::prelude::*;
    use drive_dioxus::auth;

    use super::*;

    const STYLE: Asset = asset!("/assets/landing.css");

    /// The release list this page reads: `?releases=<url>`, else GitHub's.
    fn releases_url() -> String {
        let search = drive_dioxus::env::search();
        drive_dioxus::env::query_param(&search, "releases")
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| RELEASES_API.to_owned())
    }

    /// The path the page is served under (`--base-path`).
    fn base() -> String {
        dioxus::cli_config::web_base_path().unwrap_or_default()
    }

    /// Sign in through `issuer`, coming back to `/home` from the root or
    /// `/landing`, or to the link that sent the visitor here.
    fn sign_in(issuer: &auth::Issuer) {
        crate::door::enter_app();
        auth::begin(issuer);
    }

    /// The landing page. Signed out, signing in through `issuer`; signed
    /// in (`None`), its one way in is Open Studio. `open` takes the visitor
    /// into the app in this browser.
    #[component]
    pub fn Landing(issuer: Option<auth::Issuer>, open: EventHandler<()>) -> Element {
        let mut downloads = use_signal(unread);
        use_future(move || async move {
            let headers = [("accept", "application/vnd.github+json")];
            if let Some((200, list)) = auth::fetch("GET", &releases_url(), &headers, None).await {
                downloads.set(super::downloads(&list));
            }
        });
        // A room's link opened signed out: that room, to join.
        let room = room_on_path(&drive_dioxus::env::pathname(), &base());
        let signed_in = issuer.is_none();
        let header_sign_in = issuer.clone();
        let room = room.zip(issuer.clone());
        let shots = format!("{}/landing", base().trim_end_matches('/'));
        let buttons: Vec<_> = downloads()
            .into_iter()
            .map(|d| {
                let p = d.platform;
                let detail = match &d.version {
                    Some(v) => format!("{} · {v}", p.detail()),
                    None => p.detail().to_owned(),
                };
                (
                    p.key(),
                    p.label(),
                    detail,
                    d.version.unwrap_or_default(),
                    d.url,
                )
            })
            .collect();
        rsx! {
            document::Stylesheet { href: STYLE }
            main { class: "landing", "data-landing": "",
                header { class: "lhead",
                    span { class: "lbrand", span { class: "lmark", "S" } "Studio" }
                    if let Some(issuer) = header_sign_in {
                        button {
                            class: "lsignin",
                            "data-action": "sign-in",
                            onclick: move |_| sign_in(&issuer),
                            "Sign in"
                        }
                    }
                }
                if let Some((room, issuer)) = room {
                    RoomDoor { room, issuer }
                }
                section { class: "lhero",
                    h1 { "Every song in the set, ready to play." }
                    p { class: "llead",
                        "Studio keeps your band's, church's or school's setlists, charts and sessions in one place, and plays them on every screen: macOS, Linux, iPhone, iPad and the browser."
                    }
                    div { class: "lways",
                        if signed_in {
                            button {
                                class: "lprimary",
                                "data-action": "open-studio",
                                onclick: move |_| open.call(()),
                                "Open Studio"
                            }
                        } else {
                            button {
                                class: "lprimary",
                                "data-action": "open-in-browser",
                                onclick: move |_| open.call(()),
                                "Open in browser"
                            }
                        }
                        JoinForm {}
                    }
                }
                section { class: "lpoints",
                    for (title, body) in POINTS {
                        article { class: "lpoint", key: "{title}",
                            h2 { "{title}" }
                            p { "{body}" }
                        }
                    }
                }
                section { class: "lscreens", aria_label: "Screens from Studio",
                    for (file, what) in SCREENS {
                        figure { class: "lshot", key: "{file}",
                            // A screen not yet in `public/landing/` is left
                            // out, not shown broken.
                            img {
                                src: "{shots}/{file}",
                                alt: "{what}",
                                "loading": "lazy",
                                "onerror": "this.closest('figure').hidden = true",
                            }
                            figcaption { "{what}" }
                        }
                    }
                }
                section { class: "ldownloads", id: "download",
                    h2 { "Download Studio" }
                    div { class: "lgrid",
                        for (key, label, detail, version, url) in buttons {
                            a {
                                class: "ldownload",
                                key: "{key}",
                                "data-download": key,
                                "data-version": "{version}",
                                href: "{url}",
                                rel: "noopener",
                                span { class: "lplat", "{label}" }
                                span { class: "ldetail", "{detail}" }
                            }
                        }
                    }
                    p { class: "lnote",
                        "Or use it here, nothing to install: "
                        button { class: "linline", onclick: move |_| open.call(()), "open Studio in your browser" }
                        "."
                    }
                }
                footer { class: "lfoot",
                    "FastTrack Studios · Studio is open source (GPL-3.0-or-later) · "
                    a { href: "https://github.com/FastTrackStudios/studio", "GitHub" }
                }
            }
        }
    }

    /// Join a room: its link (or name) typed, then the room's link on this
    /// site, where the room link flow takes over.
    #[component]
    fn JoinForm() -> Element {
        let mut open = use_signal(|| false);
        let mut typed = use_signal(String::new);
        let mut wrong = use_signal(|| false);
        if !open() {
            return rsx! {
                button {
                    class: "lsecondary",
                    "data-action": "join-room",
                    onclick: move |_| open.set(true),
                    "Join a room"
                }
            };
        }
        let mut go = move || match room_in(&typed.peek()) {
            Some(room) => drive_dioxus::env::go(&join_path(&room, &base())),
            None => wrong.set(true),
        };
        rsx! {
            form {
                class: "ljoin",
                "data-join": "",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    go();
                },
                input {
                    class: "ljoinin",
                    r#type: "text",
                    name: "room",
                    autofocus: true,
                    autocomplete: "off",
                    placeholder: "fasttrackstudio.app/join/…",
                    aria_label: "The room's link",
                    value: "{typed}",
                    oninput: move |e| {
                        typed.set(e.value());
                        wrong.set(false);
                    },
                }
                button { class: "lprimary", r#type: "submit", "Join" }
                if wrong() {
                    p { class: "lwrong", role: "alert",
                        "That isn't a room's link. It looks like fasttrackstudio.app/join/your-room."
                    }
                }
            }
        }
    }

    /// A room's link opened by someone signed out: the room, and how to
    /// get in (signing in comes back to this link).
    #[component]
    fn RoomDoor(room: String, issuer: auth::Issuer) -> Element {
        let app = app_link(&room);
        rsx! {
            section { class: "lroom", "data-room": "{room}",
                p { class: "lkicker", "You're invited to a room" }
                h2 { "{room}" }
                p { "Sign in to join it here, or open it in Studio on your computer, iPhone or iPad." }
                div { class: "lways",
                    button {
                        class: "lprimary",
                        "data-action": "sign-in-to-join",
                        onclick: move |_| sign_in(&issuer),
                        "Sign in to join"
                    }
                    a { class: "lsecondary", href: "{app}", "Open in Studio" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

//! A Task link, pasted to open a set in Session's web app.
//!
//! Session's web app (`/app/`) opens what its address names: a set Task
//! keeps (`?live=<link>`), or the song a share link shares
//! (`?share=<link>`). Nobody types those addresses. They paste what Task
//! gave them: a live share link (`https://task…/org/<org>/share/<token>`,
//! `?pw=` when it has one), or an app address someone sent with the link
//! already in it. [`parse`] reads either, and [`Opening::app_url`] is
//! where the page goes with it.
//!
//! A bare Task link opens as a set: that is what a live share link is
//! for, and what the desktop app does with one (`native::start`'s
//! `live_link`). A song's own share link comes in an app address that
//! says `share=`.
//!
//! [`app_open`] is the other half: what `/app/` itself opens from its
//! query, and [`AppOpen::Nothing`] when it names nothing, which the page
//! shows as [`OpenSet`] rather than a song it does not have.

use std::fmt;

/// What a pasted link opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opening {
    /// A set Task keeps, joined by its live share link.
    Live { link: String, guide: Option<String> },
    /// One song, by its folder's share link.
    Share { link: String, guide: Option<String> },
}

impl Opening {
    /// The Session web app's address with this link in it.
    #[must_use]
    pub fn app_url(&self) -> String {
        let (key, link, guide) = match self {
            Self::Live { link, guide } => ("live", link, guide),
            Self::Share { link, guide } => ("share", link, guide),
        };
        let mut url = format!("/app/?{key}={}", encode(link));
        if let Some(guide) = guide {
            url.push_str("&guide=");
            url.push_str(&encode(guide));
        }
        url
    }

    /// The Task link itself.
    #[must_use]
    pub fn link(&self) -> &str {
        match self {
            Self::Live { link, .. } | Self::Share { link, .. } => link,
        }
    }
}

/// Why a pasted text opens nothing — each a sentence the page can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Nothing was pasted.
    Empty,
    /// The text is not a web address.
    NotALink,
    /// A web address, but not a Task share or live link.
    NotATaskLink,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "Paste the link Task gave you for the set.",
            Self::NotALink => "That isn't a web link. Paste the whole link Task gave you.",
            Self::NotATaskLink => {
                "That isn't a Task share link. It looks like https://task…/org/<org>/share/<code>."
            }
        })
    }
}

impl std::error::Error for LinkError {}

/// What `text` opens: a Task live or share link as it is, or the one a
/// Session app address carries (`…/app/?live=<link>`, `?share=<link>`).
/// Typed as people type them: with or without `https://`, with spaces
/// around them.
///
/// # Errors
///
/// [`LinkError`]: the text is empty, not a web address, or not a Task link.
pub fn parse(text: &str) -> Result<Opening, LinkError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(LinkError::Empty);
    }
    let url = Url::read(text)?;
    // An app address with the link in it: that link, of that kind.
    let guide = url.param("guide").map(|g| decode(&g));
    for (key, live) in [("live", true), ("share", false)] {
        if let Some(carried) = url.param(key) {
            let inner = Url::read(&decode(&carried))?;
            if !inner.is_task_share() {
                return Err(LinkError::NotATaskLink);
            }
            let link = inner.whole();
            return Ok(if live {
                Opening::Live { link, guide }
            } else {
                Opening::Share { link, guide }
            });
        }
    }
    if url.is_task_share() {
        return Ok(Opening::Live {
            link: url.whole(),
            guide: None,
        });
    }
    Err(LinkError::NotATaskLink)
}

/// What the Session web app opens, from its address's query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppOpen {
    /// A set Task keeps (`?live=<link>`), joined as `name` when the
    /// address names one.
    Live { link: String, name: Option<String> },
    /// One song (`?share=<link>`).
    Shared { link: String },
    /// No link at all: the page asks for one ([`OpenSet`]).
    Nothing,
}

/// What the app opens, from `param` (a query parameter's value, `None`
/// when it is absent or empty), and the guide library's link.
pub fn app_open(param: impl Fn(&str) -> Option<String>) -> (AppOpen, Option<String>) {
    let param = |key: &str| param(key).filter(|v| !v.trim().is_empty());
    let open = match (param("live"), param("share")) {
        (Some(link), _) => AppOpen::Live {
            link,
            name: param("name"),
        },
        (None, Some(link)) => AppOpen::Shared { link },
        (None, None) => AppOpen::Nothing,
    };
    (open, param("guide"))
}

/// A web address, read loosely: its scheme, host, path and query.
struct Url {
    scheme: String,
    host: String,
    path: String,
    query: Option<String>,
}

impl Url {
    fn read(text: &str) -> Result<Self, LinkError> {
        if text.chars().any(char::is_whitespace) {
            return Err(LinkError::NotALink);
        }
        let (scheme, rest) = match text.split_once("://") {
            Some((s, rest))
                if s.eq_ignore_ascii_case("https") || s.eq_ignore_ascii_case("http") =>
            {
                (s.to_ascii_lowercase(), rest)
            }
            Some(_) => return Err(LinkError::NotALink),
            None => ("https".to_owned(), text),
        };
        let rest = rest.split('#').next().unwrap_or_default();
        let (before_query, query) = match rest.split_once('?') {
            Some((b, q)) => (b, Some(q.to_owned())),
            None => (rest, None),
        };
        let (host, path) = before_query.split_once('/').unwrap_or((before_query, ""));
        let name = host.split(':').next().unwrap_or_default();
        let good_host = !name.is_empty()
            && !host.contains('@')
            && (name.contains('.') || name == "localhost")
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            && !name.starts_with('.')
            && !name.ends_with('.');
        if !good_host {
            return Err(LinkError::NotALink);
        }
        Ok(Self {
            scheme,
            host: host.to_ascii_lowercase(),
            path: path.trim_end_matches('/').to_owned(),
            query,
        })
    }

    /// A query parameter's raw (still encoded) value, when it has one.
    fn param(&self, key: &str) -> Option<String> {
        self.query.as_deref()?.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == key && !v.is_empty()).then(|| v.to_owned())
        })
    }

    /// `org/<org>/share/<token>`: a Task share link's landing.
    fn is_task_share(&self) -> bool {
        let segments: Vec<&str> = self.path.split('/').collect();
        let token_ok = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        matches!(segments.as_slice(), ["org", org, "share", token] if token_ok(org) && token_ok(token))
    }

    /// The address, whole: scheme, host, path and the share's password.
    fn whole(&self) -> String {
        let mut url = format!("{}://{}/{}", self.scheme, self.host, self.path);
        if let Some(pw) = self.param("pw") {
            url.push_str("?pw=");
            url.push_str(&pw);
        }
        url
    }
}

/// `text` as a query value: everything but the unreserved characters
/// percent-encoded.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A query value, decoded (`%XX` and `+`).
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..=i + 2]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(feature = "ui")]
pub use form::{OpenSet, OpenSetForm};

#[cfg(feature = "ui")]
mod form {
    use dioxus::prelude::*;

    /// The paste box: a Task link in, the app opened on it. Says why a
    /// link won't open, in words, under the box.
    #[component]
    pub fn OpenSetForm(
        /// Where an opened link goes; by default the page goes to the app
        /// (`/app/?live=…`).
        on_open: Option<EventHandler<super::Opening>>,
    ) -> Element {
        let mut pasted = use_signal(String::new);
        let mut problem = use_signal(|| None::<super::LinkError>);
        let mut submit = move || match super::parse(&pasted.peek()) {
            Ok(opening) => {
                problem.set(None);
                match on_open {
                    Some(on_open) => on_open.call(opening),
                    None => {
                        let url = opening.app_url();
                        let _ = dioxus::document::eval(&format!("window.location.assign({url:?})"));
                    }
                }
            }
            Err(e) => problem.set(Some(e)),
        };
        rsx! {
            form {
                novalidate: true,
                style: "display:flex; flex-direction:column; gap:8px; width:100%; max-width:520px;",
                onsubmit: move |e| {
                    e.prevent_default();
                    submit();
                },
                div {
                    style: "display:flex; gap:8px; width:100%;",
                    input {
                        // Text, not `url`: the browser's own URL check would
                        // refuse a link pasted without `https://` (which
                        // `parse` takes) and say so in its own words, before
                        // ours.
                        r#type: "text",
                        inputmode: "url",
                        name: "link",
                        "aria-label": "Task link",
                        placeholder: "https://task…/org/…/share/…",
                        autocomplete: "off",
                        spellcheck: "false",
                        style: "flex:1; min-width:0; height:40px; padding:0 12px; border-radius:8px; \
                                border:1px solid #3f3f46; background:#18181b; color:#f4f4f5; \
                                font-size:14px; font-family:ui-monospace, monospace;",
                        value: "{pasted}",
                        oninput: move |e| {
                            pasted.set(e.value());
                            problem.set(None);
                        },
                    }
                    button {
                        r#type: "submit",
                        style: "flex:none; height:40px; padding:0 18px; border-radius:8px; border:none; \
                                background:#38bdf8; color:#09090b; font-weight:600; font-size:14px; cursor:pointer;",
                        "Open"
                    }
                }
                if let Some(problem) = problem() {
                    div {
                        role: "alert",
                        style: "font-size:13px; color:#f87171; line-height:1.5;",
                        "{problem}"
                    }
                }
            }
        }
    }

    /// The app with no link: what it opens and how to give it one, rather
    /// than a song it doesn't have.
    #[component]
    pub fn OpenSet() -> Element {
        rsx! {
            div {
                "data-empty-state": "open-a-set",
                style: "position:absolute; top:0; left:0; width:100vw; min-height:100vh; display:flex; \
                        flex-direction:column; align-items:center; justify-content:center; gap:14px; \
                        background:#0f1012; color:#e5e7eb; font-family:system-ui, sans-serif; \
                        text-align:center; padding:0 24px; box-sizing:border-box;",
                div { style: "font-size:22px; font-weight:700;", "Open a set" }
                div {
                    style: "font-size:14px; color:#9aa0a6; max-width:460px; line-height:1.5;",
                    "Paste the share link Task gave you for your set, and Session opens it here."
                }
                OpenSetForm {}
                a {
                    href: "/demo",
                    style: "font-size:13px; color:#38bdf8; text-decoration:underline; margin-top:6px;",
                    "Or try the live demo"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppOpen, LinkError, Opening, app_open, parse};

    const LINK: &str = "https://task.fasttrackstudio.app/org/days-to-praise/share/1e75d7fa11c5";

    #[test]
    fn a_task_share_link_opens_its_set() {
        assert_eq!(
            parse(LINK),
            Ok(Opening::Live {
                link: LINK.to_owned(),
                guide: None
            })
        );
    }

    #[test]
    fn a_link_is_read_as_people_paste_it() {
        let want = Ok(Opening::Live {
            link: LINK.to_owned(),
            guide: None,
        });
        assert_eq!(parse(&format!("  {LINK}/ \n")), want);
        assert_eq!(parse(LINK.strip_prefix("https://").unwrap()), want);
        assert_eq!(parse(&format!("{LINK}#top")), want);
    }

    #[test]
    fn the_shares_password_rides_along() {
        let with_pw = format!("{LINK}?pw=s3cret");
        assert_eq!(parse(&with_pw).unwrap().link(), with_pw);
    }

    #[test]
    fn an_app_address_opens_the_link_it_carries() {
        let live = Opening::Live {
            link: LINK.to_owned(),
            guide: None,
        };
        let address = format!("https://session.fasttrackstudio.app{}", live.app_url());
        assert_eq!(parse(&address), Ok(live));

        let share = Opening::Share {
            link: LINK.to_owned(),
            guide: Some("https://task.example/org/x/share/guide".to_owned()),
        };
        let address = format!("https://session.fasttrackstudio.app{}", share.app_url());
        assert_eq!(parse(&address), Ok(share));
    }

    #[test]
    fn the_app_url_encodes_the_link() {
        let live = parse(LINK).unwrap();
        assert_eq!(
            live.app_url(),
            "/app/?live=https%3A%2F%2Ftask.fasttrackstudio.app%2Forg%2Fdays-to-praise%2Fshare%2F1e75d7fa11c5"
        );
    }

    #[test]
    fn junk_is_refused_with_a_reason() {
        assert_eq!(parse(""), Err(LinkError::Empty));
        assert_eq!(parse("   "), Err(LinkError::Empty));
        assert_eq!(parse("hello there"), Err(LinkError::NotALink));
        assert_eq!(parse("just-a-word"), Err(LinkError::NotALink));
        assert_eq!(
            parse("ftp://task.example/org/a/share/b"),
            Err(LinkError::NotALink)
        );
        assert_eq!(parse("javascript:alert(1)"), Err(LinkError::NotALink));
        assert_eq!(
            parse("https://user@task.example/org/a/share/b"),
            Err(LinkError::NotALink)
        );
        assert_eq!(parse("https://example.com/"), Err(LinkError::NotATaskLink));
        assert_eq!(
            parse("https://task.example/org/a"),
            Err(LinkError::NotATaskLink)
        );
        assert_eq!(
            parse("https://task.example/org//share/b"),
            Err(LinkError::NotATaskLink)
        );
        assert_eq!(
            parse("https://task.example/org/a/share/b/download/song.ogg"),
            Err(LinkError::NotATaskLink)
        );
        assert_eq!(
            parse("https://session.example/app/?live=https%3A%2F%2Fevil.example%2F"),
            Err(LinkError::NotATaskLink)
        );
        // Every refusal is a sentence, not a code.
        for e in [
            LinkError::Empty,
            LinkError::NotALink,
            LinkError::NotATaskLink,
        ] {
            assert!(e.to_string().ends_with('.'));
        }
    }

    fn query<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn the_app_with_no_link_opens_nothing() {
        assert_eq!(app_open(query(&[])), (AppOpen::Nothing, None));
        assert_eq!(
            app_open(query(&[("live", ""), ("share", " ")])).0,
            AppOpen::Nothing
        );
    }

    #[test]
    fn the_app_opens_what_its_address_names() {
        assert_eq!(
            app_open(query(&[("live", LINK), ("name", "Cody"), ("guide", "g")])),
            (
                AppOpen::Live {
                    link: LINK.to_owned(),
                    name: Some("Cody".to_owned())
                },
                Some("g".to_owned())
            )
        );
        assert_eq!(
            app_open(query(&[("share", LINK)])).0,
            AppOpen::Shared {
                link: LINK.to_owned()
            }
        );
    }

    /// The page with no link renders the empty state: its headline, the
    /// paste box and the way to the demo, and no error.
    #[cfg(feature = "ui")]
    #[test]
    fn the_empty_state_renders_when_there_is_no_link() {
        use dioxus::prelude::*;
        assert_eq!(app_open(query(&[])).0, AppOpen::Nothing);
        fn page() -> Element {
            rsx! { super::OpenSet {} }
        }
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("data-empty-state=\"open-a-set\""), "{html}");
        assert!(html.contains("Open a set"), "{html}");
        assert!(html.contains("<input"), "{html}");
        // The browser's own URL check stays out of it: a link without
        // https:// is ours to read, and junk gets our sentence.
        assert!(!html.contains("type=\"url\""), "{html}");
        assert!(html.contains("href=\"/demo\""), "{html}");
        assert!(!html.contains("did not open"), "{html}");
    }
}

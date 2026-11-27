//! A new organization, made at the issuer (FastTrackStudios/auth): any
//! signed-in account creates one there and is its owner. Studio keeps no
//! list of orgs of its own, so once the issuer has it, it is simply one
//! more of this person's orgs (the switcher, All organizations).
//!
//! The issuer creates an org only for a *session* (an OIDC access token is
//! refused for every change to an org), so the app trades its own access
//! token, freshly issued, for one (RFC 8693, `/oauth2/token`): the issuer
//! allows that for a first-party client with `issue_sessions`, within five
//! minutes of the token's issue, once per token. Then
//! `POST /auth/organization/create` with that session as its bearer.
//!
//! Everything here is plain requests and answers over a [`Fetch`], so it
//! is the same in a browser and natively, and tested against the real
//! issuer's routes.

use std::future::Future;

/// One HTTP request: method, full URL, headers, body. Its answer: the
/// status and the body as text, `None` when nothing answered.
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// What sends a [`Request`] (`drive_dioxus::auth::fetch` in the app).
pub trait Fetch {
    fn send(&self, request: Request) -> impl Future<Output = Option<(u16, String)>>;
}

/// What a new org is to be called, checked: its name, and the slug the
/// issuer keys it on (what its documents' ids and its links carry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub slug: String,
}

/// Why an org was not made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No name given.
    NoName,
    /// A name with no letters or digits gives no slug.
    NoSlug,
    /// The issuer already has an org with this slug.
    Taken(String),
    /// Not signed in (or the issuer no longer takes the sign-in).
    SignIn,
    /// The issuer does not let this app trade its sign-in for a session
    /// (its client wants `issue_sessions` at the account server).
    NotAllowed,
    /// The issuer could not be reached, or said something else.
    Issuer(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NoName => write!(f, "give the organization a name"),
            Refused::NoSlug => write!(f, "the name needs a letter or a digit"),
            Refused::Taken(slug) => write!(f, "there is already an organization called {slug}"),
            Refused::SignIn => write!(f, "sign in to create an organization"),
            Refused::NotAllowed => write!(
                f,
                "the account server does not let this app create organizations yet"
            ),
            Refused::Issuer(why) => write!(f, "the account server: {why}"),
        }
    }
}

/// The org the issuer made: its slug (Studio's id for it), its name, and
/// the creator's role in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Created {
    pub org: String,
    pub name: String,
    pub role: String,
}

/// A name reduced to a slug the way the issuer's own pages do it: runs of
/// anything but ASCII letters and digits become one dash, lower case, no
/// dash at either end ("Days to Praise!" → `days-to-praise`).
pub fn slug_of(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    out
}

/// The org to make from what was typed: the name, and a slug (from the
/// name when none, or an empty one, is given).
pub fn draft(name: &str, slug: Option<&str>) -> Result<Draft, Refused> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Refused::NoName);
    }
    let slug = slug_of(slug.filter(|s| !s.trim().is_empty()).unwrap_or(name));
    if slug.is_empty() {
        return Err(Refused::NoSlug);
    }
    Ok(Draft {
        name: name.to_string(),
        slug,
    })
}

/// RFC 8693's names, as the issuer takes them.
const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const ACCESS_TOKEN: &str = "urn:ietf:params:oauth:token-type:access_token";
const SESSION_TOKEN: &str = "urn:fasttrackstudio:params:oauth:token-type:session";

fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn base(issuer: &str) -> &str {
    issuer.trim_end_matches('/')
}

/// The issuer's words for a refusal (its error body's `message`, an OAuth
/// error's `error_description`), or the status.
fn why(status: u16, body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    v["message"]
        .as_str()
        .or(v["error_description"].as_str())
        .or(v["error"].as_str())
        .map_or_else(|| format!("status {status}"), str::to_string)
}

/// Trade `access_token` (issued to `client_id` moments ago) for an issuer
/// session.
pub async fn session_for(
    fetch: &impl Fetch,
    issuer: &str,
    client_id: &str,
    access_token: &str,
) -> Result<String, Refused> {
    let body = format!(
        "grant_type={}&client_id={}&subject_token={}&subject_token_type={}&requested_token_type={}",
        form_encode(TOKEN_EXCHANGE),
        form_encode(client_id),
        form_encode(access_token),
        form_encode(ACCESS_TOKEN),
        form_encode(SESSION_TOKEN),
    );
    let (status, text) = fetch
        .send(Request {
            method: "POST",
            url: format!("{}/oauth2/token", base(issuer)),
            headers: vec![(
                "content-type",
                "application/x-www-form-urlencoded".to_string(),
            )],
            body,
        })
        .await
        .ok_or_else(|| Refused::Issuer("unreachable".into()))?;
    match status {
        200 => {
            let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            v["access_token"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .ok_or_else(|| Refused::Issuer("it gave no session".into()))
        }
        403 => Err(Refused::NotAllowed),
        400 | 401 => Err(Refused::SignIn),
        _ => Err(Refused::Issuer(why(status, &text))),
    }
}

/// Make `draft` at `issuer` as the account of `session`, its owner.
pub async fn create(
    fetch: &impl Fetch,
    issuer: &str,
    session: &str,
    draft: &Draft,
) -> Result<Created, Refused> {
    let body = serde_json::json!({
        "organization": {
            "name": draft.name,
            "slug": draft.slug,
            "logo": null,
            "metadata_json": null,
        }
    });
    let (status, text) = fetch
        .send(Request {
            method: "POST",
            url: format!("{}/auth/organization/create", base(issuer)),
            headers: vec![
                ("content-type", "application/json".to_string()),
                ("authorization", format!("Bearer {session}")),
            ],
            body: body.to_string(),
        })
        .await
        .ok_or_else(|| Refused::Issuer("unreachable".into()))?;
    match status {
        200 => {
            let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let org = &v["organization"];
            match (org["slug"].as_str(), v["membership"]["role"].as_str()) {
                (Some(slug), Some(role)) => Ok(Created {
                    org: slug.to_string(),
                    name: org["name"].as_str().unwrap_or(&draft.name).to_string(),
                    role: role.to_string(),
                }),
                _ => Err(Refused::Issuer("its answer named no organization".into())),
            }
        }
        401 => Err(Refused::SignIn),
        _ => {
            let message = why(status, &text);
            if message.contains("slug already exists") {
                Err(Refused::Taken(draft.slug.clone()))
            } else {
                Err(Refused::Issuer(message))
            }
        }
    }
}

#[cfg(feature = "drive")]
pub use here::{create_here, joined};

/// The app's side: the issuer its node names, its own sign-in, and the
/// new org taken into the app.
#[cfg(feature = "drive")]
mod here {
    use dioxus::prelude::*;
    use drive_dioxus::auth;

    use super::{Created, Fetch, Refused, Request, create, draft, session_for};
    use crate::state::AppState;

    /// Drive's fetch: the browser's, or reqwest natively.
    struct DriveFetch;

    impl Fetch for DriveFetch {
        async fn send(&self, r: Request) -> Option<(u16, String)> {
            let headers: Vec<(&str, &str)> =
                r.headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let (status, v) = auth::fetch(r.method, &r.url, &headers, Some(&r.body)).await?;
            Some((status, v.to_string()))
        }
    }

    /// Make the org `name` (its slug from `slug`, else the name) at the
    /// issuer, as the account signed in here, its owner.
    pub async fn create_here(name: &str, slug: Option<&str>) -> Result<Created, Refused> {
        let d = draft(name, slug)?;
        let bearer = auth::bearer();
        let headers: Vec<(&str, &str)> = bearer
            .iter()
            .map(|b| ("authorization", b.as_str()))
            .collect();
        let orgs = match auth::fetch("GET", "/orgs", &headers, None).await {
            Some((200, v)) => v,
            Some((401 | 403, _)) => return Err(Refused::SignIn),
            Some((status, _)) => return Err(Refused::Issuer(format!("Drive answered {status}"))),
            None => return Err(Refused::Issuer("Drive did not answer".into())),
        };
        let issuer: auth::Issuer = serde_json::from_value(orgs["auth"].clone())
            .map_err(|_| Refused::Issuer("this Drive names no account server".into()))?;
        let session = session_here(&issuer).await?;
        create(&DriveFetch, &issuer.issuer, &session, &d).await
    }

    /// An issuer session for the account signed in here. Only a token
    /// issued moments ago is traded, so the one held is tried first and,
    /// refused, refreshed once and tried again: not refreshed on every
    /// Create (Drive signs out of every app here when a refresh fails).
    async fn session_here(issuer: &auth::Issuer) -> Result<String, Refused> {
        let trade = |token: String| async move {
            session_for(&DriveFetch, &issuer.issuer, &issuer.client_id, &token).await
        };
        let held = auth::token().ok_or(Refused::SignIn)?;
        match trade(held).await {
            Err(Refused::SignIn) => {
                let fresh = auth::fresh_token(f64::INFINITY)
                    .await
                    .ok_or(Refused::SignIn)?;
                trade(fresh).await
            }
            other => other,
        }
    }

    /// The new org, one of this person's now: listed (the switcher, All
    /// organizations), its name kept in its team document, and Studio in
    /// it. On the UI thread, inside the app (it opens a document).
    pub fn joined(app: AppState, made: &Created) {
        let mut orgs = app.orgs;
        let mut list = orgs.peek().clone();
        list.push(made.org.clone());
        orgs.set(crate::orgs::ordered(list));
        crate::orgs::remember_name(&made.org, &made.name);
        let name = made.name.clone();
        crate::songs::backing::create(
            &crate::orgs::qualified(&made.org, crate::orgs::ORG_DOC),
            move |d| {
                crate::orgs::stamp_org(d, &made.org);
                let mut st = crate::orgs::settings(d);
                st.name = name;
                let _ = crate::orgs::put_settings(d, &st);
            },
        );
        app.switch_org(&made.org);
    }
}

#[cfg(test)]
mod tests;

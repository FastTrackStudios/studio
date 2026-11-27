//! An organization's people, as the issuer (FastTrackStudios/auth) keeps
//! them: who is in it, inviting someone (by email or by a link), someone
//! joining from an invitation, and an owner removing a member.
//!
//! The issuer's member list is the truth: who is in an org is its
//! accounts, by id, never matched by email. Studio's team document
//! (`studio:org`) says what each person plays; whether they are in the
//! org at all is the issuer's to say.
//!
//! Like [`crate::org_create`], it is the app's own sign-in that asks: the
//! issuer takes a first-party app's access token (Studio holds Drive's
//! `drive` client's) at exactly these calls
//! (`r[auth.jwt.first-party-access]`, FastTrackStudios/auth#5), each held
//! to its own check: inviting and removing need an owner or an admin, the
//! list needs a member, joining needs the invitation or link.
//!
//! An invitation is a link to the issuer's own page: `/invite/<id>?token=`
//! for one sent to an address, `/join?token=` for one anyone may follow.
//! Whoever opens it signs in with any account there and is a member; or
//! pastes it into Studio, which joins with its own sign-in ([`join`]).

use crate::org_create::{Fetch, Request};

/// An org of the person's at the issuer: its id there (what the member
/// calls take), its slug (Studio's id for it) and the person's role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Org {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub role: String,
}

impl Org {
    /// Whether the person may invite and remove here (the issuer's
    /// `invitation:create` and `member:delete`: an owner or an admin).
    pub fn manages(&self) -> bool {
        manages(&self.role)
    }
}

/// Whether a role at the issuer invites and removes people.
pub fn manages(role: &str) -> bool {
    has_role(role, "owner") || has_role(role, "admin")
}

/// Whether an issuer role (one, or several joined by `,`) includes `which`.
pub fn has_role(role: &str, which: &str) -> bool {
    role.split(',').map(str::trim).any(|r| r == which)
}

/// Someone in an org, as the issuer lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// Their account at the issuer (its id).
    pub account: String,
    pub name: Option<String>,
    pub email: Option<String>,
    /// `owner`, `admin` or `member`.
    pub role: String,
}

impl Member {
    /// What to call them: their name, else their address, else "Someone".
    pub fn label(&self) -> String {
        self.name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| self.email.clone())
            .unwrap_or_else(|| "Someone".into())
    }
}

/// An invitation made: the link to send, and who it is for (`None`: a
/// link anyone who has it may follow).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invited {
    pub link: String,
    pub email: Option<String>,
}

/// An invitation as pasted: its link's token, and for one sent to an
/// address, the invitation's id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// `/join?token=…`: an invite link.
    Join { token: String },
    /// `/invite/<id>?token=…`: an invitation sent to an address.
    Invitation { id: String, token: String },
}

/// Why it did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Not signed in (or the issuer no longer takes the sign-in).
    SignIn,
    /// Only an owner or an admin invites or removes people.
    NotAllowed,
    /// This person is not in an org of that name.
    NotIn(String),
    /// Not an invitation link.
    NotALink,
    /// The link is used up, revoked, expired or made up.
    LinkGone,
    /// An org must keep an owner: the last one is not removed.
    LastOwner,
    /// Already in the org this invitation is for.
    AlreadyIn,
    /// The account to remove is not in the org (any more).
    NotAMember,
    /// The issuer could not be reached, or said something else.
    Issuer(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::SignIn => write!(f, "sign in first"),
            Refused::NotAllowed => write!(f, "only an owner or an admin can do that"),
            Refused::NotIn(org) => write!(f, "you are not in {org}"),
            Refused::NotALink => write!(f, "that is not an invitation link"),
            Refused::LinkGone => write!(f, "that invitation no longer works"),
            Refused::LastOwner => write!(f, "an organization keeps at least one owner"),
            Refused::AlreadyIn => write!(f, "you are already in that organization"),
            Refused::NotAMember => write!(f, "they are not in it any more"),
            Refused::Issuer(why) => write!(f, "the account server: {why}"),
        }
    }
}

fn base(issuer: &str) -> &str {
    issuer.trim_end_matches('/')
}

/// The issuer's words for a refusal (its error body's `message`).
fn why(status: u16, v: &serde_json::Value) -> String {
    v["message"]
        .as_str()
        .or(v["error"].as_str())
        .map_or_else(|| format!("status {status}"), str::to_string)
}

/// `POST /auth/organization/<path>` with `body`, the token its bearer.
/// A 2xx answer's JSON, or why not: 401 is [`Refused::SignIn`], 403
/// [`Refused::NotAllowed`].
async fn call(
    fetch: &impl Fetch,
    issuer: &str,
    token: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, Refused> {
    let (status, text) = fetch
        .send(Request {
            method: "POST",
            url: format!("{}/auth/organization/{path}", base(issuer)),
            headers: vec![
                ("content-type", "application/json".to_string()),
                ("authorization", format!("Bearer {token}")),
            ],
            body: body.to_string(),
        })
        .await
        .ok_or_else(|| Refused::Issuer("unreachable".into()))?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    match status {
        200..=299 => Ok(v),
        401 => Err(Refused::SignIn),
        403 => Err(Refused::NotAllowed),
        _ => {
            let message = why(status, &v);
            if message.contains("last organization owner") {
                Err(Refused::LastOwner)
            } else if message.contains("member already exists") {
                Err(Refused::AlreadyIn)
            } else {
                Err(Refused::Issuer(message))
            }
        }
    }
}

/// Every org the person is in at the issuer.
pub async fn orgs(fetch: &impl Fetch, issuer: &str, token: &str) -> Result<Vec<Org>, Refused> {
    let v = call(fetch, issuer, token, "list", serde_json::json!({})).await?;
    Ok(v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|b| {
                    Some(Org {
                        id: b["organization"]["id"].as_str()?.to_string(),
                        slug: b["organization"]["slug"].as_str()?.to_string(),
                        name: b["organization"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        role: b["membership"]["role"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// The org `slug` among the person's.
pub async fn org(
    fetch: &impl Fetch,
    issuer: &str,
    token: &str,
    slug: &str,
) -> Result<Org, Refused> {
    orgs(fetch, issuer, token)
        .await?
        .into_iter()
        .find(|o| o.slug == slug)
        .ok_or_else(|| Refused::NotIn(slug.to_string()))
}

/// Who is in `org`, from the issuer: any member may ask.
pub async fn members(
    fetch: &impl Fetch,
    issuer: &str,
    token: &str,
    org: &Org,
) -> Result<Vec<Member>, Refused> {
    let v = call(
        fetch,
        issuer,
        token,
        "members",
        serde_json::json!({ "organization_id": org.id }),
    )
    .await?;
    let mut out: Vec<Member> = v
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(Member {
                        account: m["user"]["id"].as_str()?.to_string(),
                        name: m["user"]["name"].as_str().map(str::to_string),
                        email: m["user"]["email"].as_str().map(str::to_string),
                        role: m["member"]["role"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // Owners first, then admins, then everyone by name.
    let rank = |r: &str| match r {
        "owner" => 0,
        "admin" => 1,
        _ => 2,
    };
    out.sort_by_key(|m| (rank(&m.role), m.label().to_lowercase()));
    Ok(out)
}

/// Invite `email` into `org` as a member: the invitation's link, to send
/// them. They accept with any account (an owner or an admin invites).
pub async fn invite_email(
    fetch: &impl Fetch,
    issuer: &str,
    token: &str,
    org: &Org,
    email: &str,
) -> Result<Invited, Refused> {
    let email = email.trim();
    let v = call(
        fetch,
        issuer,
        token,
        "invite-member",
        serde_json::json!({ "invite": {
            "organization_id": org.id,
            "email": email,
            "role": "member",
            "expires_at": null,
        }}),
    )
    .await?;
    let (Some(id), Some(secret)) = (v["invitation"]["id"].as_str(), v["token"].as_str()) else {
        return Err(Refused::Issuer("its answer held no invitation".into()));
    };
    Ok(Invited {
        link: format!("{}/invite/{id}?token={secret}", base(issuer)),
        email: Some(email.to_string()),
    })
}

/// A link into `org` that anyone who has it may follow, as a member, with
/// any account (an owner or an admin makes one).
pub async fn invite_link(
    fetch: &impl Fetch,
    issuer: &str,
    token: &str,
    org: &Org,
) -> Result<Invited, Refused> {
    let v = call(
        fetch,
        issuer,
        token,
        "invite-link",
        serde_json::json!({ "link": {
            "organization_id": org.id,
            "role": "member",
            "label": "Studio",
            "expires_at": null,
            "max_uses": null,
        }}),
    )
    .await?;
    let secret = v["token"]
        .as_str()
        .ok_or_else(|| Refused::Issuer("its answer held no link".into()))?;
    Ok(Invited {
        link: format!("{}/join?token={secret}", base(issuer)),
        email: None,
    })
}

/// Take the account `account` out of `org`: its access to it ends (an
/// owner or an admin removes; the last owner stays). A 401 here may be
/// the account's not being a member as well as the sign-in (the issuer
/// answers both as bad credentials).
pub async fn remove(
    fetch: &impl Fetch,
    issuer: &str,
    token: &str,
    org: &Org,
    account: &str,
) -> Result<(), Refused> {
    call(
        fetch,
        issuer,
        token,
        "remove-member",
        serde_json::json!({ "organization_id": org.id, "user_id": account }),
    )
    .await
    .map(|_| ())
}

/// The invitation in what was pasted: a link to the issuer's invitation
/// or join page (any host: the issuer's own), or nothing.
pub fn link_of(text: &str) -> Option<Link> {
    let text = text.trim();
    let (path, query) = text.split_once('?')?;
    let token = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("token="))
        .filter(|t| !t.is_empty())?
        .to_string();
    let path = path.trim_end_matches('/');
    if path.ends_with("/join") {
        return Some(Link::Join { token });
    }
    let (rest, id) = path.rsplit_once('/')?;
    (rest.ends_with("/invite") && !id.is_empty()).then(|| Link::Invitation {
        id: id.to_string(),
        token,
    })
}

/// Join the org `link` invites into, as the account `token` signs in:
/// that org, with the role given. A 401 here may be the link's as well
/// as the sign-in's (the issuer answers both as bad credentials).
pub async fn join(
    fetch: &impl Fetch,
    issuer: &str,
    token: &str,
    link: &Link,
) -> Result<Org, Refused> {
    match link {
        Link::Join { token: secret } => {
            let v = call(
                fetch,
                issuer,
                token,
                "join",
                serde_json::json!({ "link_token": secret }),
            )
            .await?;
            let id = v["organization_id"]
                .as_str()
                .ok_or_else(|| Refused::Issuer("its answer named no org".into()))?;
            orgs(fetch, issuer, token)
                .await?
                .into_iter()
                .find(|o| o.id == id)
                .ok_or_else(|| Refused::Issuer("the org joined is not listed".into()))
        }
        Link::Invitation { id, token: secret } => {
            // Accepting answers nothing: the org joined is the one listed
            // after that was not before.
            let before = orgs(fetch, issuer, token).await?;
            call(
                fetch,
                issuer,
                token,
                "accept-invitation",
                serde_json::json!({ "invitation_id": id, "invitation_token": secret }),
            )
            .await
            .map_err(|e| match e {
                // Accepted already, or no longer pending.
                Refused::Issuer(_) => Refused::LinkGone,
                other => other,
            })?;
            orgs(fetch, issuer, token)
                .await?
                .into_iter()
                .find(|o| !before.iter().any(|b| b.id == o.id))
                .ok_or_else(|| Refused::Issuer("the org joined is not listed".into()))
        }
    }
}

#[cfg(feature = "drive")]
pub use here::{invite_here, join_here, joined, members_here, remove_here};

/// The app's side: the issuer its Drive names, and its own sign-in,
/// refreshed once when the issuer no longer takes it.
#[cfg(feature = "drive")]
mod here {
    use dioxus::prelude::*;
    use drive_dioxus::auth;

    use super::{Invited, Link, Member, Org, Refused};
    use crate::org_create::{Fetch, Request};
    use crate::state::AppState;

    struct DriveFetch;

    impl Fetch for DriveFetch {
        async fn send(&self, r: Request) -> Option<(u16, String)> {
            let headers: Vec<(&str, &str)> =
                r.headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let (status, v) = auth::fetch(r.method, &r.url, &headers, Some(&r.body)).await?;
            Some((status, v.to_string()))
        }
    }

    /// The account server this app's Drive names.
    async fn issuer() -> Result<String, Refused> {
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
        Ok(issuer.issuer)
    }

    /// `f` with the issuer and the token held; refused as a sign-in, once
    /// more with a fresh token (not on every call: Drive signs out of
    /// every app here when a refresh fails).
    async fn signed_in<T>(f: impl AsyncFn(&str, &str) -> Result<T, Refused>) -> Result<T, Refused> {
        let issuer = issuer().await?;
        let held = auth::token().ok_or(Refused::SignIn)?;
        match f(&issuer, &held).await {
            Err(Refused::SignIn) => {
                let fresh = auth::fresh_token(f64::INFINITY)
                    .await
                    .ok_or(Refused::SignIn)?;
                f(&issuer, &fresh).await
            }
            other => other,
        }
    }

    /// `slug`'s people, and the person's own standing there.
    pub async fn members_here(slug: &str) -> Result<(Org, Vec<Member>), Refused> {
        signed_in(async |issuer: &str, token: &str| {
            let org = super::org(&DriveFetch, issuer, token, slug).await?;
            let people = super::members(&DriveFetch, issuer, token, &org).await?;
            Ok((org, people))
        })
        .await
    }

    /// Invite into `slug`: `email`, or by a link anyone may follow.
    pub async fn invite_here(slug: &str, email: Option<&str>) -> Result<Invited, Refused> {
        signed_in(async |issuer: &str, token: &str| {
            let org = super::org(&DriveFetch, issuer, token, slug).await?;
            match email.map(str::trim).filter(|e| !e.is_empty()) {
                Some(email) => super::invite_email(&DriveFetch, issuer, token, &org, email).await,
                None => super::invite_link(&DriveFetch, issuer, token, &org).await,
            }
        })
        .await
    }

    /// Take the account `account` out of `slug`.
    pub async fn remove_here(slug: &str, account: &str) -> Result<(), Refused> {
        // A refusal a fresh sign-in did not cure is the account's not
        // being in it.
        match signed_in(async |issuer: &str, token: &str| {
            let org = super::org(&DriveFetch, issuer, token, slug).await?;
            super::remove(&DriveFetch, issuer, token, &org, account).await
        })
        .await
        {
            Err(Refused::SignIn) if auth::token().is_some() => Err(Refused::NotAMember),
            other => other,
        }
    }

    /// An org joined, one of this person's now: listed (the switcher, All
    /// organizations), named, and Studio in it. On the UI thread.
    pub fn joined(app: AppState, org: &Org) {
        let mut orgs = app.orgs;
        let mut list = orgs.peek().clone();
        if !list.contains(&org.slug) {
            list.push(org.slug.clone());
            orgs.set(crate::orgs::ordered(list));
        }
        if !org.name.is_empty() {
            crate::orgs::remember_name(&org.slug, &org.name);
        }
        app.switch_org(&org.slug);
    }

    /// Join from a pasted invitation; a refusal a fresh sign-in did not
    /// cure is the link's.
    pub async fn join_here(pasted: &str) -> Result<Org, Refused> {
        let link: Link = super::link_of(pasted).ok_or(Refused::NotALink)?;
        match signed_in(async |issuer: &str, token: &str| {
            super::join(&DriveFetch, issuer, token, &link).await
        })
        .await
        {
            Err(Refused::SignIn) if auth::token().is_some() => Err(Refused::LinkGone),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests;

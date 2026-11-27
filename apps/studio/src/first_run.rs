//! A first run (studio#699): a fresh install, with no config, gets from
//! signing in to an open setlist with no setup screens.
//!
//! - **The node.** With nothing naming a Drive node (no `DRIVE_NODE`, no
//!   Drive agent naming one, no node on this computer), Studio uses
//!   FastTrack's own ([`DEFAULT_NODE`]), which signs in through the
//!   account server ([`default_node`]). Natively, Drive's sign-in opens
//!   the account server in the system's browser and comes back on a
//!   loopback port (drive#206).
//! - **The organizations.** Signed in, the orgs Studio lists are the
//!   node's answer for that account, none included ([`orgs_listed`]):
//!   never the made-up ones a build without Drive shows.
//! - **The setlists.** Home lists each org's setlists, each one tap from
//!   open ([`offers`]); with no org, it offers to create one, and with
//!   no setlist, says so ([`step`]).
//!
//! `STUDIO_ACCOUNT`, `STUDIO_SIGN_IN` and `STUDIO_FIRST_RUN` say and do
//! the same over the Action List (`crate::actions`), which is how the
//! Seam 1 test (`tests/first_run.rs`) drives a fresh install end to end.

// The node and the Action List's answers are the native build's; without
// Drive (the in-memory build) only the tests read the rest.
#![cfg_attr(not(feature = "native"), allow(dead_code))]

use serde_json::{Value, json};

use crate::Route;
use crate::files::Hit;

/// The node a fresh install reaches: FastTrack's Drive, at its root (so a
/// native sign-in comes back to `http://127.0.0.1:<port>/auth/callback`,
/// a loopback redirect the `drive` client already lists).
pub const DEFAULT_NODE: &str = "wss://drive.fasttrackstudio.app/sync";

/// Where a node on this computer answers, unless something names another
/// (`drive-client`'s own default).
pub const LOCAL_NODE: (std::net::Ipv4Addr, u16) = (std::net::Ipv4Addr::LOCALHOST, 8096);

/// What already names a node when Studio starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Named {
    /// `DRIVE_NODE` is set (and not empty).
    pub env: bool,
    /// The Drive agent's config names the node it uses.
    pub agent: bool,
    /// A node answers on this computer's default port.
    pub local: bool,
}

/// The node Studio names when nothing else does: [`DEFAULT_NODE`]. `None`:
/// leave it to Drive's client (the variable, the agent's node, this
/// computer's).
pub fn default_node(named: Named) -> Option<&'static str> {
    (!named.env && !named.agent && !named.local).then_some(DEFAULT_NODE)
}

/// Whether the Drive agent's `config.json` names a node (as Drive's
/// client reads it when `DRIVE_NODE` is unset).
pub fn agent_names_node(config: Option<&str>) -> bool {
    config
        .and_then(|c| serde_json::from_str::<Value>(c).ok())
        .and_then(|c| c.get("node")?.as_str().map(str::to_owned))
        .is_some_and(|n| !n.trim().is_empty())
}

/// Natively, at start (before any thread): the node to name, from the
/// environment, the agent's config and a quick look at this computer's
/// port. What `main` sets `DRIVE_NODE` to.
#[cfg(feature = "native")]
pub fn node_to_name() -> Option<&'static str> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let agent_dir = var("DRIVE_AGENT_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            var("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    var("HOME").map(|h| std::path::PathBuf::from(h).join(".local").join("share"))
                })
                .map(|d| d.join("drive-agent"))
        });
    let config = agent_dir.and_then(|d| std::fs::read_to_string(d.join("config.json")).ok());
    let local = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(LOCAL_NODE),
        std::time::Duration::from_millis(150),
    )
    .is_ok();
    default_node(Named {
        env: var("DRIVE_NODE").is_some(),
        agent: agent_names_node(config.as_deref()),
        local,
    })
}

// ── the account ─────────────────────────────────────────────────────────

/// The node's `/orgs`, as the first run reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    /// The node wants a sign-in and has none (`signed_in: false`).
    pub signed_out: bool,
    /// Who is signed in, when the node says.
    pub who: String,
    /// The orgs the node lists for this account (or for this computer).
    pub orgs: Vec<String>,
    /// The account server the node signs in through, when it has one.
    pub issuer: Option<String>,
}

impl Account {
    /// The node's answer to `/orgs`.
    pub fn read(v: &Value) -> Self {
        Self {
            signed_out: v["signed_in"] == Value::Bool(false),
            who: v["who"].as_str().unwrap_or_default().to_string(),
            orgs: v["orgs"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|o| o.as_str().filter(|s| !s.is_empty()).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            issuer: v["auth"]["issuer"].as_str().map(str::to_string),
        }
    }

    /// Signed in through an account server (not a node that lets anyone in).
    pub fn signed_in(&self) -> bool {
        !self.signed_out && self.issuer.is_some()
    }
}

/// The orgs Studio lists once the node answered: an account's are the
/// node's, even none (a new account is in no org yet); a node that lets
/// anyone in lists its own, keeping `had` when it lists none.
pub fn orgs_listed(answer: &Account, had: &[String]) -> Vec<String> {
    if answer.signed_in() || !answer.orgs.is_empty() {
        crate::orgs::ordered(answer.orgs.clone())
    } else {
        had.to_vec()
    }
}

// ── where the first run is ──────────────────────────────────────────────

/// Where a person is on their way from signing in to an open setlist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The node wants a sign-in.
    SignIn,
    /// Signed in, in no organization: create one, or open an invitation.
    NoOrganization,
    /// In an organization that has no setlist yet.
    NoSetlist,
    /// A setlist to open, one tap away.
    Ready,
}

impl Step {
    pub fn slug(self) -> &'static str {
        match self {
            Step::SignIn => "sign-in",
            Step::NoOrganization => "no-organization",
            Step::NoSetlist => "no-setlist",
            Step::Ready => "ready",
        }
    }
}

/// The step from what the app knows: signed out, the orgs listed, and
/// how many setlists they have between them.
pub fn step(signed_out: bool, orgs: &[String], setlists: usize) -> Step {
    if signed_out {
        Step::SignIn
    } else if orgs.is_empty() {
        Step::NoOrganization
    } else if setlists == 0 {
        Step::NoSetlist
    } else {
        Step::Ready
    }
}

/// A setlist the first run offers: its org, its title, and where it opens.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub org: String,
    pub title: String,
    pub open: Route,
}

/// One org's setlists, as Home lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct OrgSetlists {
    pub org: String,
    pub setlists: Vec<Offer>,
}

/// Each org's setlists, in the orgs' order: the sets of the org Studio is
/// in (`here`'s index: `(id, title)`) and the setlist files Drive and
/// this computer have, each in the org it belongs to. A file in an org
/// not listed is left out (it is not this person's).
pub fn offers(
    orgs: &[String],
    here: &str,
    sets: &[(String, String)],
    files: &[Hit],
) -> Vec<OrgSetlists> {
    orgs.iter()
        .map(|org| {
            let mut setlists: Vec<Offer> = Vec::new();
            if org == here {
                setlists.extend(sets.iter().map(|(id, title)| Offer {
                    org: org.clone(),
                    title: title.clone(),
                    open: Route::OrgSet {
                        owner: org.clone(),
                        id: id.clone(),
                    },
                }));
            }
            for h in files.iter().filter(|h| &h.space == org) {
                let title = crate::setfile::name_of(&h.name);
                if setlists.iter().any(|o| o.title == title) {
                    continue;
                }
                setlists.push(Offer {
                    org: org.clone(),
                    title,
                    open: Route::DriveSet {
                        space: h.space.clone(),
                        location: h.location.clone(),
                        path: h.path.split('/').map(str::to_string).collect(),
                    },
                });
            }
            OrgSetlists {
                org: org.clone(),
                setlists,
            }
        })
        .collect()
}

/// What `STUDIO_FIRST_RUN` answers: the step, the org Studio is in, and
/// each org's setlists with the route each opens at.
pub fn answer(step: Step, here: &str, listed: &[OrgSetlists]) -> Value {
    json!({
        "step": step.slug(),
        "here": here,
        "orgs": listed.iter().map(|o| json!({
            "org": o.org,
            "name": crate::orgs::org_name(&o.org),
            "setlists": o.setlists.iter().map(|s| json!({
                "title": s.title,
                "route": s.open.to_string(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests;

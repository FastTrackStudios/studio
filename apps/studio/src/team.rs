//! The org documents, live: an org's team (`studio:org`), what other orgs
//! share with it, and the mirror that keeps what it shares current. The
//! shapes and rules are `orgs`; this is where they meet Drive.

use dioxus::prelude::*;
use loro::LoroDoc;

use crate::orgs::{self, Group, Person, Settings};
use crate::songs::{self, Entry, backing};
use crate::state::use_app;

/// An org's team, live.
#[derive(Clone, PartialEq)]
pub struct Team {
    b: backing::Backing,
    pub org: String,
}

pub fn use_team_of(org: &str) -> Team {
    let b = backing::use_backing(orgs::qualified(org, orgs::ORG_DOC));
    songs::use_upgrade(&b, org, orgs::ORG_STEPS);
    Team {
        b,
        org: org.to_string(),
    }
}

/// The team of the org Studio is in.
pub fn use_team() -> Team {
    let org = (use_app().here)();
    use_team_of(&org)
}

impl Team {
    pub fn loro(&self) -> LoroDoc {
        self.b.loro()
    }
    pub fn people(&self) -> Vec<Person> {
        orgs::people(&self.b.loro())
    }
    pub fn groups(&self) -> Vec<Group> {
        orgs::groups(&self.b.loro())
    }
    pub fn settings(&self) -> Settings {
        orgs::settings(&self.b.loro())
    }
    pub fn person(&self, id: &str) -> Option<Person> {
        orgs::person(&self.b.loro(), id)
    }
    /// The person the signed-in account is here.
    pub fn me(&self, account: &str) -> Option<Person> {
        orgs::person_for(&self.b.loro(), account)
    }
    pub fn put_person(&self, p: &Person) {
        self.b.edit(|d| {
            let _ = orgs::put_person(d, p);
        });
    }
    /// A new person, by name; their id.
    pub fn add_person(&self, name: &str) -> String {
        self.b.edit(|d| {
            let p = Person {
                id: orgs::person_id(d, name),
                name: name.to_string(),
                hue: crate::look::str_hue(name),
                ..Person::default()
            };
            let _ = orgs::put_person(d, &p);
            p.id
        })
    }
    pub fn remove_person(&self, id: &str) {
        self.b.edit(|d| {
            let _ = orgs::remove_person(d, id);
        });
    }
    pub fn link_account(&self, id: &str, account: &str) {
        self.b.edit(|d| {
            let _ = orgs::link_account(d, id, account);
        });
    }
    pub fn put_group(&self, g: &Group) {
        self.b.edit(|d| {
            let _ = orgs::put_group(d, g);
        });
    }
    pub fn put_settings(&self, st: &Settings) {
        self.b.edit(|d| {
            let _ = orgs::put_settings(d, st);
        });
    }
    /// The name Studio shows for the org: its setting, else the known one.
    pub fn name(&self) -> String {
        let st = self.settings();
        if st.name.is_empty() {
            orgs::org_name(&self.org)
        } else {
            st.name
        }
    }
}

// ── what other orgs share with this one ────────────────────────────────

/// The orgs sharing Studio songs with `to`: on the web Drive's projects
/// shared with it (`/shared`), else every other org Studio knows.
pub fn use_owners_sharing(to: &str) -> Signal<Vec<String>> {
    let app = use_app();
    let mut owners = use_signal(Vec::<String>::new);
    let to = to.to_string();
    use_future(use_reactive!(|to| async move {
        #[cfg(feature = "drive")]
        {
            let _ = app;
            let bearer = drive_dioxus::auth::bearer();
            let headers: Vec<(&str, &str)> = bearer
                .iter()
                .map(|b| ("authorization", b.as_str()))
                .collect();
            let url = format!("/shared?org={}", drive_dioxus::env::encode_component(&to));
            if let Some((200, v)) = drive_dioxus::auth::fetch("GET", &url, &headers, None).await {
                let pairs: Vec<(String, String)> = v["shared"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|x| {
                                (
                                    x["org"].as_str().unwrap_or_default().to_string(),
                                    x["project"]["id"].as_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                owners.set(orgs::owners_sharing(&to, &pairs));
            }
        }
        #[cfg(not(feature = "drive"))]
        {
            let all = app.orgs.peek().clone();
            owners.set(all.into_iter().filter(|o| *o != to).collect());
        }
    }));
    owners
}

/// What `owner` shares with `to`, live: the songs' faces.
#[derive(Clone, PartialEq)]
pub struct Digest(backing::Backing);

pub fn use_digest(owner: &str, to: &str) -> Digest {
    Digest(backing::use_backing(orgs::qualified(
        owner,
        &orgs::digest_doc(to),
    )))
}

impl Digest {
    pub fn entries(&self) -> Vec<Entry> {
        songs::entries(&self.0.loro())
    }
    pub fn get(&self, id: &str) -> Option<Entry> {
        self.entries().into_iter().find(|e| e.id == id)
    }
}

// ── the mirror ──────────────────────────────────────────────────────────

/// Keeps what the org Studio is in shares current: for every org it has
/// shared with, the digest and Drive's project follow the library.
#[component]
pub fn ShareMirror() -> Element {
    let org = (use_app().here)();
    rsx! {
        Mirror { key: "{org}", org }
    }
}

#[component]
fn Mirror(org: String) -> Element {
    let lib = songs::use_library_of(&org);
    let targets = orgs::share_targets(&lib.loro());
    rsx! {
        for to in targets {
            MirrorTo { key: "{to}", lib: lib.clone(), to }
        }
        MirrorFiles { lib: lib.clone() }
    }
}

/// Each song's files project follows the song: its title, and the orgs it
/// is shared with (sharing a song shares its files).
#[component]
fn MirrorFiles(lib: songs::Library) -> Element {
    let registry = backing::use_backing(drive_files::space_doc(&lib.org));
    let ready = registry.ready();
    let rev = lib.loro().oplog_vv();
    let reg_rev = registry.loro().oplog_vv();
    use_effect(use_reactive!(|ready, rev, reg_rev| {
        let _ = (rev, reg_rev);
        if !(ready && lib.ready()) {
            return;
        }
        let entries = lib.entries();
        if crate::files::projects_differ(&registry.peek(), &entries) {
            registry.edit(|r| crate::files::mirror_projects(r, &entries));
        }
    }));
    rsx! {}
}

#[component]
fn MirrorTo(lib: songs::Library, to: String) -> Element {
    let digest = backing::use_backing(orgs::qualified(&lib.org, &orgs::digest_doc(&to)));
    let registry = backing::use_backing(drive_files::space_doc(&lib.org));
    let (ready_d, ready_r) = (digest.ready(), registry.ready());
    // Re-run whenever the library changes.
    let rev = lib.loro().oplog_vv();
    use_effect(use_reactive!(|ready_d, ready_r, rev| {
        let _ = rev;
        if !(ready_d && ready_r && lib.ready()) {
            return;
        }
        let l = lib.peek();
        // Only an edit when something differs (each edit is sent).
        let (d, r) = (digest.peek(), registry.peek());
        if songs::entries(&d) != orgs::shared_with(&l, &to) {
            digest.edit(|d| orgs::mirror_digest(&l, &to, d));
        }
        if orgs::project_differs(&l, &to, &r) {
            registry.edit(|r| orgs::mirror_project(&l, &to, r));
        }
    }));
    rsx! {}
}

/// Your person in a team: the one your account is, or (no one signed in:
/// the desktop and phone builds, a node without an issuer) the
/// prototype's you.
pub fn me_id(team: &Team, who: &str) -> Option<String> {
    match team.me(who) {
        Some(p) => Some(p.id),
        None if who.is_empty() => Some("cody-wright".into()),
        None => None,
    }
}

/// Your person in the org Studio is in.
pub fn use_me() -> Option<String> {
    let app = use_app();
    let team = use_team();
    me_id(&team, &(app.who)())
}

//! The setlist documents, live: an org's index of sets (`studio:sets`)
//! and a set's own document (`studio:setlist:<id>`), and what keeps the
//! rest current from them while a set is open: its face in the index,
//! its date and rehearsals in Drive's calendar, and its sharing project.
//! The shapes and rules are `setlist`; this is where they meet Drive.

use dioxus::prelude::*;
use loro::LoroDoc;

use crate::orgs;
use crate::setlist::{self, Entry, Face, Info, Rehearsal, Seat, Series};
use crate::songs::backing;
use crate::state::use_app;

/// Where the app is served (the calendar's links lead back here).
pub const BASE: &str = "/studio";

/// An org's index of sets, live.
#[derive(Clone, PartialEq)]
pub struct Index {
    b: backing::Backing,
    pub org: String,
}

pub fn use_index_of(org: &str) -> Index {
    let b = backing::use_backing(orgs::qualified(org, setlist::INDEX_DOC));
    crate::songs::use_upgrade(&b, org, setlist::INDEX_STEPS);
    Index {
        b,
        org: org.to_string(),
    }
}

/// The index of the org Studio is in.
pub fn use_index() -> Index {
    let org = (use_app().here)();
    use_index_of(&org)
}

impl Index {
    pub fn faces(&self) -> Vec<Face> {
        setlist::faces(&self.b.loro())
    }
    pub fn face(&self, id: &str) -> Option<Face> {
        setlist::face(&self.b.loro(), id)
    }
    pub fn series(&self) -> Vec<Series> {
        setlist::series_list(&self.b.loro())
    }
    pub fn series_of(&self, id: &str) -> Option<Series> {
        self.series().into_iter().find(|s| s.id == id)
    }
    /// Put a face in (a set just made, before its document has synced).
    pub fn put_face(&self, f: &Face) {
        self.b.edit(|d| {
            setlist::put_face(d, f);
        });
    }
    /// Take a set out of the list (its setlist file was deleted by hand).
    pub fn remove(&self, id: &str, when: &str) {
        self.b.edit(|d| setlist::remove_set(d, id, when));
    }
    pub fn ready(&self) -> bool {
        self.b.ready()
    }
    pub fn templates(&self) -> Vec<Face> {
        self.faces()
            .into_iter()
            .filter(|f| f.kind == setlist::Kind::Template)
            .collect()
    }
}

/// A set, live.
#[derive(Clone, PartialEq)]
pub struct SetDoc {
    b: backing::Backing,
    /// The org that owns it.
    pub org: String,
    pub id: String,
}

/// A set's document, brought up to its version once its node has answered
/// (stamped with its org and id first: the step reads them), and the
/// prototype's live keys carried onto it once.
pub fn use_set(org: &str, id: &str) -> SetDoc {
    let b = backing::use_backing(orgs::qualified(org, &setlist::setlist_doc(id)));
    let legacy = backing::use_backing(orgs::qualified(org, &setlist::legacy_doc(id)));
    let ready = b.ready() && legacy.ready();
    {
        let (b, org, id) = (b.clone(), org.to_string(), id.to_string());
        use_effect(use_reactive!(|ready| {
            if !ready {
                return;
            }
            let d = b.peek();
            if drive_spine::schema::version(&d) < setlist::SETLIST_STEPS.len() as i64 {
                b.edit(|d| {
                    setlist::stamp_set(d, &org, &id);
                    drive_spine::schema::upgrade(d, setlist::SETLIST_STEPS)
                });
            }
            if setlist::builtin_home(&id) == Some(org.as_str())
                && !legacy.peek().get_map("songs").is_empty()
                && !matches!(
                    b.peek().get_map("meta").get("legacy"),
                    Some(loro::ValueOrContainer::Value(loro::LoroValue::Bool(true)))
                )
            {
                let old = legacy.peek();
                b.edit(|d| setlist::import_legacy(d, &old));
            }
        }));
    }
    SetDoc {
        b,
        org: org.to_string(),
        id: id.to_string(),
    }
}

impl SetDoc {
    pub fn loro(&self) -> LoroDoc {
        self.b.loro()
    }
    pub fn peek(&self) -> LoroDoc {
        self.b.peek()
    }
    pub fn ready(&self) -> bool {
        self.b.ready()
    }
    pub fn info(&self) -> Info {
        let mut i = setlist::info(&self.b.loro());
        if i.id.is_empty() {
            i.id = self.id.clone();
        }
        i
    }
    pub fn entries(&self) -> Vec<Entry> {
        setlist::entries(&self.b.loro())
    }
    pub fn entry(&self, id: &str) -> Option<Entry> {
        setlist::entry(&self.b.loro(), id)
    }
    pub fn songs(&self) -> Vec<Entry> {
        setlist::songs_of(&self.b.loro())
    }
    pub fn band(&self) -> Vec<Seat> {
        setlist::band(&self.b.loro())
    }
    pub fn rehearsals(&self) -> Vec<Rehearsal> {
        setlist::rehearsals(&self.b.loro())
    }
    pub fn shared_with(&self) -> Vec<String> {
        setlist::shared_with(&self.b.loro())
    }
    pub fn edit<R>(&self, f: impl FnOnce(&LoroDoc) -> R) -> R {
        self.b.edit(f)
    }
    pub fn put_info(&self, i: &Info) {
        self.edit(|d| {
            let _ = setlist::put_info(d, i);
        });
    }
    pub fn update(&self, e: &Entry) {
        self.edit(|d| {
            let _ = setlist::update_entry(d, e);
        });
    }
    pub fn insert(&self, at: Option<usize>, e: &Entry) -> String {
        self.edit(|d| setlist::insert_entry(d, at, e).unwrap_or_default())
    }
    pub fn remove(&self, id: &str) {
        self.edit(|d| {
            let _ = setlist::remove_entry(d, id);
        });
    }
    pub fn move_by(&self, id: &str, by: i32) {
        self.edit(|d| {
            let _ = setlist::move_by(d, id, by);
        });
    }
    pub fn move_before(&self, id: &str, before: Option<&str>) {
        self.edit(|d| {
            let _ = setlist::move_before(d, id, before);
        });
    }
    pub fn put_seat(&self, s: &Seat) {
        self.edit(|d| {
            let _ = setlist::put_seat(d, s);
        });
    }
    pub fn remove_seat(&self, person: &str) {
        self.edit(|d| {
            let _ = setlist::remove_seat(d, person);
        });
    }
    pub fn put_rehearsal(&self, r: &Rehearsal) {
        self.edit(|d| {
            let _ = setlist::put_rehearsal(d, r);
        });
    }
    pub fn share(&self, org: &str, on: bool) {
        self.edit(|d| {
            let _ = setlist::share_set(d, org, on);
        });
    }
}

/// Keeps what follows an open set current: its face in its org's index,
/// its date and rehearsals in the calendar of the org Studio is in, and
/// (in its own org) its sharing project. Mounted by a set's pages.
#[component]
pub fn SetMirror(set: SetDoc) -> Element {
    let app = use_app();
    let index = backing::use_backing(orgs::qualified(&set.org, setlist::INDEX_DOC));
    let registry = backing::use_backing(drive_files::space_doc(&set.org));
    let here = (app.here)();
    let ready = set.ready() && index.ready() && registry.ready();
    // Re-run whenever the set changes.
    let rev = set.loro().oplog_vv();
    use_effect(use_reactive!(|ready, rev, here| {
        let _ = rev;
        if !ready {
            return;
        }
        let d = set.peek();
        let i = setlist::info(&d);
        if i.id.is_empty() {
            return;
        }
        let f = setlist::face_of(&d);
        if setlist::face(&index.peek(), &f.id).as_ref() != Some(&f) {
            index.edit(|x| setlist::put_face(x, &f));
        }
        if here == set.org && setlist::set_project_differs(&d, &registry.peek()) {
            registry.edit(|r| setlist::mirror_set_project(&d, r));
        }
        let mut events: Vec<drive_spine::Event> =
            setlist::calendar_event(&i, BASE).into_iter().collect();
        if i.kind != setlist::Kind::Template {
            events.extend(
                setlist::rehearsals(&d)
                    .iter()
                    .map(|r| setlist::rehearsal_event(&i, r, BASE)),
            );
        }
        app.sync.put_events(&events);
    }));
    rsx! {}
}

/// Make a new set in an org: its document (from `from` when given: last
/// week's or a template) and its face. Returns its id.
pub fn create(org: &str, info: &Info, from: Option<&LoroDoc>, index: &Index) -> String {
    let doc = backing::create(
        &orgs::qualified(org, &setlist::setlist_doc(&info.id)),
        |d| {
            setlist::stamp_set(d, org, &info.id);
            // A new set is already at the current version: nothing to seed.
            let _ = d
                .get_map("meta")
                .insert("version", setlist::SETLIST_STEPS.len() as i64);
            match from {
                Some(src) => {
                    let _ = setlist::start_from(src, d, info);
                }
                None => {
                    let _ = setlist::put_info(d, info);
                }
            }
        },
    );
    index.put_face(&setlist::face_of(&doc));
    info.id.clone()
}

/// A new set's id: a meeting's is its event's and date; any other a fresh one.
pub fn new_id() -> String {
    format!("s{}", crate::songs::new_id())
}

// ── sets other orgs share with this one ─────────────────────────────────

/// The sets other orgs share with `to`: (owner, set id). On the web,
/// Drive's projects shared with it (`/shared`); without Drive, every
/// other org's index in memory.
pub fn use_sets_shared_with(to: &str) -> Signal<Vec<(String, String)>> {
    let app = use_app();
    let mut found = use_signal(Vec::<(String, String)>::new);
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
                found.set(setlist::shared_sets(&to, &pairs));
            }
        }
        #[cfg(not(feature = "drive"))]
        {
            let orgs = app.orgs.peek().clone();
            let mut out = Vec::new();
            for owner in orgs.iter().filter(|o| **o != to) {
                if let Some(index) = backing::peek(&orgs::qualified(owner, setlist::INDEX_DOC)) {
                    for f in setlist::faces(&index) {
                        if f.shared_with.contains(&to) {
                            out.push((owner.clone(), f.id));
                        }
                    }
                }
            }
            found.set(out);
        }
    }));
    found
}

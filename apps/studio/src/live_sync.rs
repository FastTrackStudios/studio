//! Live sets on Drive, the local-first sync: who is in a set and which
//! entry they have open, the chart following whoever pressed Play first,
//! and the calendar (the spine) the sets put their dates in. The set
//! itself is its document (`sets`); this is the room around it.
//!
//! Drive's client is the browser's, so this is the web build's; the
//! desktop and the phone keep the app's memory alone (the same API, doing
//! nothing).

/// Someone else in this set, and the song they have open, if any.
#[derive(Clone, PartialEq, Debug)]
pub struct Viewer {
    pub name: String,
    pub hue: u32,
    /// The entry (its id) they have open.
    pub row: Option<String>,
    /// `key`, `arr` or `entry`.
    pub edit: Option<String>,
}

#[cfg(feature = "drive")]
pub use web::*;

#[cfg(not(feature = "drive"))]
pub use local::*;

#[cfg(feature = "drive")]
mod web {
    use dioxus::prelude::*;
    use drive_dioxus::ui::{Avatars, ConnectionStatus};
    use drive_dioxus::{Doc, DriveCtx, DriveProvider, Presence, use_doc, use_drive};
    use drive_spine as spine;
    use serde_json::{Value, json};

    use super::*;
    use crate::Route;
    use crate::data;
    use crate::state::{Live, SetRef, Sheet, use_app};

    /// Drive, as the app reaches it: the spine, the open set's document,
    /// and whose transport the chart follows.
    #[derive(Clone, Copy)]
    pub struct Sync {
        drive: Signal<Option<DriveCtx>>,
        spine: Signal<Option<Doc>>,
        set: Signal<Option<Doc>>,
        following: Signal<Option<String>>,
    }

    impl Sync {
        pub fn new() -> Self {
            Self {
                drive: Signal::new(None),
                spine: Signal::new(None),
                set: Signal::new(None),
                following: Signal::new(None),
            }
        }

        /// Move every app on this address to another org (Drive reloads).
        pub fn switch_org(self, org: &str) -> bool {
            match self.drive.peek().as_ref() {
                Some(ctx) => {
                    ctx.set_org(org);
                    true
                }
                None => false,
            }
        }

        /// Everyone else in the open set, live.
        pub fn viewers(self) -> Vec<Viewer> {
            let Some(doc) = (self.set)() else {
                return Vec::new();
            };
            doc.others().iter().map(viewer).collect()
        }

        /// Whose transport the chart follows, when it is someone else's.
        pub fn following(self) -> Option<String> {
            (self.following)()
        }

        pub fn is_following(self) -> bool {
            self.following.peek().is_some()
        }

        /// Put events in the calendar of the org Studio is in (only those
        /// that differ: each edit is sent).
        pub fn put_events(self, events: &[spine::Event]) {
            let Some(doc) = self.spine.peek().clone() else {
                return;
            };
            let have = spine::events(&doc.peek());
            let want: Vec<&spine::Event> = events.iter().filter(|e| !have.contains(e)).collect();
            if want.is_empty() {
                return;
            }
            doc.edit(|d| {
                for e in want {
                    spine::put_event(d, e);
                }
            });
        }
    }

    /// Connects to Drive (the node this page came from) around the app.
    #[component]
    pub fn Provider(children: Element) -> Element {
        let app = rsx! {
            DriveProvider {
                Attach { {children} }
            }
        };
        // In a browser, a signed-out visitor gets the stand-in page.
        #[cfg(feature = "web")]
        return rsx! {
            crate::door::Gate { {app} }
        };
        #[cfg(not(feature = "web"))]
        app
    }

    /// Hands the connection and the spine to the app's state.
    #[component]
    fn Attach(children: Element) -> Element {
        let app = use_app();
        let ctx = use_drive();
        let spine_doc = use_doc(spine::SPINE);
        use_hook({
            let ctx = ctx.clone();
            move || {
                let (mut d, mut s) = (app.sync.drive, app.sync.spine);
                let (mut here, mut org, mut who) = (app.here, app.org, app.who);
                here.set(ctx.org.clone());
                org.set(Some(ctx.org.clone()));
                who.set(ctx.who.clone());
                d.set(Some(ctx));
                s.set(Some(spine_doc));
            }
        });
        // The orgs this person is in, as the node says.
        use_future(move || async move {
            let bearer = drive_dioxus::auth::bearer();
            let headers: Vec<(&str, &str)> = bearer
                .iter()
                .map(|b| ("authorization", b.as_str()))
                .collect();
            if let Some((200, v)) = drive_dioxus::auth::fetch("GET", "/orgs", &headers, None).await
            {
                let list: Vec<String> = v["orgs"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|o| o.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                if !list.is_empty() {
                    let mut orgs = app.orgs;
                    orgs.set(crate::orgs::ordered(list));
                }
            }
        });
        children
    }

    /// Who is here, in a set's bar.
    #[component]
    pub fn Here() -> Element {
        let app = use_app();
        let Some(doc) = (app.sync.set)() else {
            return rsx! {};
        };
        rsx! {
            span { style: "display:inline-flex;align-items:center;gap:10px;margin-right:8px;",
                ConnectionStatus {}
                Avatars { doc }
            }
        }
    }

    /// A set's room while one of its pages is open (keyed by the set, so a
    /// new set is a new room): where you are out to the others, and the
    /// leader's transport in. The room is the set's own document.
    #[component]
    pub fn SetRoom(set: SetRef, view: &'static str) -> Element {
        let app = use_app();
        let doc = use_doc(crate::orgs::qualified(
            &set.owner,
            &crate::setlist::setlist_doc(&set.id),
        ));
        let id = set.id.clone();
        let mut following = app.sync.following;
        // When you pressed Play yourself (ms), while you lead.
        let mut started = use_signal(|| None::<f64>);

        use_hook({
            let doc = doc.clone();
            move || {
                let mut s = app.sync.set;
                s.set(Some(doc));
            }
        });
        use_drop({
            let doc = doc.clone();
            move || {
                let mut s = app.sync.set;
                if s.peek().as_ref().is_some_and(|d| d.id == doc.id) {
                    s.set(None);
                    following.set(None);
                }
            }
        });

        // The first to press Play leads; the rest follow.
        use_effect({
            let doc = doc.clone();
            let id = id.clone();
            move || {
                let others = doc.others();
                let mine = started();
                match leader(&others, &id, mine) {
                    Some((name, l)) => {
                        if following.peek().as_deref() != Some(name.as_str()) {
                            following.set(Some(name));
                        }
                        let mut live = app.live;
                        let cur = *live.peek();
                        if (cur.song, cur.sec, cur.bar, cur.playing)
                            != (l.song, l.sec, l.bar, l.playing)
                        {
                            live.with_mut(|x| {
                                x.song = l.song;
                                x.sec = l.sec;
                                x.bar = l.bar;
                                x.playing = l.playing;
                            });
                        }
                    }
                    None if following.peek().is_some() => {
                        following.set(None);
                        let mut live = app.live;
                        live.with_mut(|x| x.playing = false);
                    }
                    None => {}
                }
            }
        });

        // Where you are, out to the others: the song whose sheet you have
        // open, and your transport while you lead.
        use_effect(use_reactive((&view,), {
            let doc = doc.clone();
            let id = id.clone();
            move |(view,)| {
                let l = (app.live)();
                let sheet = (app.sheet)();
                let leading = l.playing && following().is_none();
                if leading && started.peek().is_none() {
                    started.set(Some(drive_dioxus::env::now_ms()));
                } else if !leading && started.peek().is_some() {
                    started.set(None);
                }
                let (row, edit) = match sheet {
                    Some(Sheet::Key(s, e)) if s.id == id => (Some(e), Some("key")),
                    Some(Sheet::Arr(s, e)) if s.id == id => (Some(e), Some("arr")),
                    Some(Sheet::Entry(s, e)) if s.id == id => (Some(e), Some("entry")),
                    _ => match (app.picked)() {
                        Some(e) => (Some(e), Some("entry")),
                        None => (None, None),
                    },
                };
                let live = (*started.peek()).map(|since| {
                    json!({ "song": l.song, "sec": l.sec, "bar": l.bar, "playing": l.playing, "since": since })
                });
                doc.set_presence(
                    &json!({ "set": id, "view": view, "row": row, "edit": edit, "live": live }),
                );
            }
        }));

        rsx! {}
    }

    /// Who leads the chart: whoever else in this set has played longest,
    /// unless you started first.
    fn leader(others: &[Presence], set: &str, mine: Option<f64>) -> Option<(String, Live)> {
        let (since, name, l) = others
            .iter()
            .filter(|p| p.data.get("set").and_then(Value::as_str) == Some(set))
            .filter_map(|p| {
                let l = p.data.get("live")?;
                let n = |k: &str| l.get(k).and_then(Value::as_u64).map(|v| v as usize);
                let live = Live {
                    song: n("song")?,
                    sec: n("sec")?.min(data::SECS.len() - 1),
                    bar: n("bar")?,
                    playing: l.get("playing").and_then(Value::as_bool)?,
                    click: false,
                    looping: false,
                };
                let since = l.get("since").and_then(Value::as_f64)?;
                live.playing.then(|| (since, p.name.clone(), live))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        match mine {
            Some(m) if m <= since => None,
            _ => Some((name, l)),
        }
    }

    fn viewer(p: &Presence) -> Viewer {
        Viewer {
            name: p.name.clone(),
            hue: p.hue,
            row: p
                .data
                .get("row")
                .and_then(Value::as_str)
                .map(str::to_string),
            edit: p
                .data
                .get("edit")
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }

    /// The route's view of a set, for presence.
    pub fn view_of(route: &Route) -> &'static str {
        match route {
            Route::Chart { .. } => "chart",
            Route::Lyrics { .. } => "lyrics",
            _ => "set",
        }
    }
}

/// Without Drive: the app's memory alone.
#[cfg(not(feature = "drive"))]
mod local {
    use dioxus::prelude::*;

    use super::*;
    use crate::Route;

    #[derive(Clone, Copy, Default)]
    pub struct Sync;

    impl Sync {
        pub fn new() -> Self {
            Self
        }
        pub fn switch_org(self, _org: &str) -> bool {
            false
        }
        pub fn viewers(self) -> Vec<Viewer> {
            Vec::new()
        }
        pub fn following(self) -> Option<String> {
            None
        }
        pub fn is_following(self) -> bool {
            false
        }
        pub fn put_events(self, _events: &[drive_spine::Event]) {}
    }

    #[component]
    pub fn Provider(children: Element) -> Element {
        children
    }

    #[component]
    pub fn Here() -> Element {
        rsx! {}
    }

    #[component]
    pub fn SetRoom(set: crate::state::SetRef, view: &'static str) -> Element {
        let _ = (set, view);
        rsx! {}
    }

    pub fn view_of(_route: &Route) -> &'static str {
        "set"
    }
}

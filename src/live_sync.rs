//! Live sets on Drive, the local-first sync: a set's keys and arrangements
//! shared by everyone who has it open, who is here and which song they are
//! on, the chart following whoever pressed Play first, and the set's date
//! and rehearsals in Drive's calendar (the spine).
//!
//! Drive's client is the browser's, so this is the web build's; the
//! desktop and the phone keep the app's memory alone (the same API, doing
//! nothing).

use crate::data::{Occ, Rehearsal};
use crate::state::Override;

/// Someone else in this set, and the song they have open, if any.
#[derive(Clone, PartialEq, Debug)]
pub struct Viewer {
    pub name: String,
    pub hue: u32,
    /// The song (its place in the set) whose key or arrangement they have open.
    pub row: Option<usize>,
    /// `key` or `arr`.
    pub edit: Option<String>,
}

#[cfg(feature = "web")]
pub use web::*;

#[cfg(not(feature = "web"))]
pub use local::*;

#[cfg(feature = "web")]
mod web {
    use dioxus::prelude::*;
    use drive_dioxus::loro::{LoroDoc, LoroMap, LoroValue, ValueOrContainer};
    use drive_dioxus::ui::{Avatars, ConnectionStatus};
    use drive_dioxus::{Doc, DriveCtx, DriveProvider, Presence, use_doc, use_drive};
    use drive_spine as spine;
    use serde_json::{Value, json};

    use super::*;
    use crate::Route;
    use crate::data;
    use crate::state::{Live, Sheet, use_app};

    /// The document a set lives in.
    fn doc_id(set: &str) -> String {
        format!("studio:set:{set}")
    }

    /// A key or an arrangement read back from a document, as the app's own.
    fn known_key(k: &str) -> Option<&'static str> {
        data::FIFTHS.iter().copied().find(|x| *x == k)
    }

    fn known_arr(a: &str) -> Option<&'static str> {
        data::ARRS.iter().map(|x| x.0).find(|x| *x == a)
    }

    /// The calendar event of a week's set.
    fn set_event(o: &Occ) -> spine::Event {
        let pl = o.series.place();
        spine::Event {
            id: doc_id(&o.id),
            title: format!("{} · set", o.name()),
            app: "studio".into(),
            link: format!("/studio/set/{}", o.id),
            date: o.date.iso(),
            time: pl.map_or(String::new(), |p| p.starts.to_string()),
            place: pl.map_or(String::new(), |p| p.room.to_string()),
        }
    }

    /// The inbox task a set with empty slots asks for.
    fn plan_task(o: &Occ) -> Option<String> {
        let n = o.series.size.saturating_sub(o.songs.len());
        (n > 0).then(|| {
            let songs = if n == 1 { "song" } else { "songs" };
            format!("Plan {n} more {songs} for {}, {}", o.name(), o.date.short())
        })
    }

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

        /// Share a song's key and arrangement in a set with everyone in it.
        pub fn put_override(self, set: &str, i: usize, o: Override) {
            let Some(doc) = self.set.peek().clone().filter(|d| d.id == doc_id(set)) else {
                return;
            };
            doc.edit(|d| {
                if let Ok(m) = d.get_map("songs").ensure_mergeable_map(&i.to_string()) {
                    if let Some(k) = o.key {
                        let _ = m.insert("key", k);
                    }
                    if let Some(a) = o.arr {
                        let _ = m.insert("arr", a);
                    }
                }
            });
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

        /// Put a week's set in Drive's calendar, and ask the inbox for the
        /// songs it is short of. Says what it did.
        pub fn add_to_calendar(self, o: &Occ) -> String {
            let Some(doc) = self.spine.peek().clone() else {
                return "Not connected to Drive".into();
            };
            let by = self
                .drive
                .peek()
                .as_ref()
                .map(|c| c.me.peek().name.clone())
                .unwrap_or_default();
            let event = set_event(o);
            let link = event.link.clone();
            let task = plan_task(o);
            doc.edit(|d| {
                spine::put_event(d, &event);
                if let Some(t) = &task
                    && !spine::tasks(d)
                        .iter()
                        .any(|x| !x.done && x.link == link && &x.title == t)
                {
                    spine::add_task(d, t, "studio", &link, &by);
                }
            });
            match task {
                Some(_) => "Added to the calendar, and a task to plan the rest".into(),
                None => "Added to the calendar".into(),
            }
        }

        /// Put a booked rehearsal in Drive's calendar.
        pub fn book(self, r: &Rehearsal) {
            let Some(doc) = self.spine.peek().clone() else {
                return;
            };
            let name = data::occ(&r.set).map_or_else(|| r.set.clone(), |o| o.name());
            let date = r.date.iso();
            doc.edit(|d| {
                spine::put_event(
                    d,
                    &spine::Event {
                        id: format!("studio:reh:{}:{date}:{}", r.set, r.time),
                        title: format!("Rehearsal · {name}"),
                        app: "studio".into(),
                        link: format!("/studio/set/{}", r.set),
                        date: date.clone(),
                        time: r.time.clone(),
                        place: r.room.clone(),
                    },
                );
            });
        }
    }

    /// Connects to Drive (the node this page came from) around the app.
    #[component]
    pub fn Provider(children: Element) -> Element {
        rsx! {
            DriveProvider {
                Attach { {children} }
            }
        }
    }

    /// Hands the connection and the spine to the app's state.
    #[component]
    fn Attach(children: Element) -> Element {
        let app = use_app();
        let ctx = use_drive();
        let spine_doc = use_doc(spine::SPINE);
        use_hook(move || {
            let (mut d, mut s) = (app.sync.drive, app.sync.spine);
            d.set(Some(ctx));
            s.set(Some(spine_doc));
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

    /// A set's live document while one of its pages is open (keyed by the
    /// set, so a new set is a new room): its keys and arrangements into the
    /// app, where you are out to the others, and the leader's transport in.
    #[component]
    pub fn SetRoom(id: String, view: &'static str) -> Element {
        let app = use_app();
        let ctx = use_drive();
        let doc = use_doc(doc_id(&id));
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
                ctx.drive.close(&doc.id);
            }
        });

        // The document's keys and arrangements, into the app's.
        use_effect({
            let doc = doc.clone();
            let id = id.clone();
            move || {
                let found = overrides(&doc.loro());
                let mut ov = app.ov;
                ov.with_mut(|m| {
                    for (i, o) in found {
                        m.insert((id.clone(), i), o);
                    }
                });
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
                    started.set(Some(js_sys::Date::now()));
                } else if !leading && started.peek().is_some() {
                    started.set(None);
                }
                let (row, edit) = match sheet {
                    Some(Sheet::Key(s, i)) if s == id => (Some(i), Some("key")),
                    Some(Sheet::Arr(s, i)) if s == id => (Some(i), Some("arr")),
                    _ => (None, None),
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
                .and_then(Value::as_u64)
                .map(|r| r as usize),
            edit: p
                .data
                .get("edit")
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }

    /// A set document's songs: `songs` maps a song's place to its key and arrangement.
    fn overrides(d: &LoroDoc) -> Vec<(usize, Override)> {
        let songs = d.get_map("songs");
        songs
            .keys()
            .filter_map(|k| {
                let i = k.parse().ok()?;
                let m = match songs.get(&k) {
                    Some(ValueOrContainer::Container(c)) => c.into_map().ok()?,
                    _ => return None,
                };
                Some((
                    i,
                    Override {
                        key: text(&m, "key").and_then(|k| known_key(&k)),
                        arr: text(&m, "arr").and_then(|a| known_arr(&a)),
                    },
                ))
            })
            .collect()
    }

    fn text(m: &LoroMap, k: &str) -> Option<String> {
        match m.get(k) {
            Some(ValueOrContainer::Value(LoroValue::String(v))) => Some(v.to_string()),
            _ => None,
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
#[cfg(not(feature = "web"))]
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
        pub fn put_override(self, _set: &str, _i: usize, _o: Override) {}
        pub fn viewers(self) -> Vec<Viewer> {
            Vec::new()
        }
        pub fn following(self) -> Option<String> {
            None
        }
        pub fn is_following(self) -> bool {
            false
        }
        pub fn add_to_calendar(self, _o: &Occ) -> String {
            "The calendar is Drive's, on the web".into()
        }
        pub fn book(self, _r: &Rehearsal) {}
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
    pub fn SetRoom(id: String, view: &'static str) -> Element {
        let _ = (id, view);
        rsx! {}
    }

    pub fn view_of(_route: &Route) -> &'static str {
        "set"
    }
}

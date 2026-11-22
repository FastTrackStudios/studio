//! A set: its document, live. The running order in its sections (drag a
//! row to move it on a desktop; a phone moves it from its sheet), each
//! song with the key, arrangement and leader it is played with, the items
//! between songs, the band, your part, rehearsals and sharing. A desktop
//! edits the picked entry in the inspector beside the list; a phone in a
//! sheet. The same document is the room everyone in the set shares.

use dioxus::prelude::*;

use crate::Route;
use crate::data;
use crate::icons::{I, Icon};
use crate::orgs;
use crate::pieces::{Bar, Cal, Hair, Hero, Joined, Page, Tool};
use crate::setlist::{self, Entry, ItemKind, Played};
use crate::sets::{self, SetDoc};
use crate::state::{LiveSong, Menu, SetRef, Sheet, use_app};

/// A set of the org Studio is in.
#[component]
pub fn SetPage(id: String) -> Element {
    let here = (use_app().here)();
    rsx! {
        SetView { key: "{here}/{id}", owner: here, id }
    }
}

/// A set of another org (one shared with this one, or a built-in show).
#[component]
pub fn OrgSet(owner: String, id: String) -> Element {
    rsx! {
        SetView { key: "{owner}/{id}", owner, id }
    }
}

/// What the pages of a set need of the org's library and team to say how
/// each song is played: the set's own org's (an org a set is shared with
/// reads the entries' own faces and overrides).
#[derive(Clone, PartialEq)]
pub struct Ctx {
    pub set: SetDoc,
    pub lib: Option<crate::songs::Library>,
    pub team: crate::team::Team,
    pub here: String,
}

impl Ctx {
    /// What an entry is played as.
    pub fn played(&self, e: &Entry) -> Played {
        let (u, face) = match (&self.lib, &e.song) {
            (Some(lib), Some(r)) => (
                lib.use_of(&r.owner, &r.id),
                (r.owner == lib.org).then(|| lib.get(&r.id)).flatten(),
            ),
            _ => (orgs::Use::default(), None),
        };
        let st = if self.lib.is_some() {
            self.team.settings()
        } else {
            orgs::Settings::default()
        };
        setlist::resolve(e, face.as_ref(), &u, &st, &self.set.info())
    }

    /// A person's name: the band's, else the team's, else their id.
    pub fn name(&self, id: &str) -> String {
        if let Some(s) = self.set.band().into_iter().find(|s| s.person == id) {
            return s.name;
        }
        if self.lib.is_some()
            && let Some(p) = self.team.person(id)
        {
            return p.name;
        }
        data::cap(id)
    }

    /// The set's ref.
    pub fn at(&self) -> SetRef {
        SetRef::new(&self.set.org, &self.set.id)
    }

    /// Whether this org owns the set.
    pub fn own(&self) -> bool {
        self.set.org == self.here
    }
}

pub fn use_ctx(owner: &str, id: &str) -> Ctx {
    let app = use_app();
    let here = (app.here)();
    let set = sets::use_set(owner, id);
    let lib = crate::songs::use_library_of(owner);
    let team = crate::team::use_team_of(owner);
    Ctx {
        set,
        lib: (owner == here).then_some(lib),
        team,
        here,
    }
}

/// Keeps a set's pages going while any of them is open: what follows the
/// set (its face, calendar, sharing), the room, and the songs Chart and
/// Lyrics follow. Mounted once by the shell around a set's pages.
#[component]
pub fn SetLive(set: SetRef, view: &'static str) -> Element {
    let app = use_app();
    let ctx = use_ctx(&set.owner, &set.id);
    use_hook({
        let set = set.clone();
        move || app.enter_set(&set)
    });
    let list: Vec<LiveSong> = ctx
        .set
        .songs()
        .iter()
        .map(|e| {
            let p = ctx.played(e);
            LiveSong {
                title: e.title.clone(),
                key: p.key,
                own_key: p.own_key,
                tempo: p.tempo,
            }
        })
        .collect();
    use_effect(use_reactive!(|list| {
        let mut l = app.live_list;
        if *l.peek() != list {
            l.set(list);
        }
    }));
    rsx! {
        sets::SetMirror { set: ctx.set.clone() }
        crate::live_sync::SetRoom { set, view }
    }
}

#[component]
fn SetView(owner: String, id: String) -> Element {
    let app = use_app();
    let ctx = use_ctx(&owner, &id);
    let info = ctx.set.info();
    let entries = ctx.set.entries();
    let shell = app_shell::ui::use_shell();
    let desktop = shell.form() == app_shell::ui::FormFactor::Desktop;
    let at = ctx.at();
    if !ctx.set.ready() && entries.is_empty() {
        return rsx! {
            Page {
                Bar { back: Route::Setlists {}, title: "Loading…" }
            }
        };
    }
    let back = if info.series.is_empty() {
        Route::Setlists {}
    } else {
        Route::Series {
            id: info.series.clone(),
        }
    };
    let songs: Vec<&Entry> = entries.iter().filter(|e| e.is_song()).collect();
    let total: u32 = songs
        .iter()
        .filter_map(|e| e.length)
        .sum::<u32>()
        + entries
            .iter()
            .filter(|e| !e.is_song() && e.kind != ItemKind::Section)
            .filter_map(|e| e.length)
            .sum::<u32>();
    let missing = info.size.saturating_sub(songs.len());
    let img = data::logo(&info.series).or_else(|| data::art(&info.id));
    let picked = (app.picked)().filter(|p| entries.iter().any(|e| &e.id == p));
    let tools = {
        let (a1, a2, a3) = (at.clone(), at.clone(), at.clone());
        rsx! {
            // A desktop's top bar already says who is here.
            if !desktop {
                crate::live_sync::Here {}
            }
            Joined {
                Tool {
                    label: "Add songs",
                    icon: I::Plus,
                    onclick: move |_| app.open(Sheet::AddSongs(a1.clone())),
                }
                Hair {}
                Tool {
                    label: "The band",
                    icon: I::Users,
                    onclick: move |_| app.open(Sheet::Band(a2.clone())),
                }
                Hair {}
                Tool {
                    label: "Print or export",
                    icon: I::Printer,
                    onclick: move |_| app.open(Sheet::Export(a3.clone())),
                }
                Hair {}
                Tool {
                    label: "More",
                    icon: I::More,
                    onclick: move |_| {
                        let mut m = app.menu;
                        m.set(Some(Menu::Occ));
                    },
                }
            }
        }
    };
    let when = info.day();
    let kicker = match (info.kind, ctx.own()) {
        (_, false) => format!("Shared by {}", orgs::org_name(&owner)),
        (setlist::Kind::Template, _) => "Template".to_string(),
        (k, _) => k.label().to_string(),
    };
    let list = rsx! {
        SetList { ctx: ctx.clone(), desktop }
        if missing > 0 {
            button {
                class: "trow roomy add",
                onclick: {
                    let at = at.clone();
                    move |_| app.open(Sheet::AddSongs(at.clone()))
                },
                span { class: "sart empty",
                    Icon { name: I::Plus, size: 18, w: 2.0 }
                }
                span { class: "mid",
                    span { class: "t1",
                        if missing == 1 {
                            "1 more song to plan"
                        } else {
                            "{missing} more songs to plan"
                        }
                    }
                    span { class: "s1", "From the library" }
                }
                span {}
            }
        } else if entries.is_empty() {
            button {
                class: "trow roomy add",
                onclick: {
                    let at = at.clone();
                    move |_| app.open(Sheet::AddSongs(at.clone()))
                },
                span { class: "sart empty",
                    Icon { name: I::Plus, size: 18, w: 2.0 }
                }
                span { class: "mid",
                    span { class: "t1", "Add songs" }
                    span { class: "s1", "From the library, or a reading, a prayer, a video" }
                }
                span {}
            }
        }
        YourPart { ctx: ctx.clone() }
        BandCard { ctx: ctx.clone() }
        Rehearsals { ctx: ctx.clone() }
        if !info.notes.is_empty() {
            div { class: "dayhead", "Notes" }
            div { class: "setnotes", "{info.notes}" }
        }
        ShareRow { ctx: ctx.clone() }
    };
    rsx! {
        Page { tint: info.hue,
            Bar { back, tools }
            div { class: if desktop { "body setbody wide" } else { "body setbody" },
                Hero {
                    img,
                    hue: info.hue,
                    title: info.title.clone(),
                    kicker,
                    facts: rsx! {
                        span { class: "when2",
                            if let Some(d) = when {
                                Cal { top: data::MON[d.month0()], big: d.mday().to_string() }
                            }
                            if !info.place.is_empty() || !info.call.is_empty() {
                                span { class: "loc",
                                    Icon { name: I::Mappin, size: 14, w: 2.0 }
                                    span {
                                        b { "{info.place}" }
                                        small {
                                            if !info.call.is_empty() {
                                                span { class: "call", "Call {info.call}" }
                                            }
                                            if !info.starts.is_empty() {
                                                " · Starts {info.starts}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    sub: rsx! {
                        button {
                            class: "infobtn",
                            onclick: {
                                let at = at.clone();
                                move |_| app.open(Sheet::SetInfo(at.clone()))
                            },
                            if let Some(d) = when {
                                "{d.when()}"
                            } else {
                                "No date"
                            }
                            Icon { name: I::Pencil, size: 12, w: 2.2 }
                        }
                        if missing > 0 {
                            " · "
                            span { class: "todo2", "{songs.len()} of {info.size} songs" }
                        }
                        span { class: "hcount",
                            "{songs.len()} songs"
                            if total > 0 {
                                " · {data::dur(f64::from(total))}"
                            }
                        }
                    },
                }
                if desktop {
                    div { class: "setgrid",
                        div { class: "setmain", {list} }
                        aside { class: "inspector",
                            if let Some(p) = picked {
                                EntryEditor { key: "{p}", ctx: ctx.clone(), entry: p }
                            } else {
                                div { class: "insp-empty",
                                    Icon { name: I::Setlist, size: 22 }
                                    b { "Pick a song" }
                                    span { "Its key, arrangement, who leads it, notes and how it goes into the next." }
                                }
                            }
                        }
                    }
                } else {
                    {list}
                }
            }
        }
    }
}

/// The running order, in its sections. On a desktop each row drags.
#[component]
fn SetList(ctx: Ctx, desktop: bool) -> Element {
    let entries = ctx.set.entries();
    let mut dragging = use_signal(|| None::<String>);
    let mut over = use_signal(|| None::<String>);
    let mut k = 0usize;
    let rows: Vec<(Entry, usize)> = entries
        .iter()
        .map(|e| {
            if e.is_song() {
                k += 1;
            }
            (e.clone(), k)
        })
        .collect();
    let set = ctx.set.clone();
    rsx! {
        div { class: "scard setlist",
            for (e , n) in rows {
                EntryRow {
                    key: "{e.id}",
                    ctx: ctx.clone(),
                    entry: e.clone(),
                    n,
                    desktop,
                    over: over() == Some(e.id.clone()) && dragging().is_some_and(|d| d != e.id),
                    ondragstart: move |id: String| dragging.set(Some(id)),
                    ondragover: move |id: String| {
                        if over.peek().as_deref() != Some(id.as_str()) {
                            over.set(Some(id));
                        }
                    },
                    ondrop: {
                        let set = set.clone();
                        move |before: String| {
                            if let Some(d) = dragging.take() && d != before {
                                set.move_before(&d, Some(&before));
                            }
                            over.set(None);
                        }
                    },
                    ondragend: move |_| {
                        dragging.set(None);
                        over.set(None);
                    },
                }
            }
            if desktop && dragging().is_some() {
                div {
                    class: if over().as_deref() == Some("") { "dropend on" } else { "dropend" },
                    ondragover: move |e| {
                        e.prevent_default();
                        over.set(Some(String::new()));
                    },
                    ondrop: {
                        let set = set.clone();
                        move |e: DragEvent| {
                            e.prevent_default();
                            if let Some(d) = dragging.take() {
                                set.move_before(&d, None);
                            }
                            over.set(None);
                        }
                    },
                    "Drop here for the end"
                }
            }
        }
    }
}

/// The icon of an item that is not a song.
pub fn item_icon(k: ItemKind) -> I {
    match k {
        ItemKind::Song => I::Audio,
        ItemKind::Section => I::Hash,
        ItemKind::Reading => I::Book,
        ItemKind::Announcement => I::Megaphone,
        ItemKind::Prayer => I::Heart,
        ItemKind::Video => I::Video,
        ItemKind::Other => I::Message,
    }
}

/// A row of the running order: a section's head, a song, or an item.
#[component]
fn EntryRow(
    ctx: Ctx,
    entry: Entry,
    n: usize,
    desktop: bool,
    over: bool,
    ondragstart: EventHandler<String>,
    ondragover: EventHandler<String>,
    ondrop: EventHandler<String>,
    ondragend: EventHandler<()>,
) -> Element {
    let app = use_app();
    let at = ctx.at();
    let id = entry.id.clone();
    let picked = (app.picked)().as_deref() == Some(id.as_str());
    let open = {
        let (id, at) = (id.clone(), at.clone());
        move |_| {
            if desktop {
                let mut p = app.picked;
                p.set(Some(id.clone()));
            } else {
                app.open(Sheet::Entry(at.clone(), id.clone()));
            }
        }
    };
    // Others with this entry open, like a live selection.
    let here: Vec<_> = app
        .sync
        .viewers()
        .into_iter()
        .filter(|v| v.row.as_deref() == Some(id.as_str()))
        .collect();
    let ring = here.first().map_or(String::new(), |v| {
        format!(
            "box-shadow:inset 0 0 0 2px oklch(.62 .16 {});border-radius:12px;",
            v.hue
        )
    });
    let mut cls = vec!["erow"];
    if entry.kind == ItemKind::Section {
        cls.push("esec");
    } else if !entry.is_song() {
        cls.push("eitem");
    }
    if picked {
        cls.push("picked");
    }
    if over {
        cls.push("over");
    }
    let cls = cls.join(" ");
    let drag = (
        id.clone(),
        id.clone(),
        id.clone(),
    );
    let body = match entry.kind {
        ItemKind::Section => rsx! {
            button { class: "mid", onclick: open,
                span { class: "t1", "{entry.title}" }
            }
        },
        ItemKind::Song => {
            let p = ctx.played(&entry);
            let leader = p.leader.as_ref().map(|l| ctx.name(l));
            let (ka, aa) = (at.clone(), at.clone());
            let (kid, aid) = (id.clone(), id.clone());
            rsx! {
                span { class: "n", "{n}" }
                button { class: "mid", onclick: open,
                    span { class: "t1", "{entry.title}" }
                    span { class: "s1",
                        "{entry.artist.clone().unwrap_or_default()}"
                        if let Some(o) = entry.song.as_ref().filter(|r| r.owner != ctx.set.org) {
                            span { class: "fromorg", " · {orgs::org_name(&o.owner)}" }
                        }
                    }
                    span { class: "meta3",
                        if let Some(l) = leader {
                            span { class: "ld",
                                Icon { name: I::Mic, size: 13, w: 2.0 }
                                "{l}"
                            }
                        }
                        if let Some(t) = p.tempo {
                            span { class: "ld", "{t} bpm" }
                        }
                        if !entry.notes.is_empty() {
                            span { class: "ld",
                                Icon { name: I::Message, size: 12, w: 2.0 }
                            }
                        }
                    }
                }
                span { class: "rcol",
                    KeyChip {
                        own: p.own_key.clone(),
                        cur: p.key.clone(),
                        onclick: move |_| app.open(Sheet::Key(ka.clone(), kid.clone())),
                    }
                    span {
                        class: if p.arrangement == "Normal" { "arrsm none" } else { "arrsm" },
                        role: "button",
                        aria_label: "Arrangement: {p.arrangement}",
                        onclick: move |_| app.open(Sheet::Arr(aa.clone(), aid.clone())),
                        Icon { name: I::Branch, size: 11, w: 2.4 }
                        if p.arrangement != "Normal" {
                            "{p.arrangement}"
                        }
                    }
                }
            }
        }
        k => rsx! {
            span { class: "n ic",
                Icon { name: item_icon(k), size: 15, w: 2.0 }
            }
            button { class: "mid", onclick: open,
                span { class: "t1", "{entry.title}" }
                span { class: "s1",
                    "{k.label()}"
                    if let Some(l) = entry.length {
                        " · {data::dur(f64::from(l))}"
                    }
                }
            }
            span {}
        },
    };
    rsx! {
        div {
            class: "{cls}",
            style: "{ring}",
            draggable: if desktop { "true" } else { "false" },
            ondragstart: move |_| ondragstart.call(drag.0.clone()),
            ondragover: move |e| {
                e.prevent_default();
                ondragover.call(drag.1.clone());
            },
            ondrop: move |e| {
                e.prevent_default();
                ondrop.call(drag.2.clone());
            },
            ondragend: move |_| ondragend.call(()),
            if desktop {
                span { class: "grip", aria_hidden: "true",
                    Icon { name: I::Grip, size: 14, w: 2.0 }
                }
            }
            for v in here.iter() {
                span {
                    key: "{v.name}",
                    class: "herewho",
                    style: "background:oklch(.62 .16 {v.hue});",
                    match v.edit.as_deref() {
                        Some("arr") => "{v.name} · arrangement",
                        Some("key") => "{v.name} · key",
                        _ => "{v.name}",
                    }
                }
            }
            {body}
        }
        if !entry.transition.is_empty() {
            div { class: "segue",
                Icon { name: I::ChevDown, size: 12, w: 2.2 }
                "{entry.transition}"
            }
        }
    }
}

/// The key: big in its colour; moved, how far from the song's own.
#[component]
pub fn KeyChip(
    own: Option<String>,
    cur: Option<String>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let Some(cur) = cur else {
        return rsx! {
            button {
                class: "key2 nokey",
                aria_label: "Set the key",
                onclick: move |e| onclick.call(e),
                b { "Key" }
            }
        };
    };
    let label = crate::keys::KeyLabel::parse(own.as_deref().unwrap_or(&cur))
        .unwrap_or_else(|| crate::keys::KeyLabel::plain(&cur));
    let (shown, d) = match label.played(&cur) {
        crate::keys::Played::Moved { label, semitones } => (label.text(), semitones),
        _ => (cur.clone(), 0),
    };
    let h = data::hue(&cur).unwrap_or(0);
    let said = if d != 0 {
        format!(
            "Key {shown}, {} {} from {}",
            d.abs(),
            if d > 0 { "up" } else { "down" },
            own.clone().unwrap_or_default()
        )
    } else {
        format!("Key {shown}, the song's own")
    };
    let sign = if d > 0 { "+" } else { "−" };
    rsx! {
        button {
            class: if d != 0 { "key2 moved" } else { "key2" },
            style: "--h:{h}",
            aria_label: "{said}",
            onclick: move |e| onclick.call(e),
            b { "{shown}" }
            if d != 0 {
                small { "{sign}{d.abs()}" }
            }
        }
    }
}

/// Your part in the set, when you play it: your parts, and per song its key
/// and where a guitar's capo goes.
#[component]
fn YourPart(ctx: Ctx) -> Element {
    let me = crate::team::use_me();
    let Some(me) = me.filter(|_| ctx.own()) else {
        return rsx! {};
    };
    let songs = ctx.set.songs();
    let played: Vec<(String, Played)> = songs.iter().map(|e| (e.id.clone(), ctx.played(e))).collect();
    let Some(v) = setlist::my_view(&me, &ctx.set.band(), &played) else {
        return rsx! {};
    };
    let leads = v.songs.iter().filter(|s| s.3).count();
    rsx! {
        div { class: "dayhead dh2",
            span { "Your part" }
        }
        div { class: "mypart",
            div { class: "chips",
                for p in v.parts.iter() {
                    span { key: "{p}", class: "part on", "{p}" }
                }
                if leads > 0 {
                    span { class: "part lead", "You lead {leads}" }
                }
            }
            div { class: "mysongs",
                for ((id , key , capo , lead) , e) in v.songs.iter().zip(songs.iter()) {
                    div { key: "{id}", class: "mysong",
                        span { class: "t1", "{e.title}" }
                        if *lead {
                            span { class: "ld",
                                Icon { name: I::Mic, size: 12, w: 2.0 }
                                "You"
                            }
                        }
                        span { class: "mykey",
                            "{key.clone().unwrap_or_else(|| \"—\".into())}"
                            if let Some((f, s)) = capo {
                                small { " · capo {f}, {s} shapes" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Who plays the set, and what.
#[component]
fn BandCard(ctx: Ctx) -> Element {
    let app = use_app();
    let band = ctx.set.band();
    let at = ctx.at();
    rsx! {
        div { class: "dayhead dh2",
            span { "The band" }
            button {
                class: "seeall",
                onclick: move |_| app.open(Sheet::Band(at.clone())),
                if band.is_empty() {
                    "Choose "
                } else {
                    "Change "
                }
                Icon { name: I::ChevRight, size: 14 }
            }
        }
        if band.is_empty() {
            div { class: "setnotes muted", "Nobody yet." }
        } else {
            div { class: "bandrow",
                for s in band {
                    span { key: "{s.person}", class: "seat",
                        span {
                            class: "pmark",
                            style: "--h:{data::str_hue(&s.name)};width:26px;height:26px;font-size:11px",
                            "{initials(&s.name)}"
                        }
                        span { class: "mid",
                            b { "{s.name}" }
                            small { "{s.parts.join(\", \")}" }
                        }
                    }
                }
            }
        }
    }
}

pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// The set's rehearsals, and booking another.
#[component]
fn Rehearsals(ctx: Ctx) -> Element {
    let app = use_app();
    let list = ctx.set.rehearsals();
    let info = ctx.set.info();
    if info.kind == setlist::Kind::Template {
        return rsx! {};
    }
    let at = ctx.at();
    rsx! {
        div { class: "dayhead dh2",
            span { "Rehearsals" }
            button {
                class: "bookbtn",
                onclick: move |_| crate::pages::book_for(app, &at, &info),
                Icon { name: I::Plus, size: 14, w: 2.4 }
                "Rehearsal"
            }
        }
        if !list.is_empty() {
            div { class: "agenda",
                for r in list {
                    if let Some(d) = setlist::parse_day(&r.date) {
                        div { key: "{r.id}", class: "ag reh",
                            crate::pieces::DayTile { day: d }
                            span { class: "rehic",
                                Icon { name: I::Mic, size: 17, w: 2.0 }
                            }
                            span { class: "mid",
                                span { class: "t1", "Rehearsal" }
                                span { class: "s1", "{r.room} · {r.time}" }
                            }
                            span {}
                        }
                    }
                }
            }
        }
    }
}

/// Who the set is shared with.
#[component]
fn ShareRow(ctx: Ctx) -> Element {
    let app = use_app();
    let with = ctx.set.shared_with();
    if !ctx.own() {
        return rsx! {};
    }
    let at = ctx.at();
    rsx! {
        div { class: "dayhead dh2",
            span { "Shared with" }
            button {
                class: "seeall",
                onclick: move |_| app.open(Sheet::ShareSet(at.clone())),
                "Share "
                Icon { name: I::ChevRight, size: 14 }
            }
        }
        div { class: "sharerow",
            if with.is_empty() {
                span { class: "setnotes muted", "Only {orgs::org_name(&ctx.set.org)}." }
            }
            for o in with {
                span { key: "{o}", class: "orgtag", style: "--h:{orgs::org_hue(&o)}", "{orgs::org_name(&o)}" }
            }
        }
    }
}

/// Everything about an entry: the inspector on a desktop, a sheet on a phone.
#[component]
pub fn EntryEditor(ctx: Ctx, entry: String) -> Element {
    let app = use_app();
    let Some(e) = ctx.set.entry(&entry) else {
        return rsx! {
            div { class: "insp-empty", "It was taken out of the set." }
        };
    };
    let set = ctx.set.clone();
    let p = ctx.played(&e);
    let at = ctx.at();
    let people: Vec<(String, String)> = {
        let mut v: Vec<(String, String)> = ctx
            .set
            .band()
            .into_iter()
            .map(|s| (s.person, s.name))
            .collect();
        if ctx.lib.is_some() {
            for x in ctx.team.people() {
                if !v.iter().any(|(id, _)| *id == x.id) {
                    v.push((x.id, x.name));
                }
            }
        }
        v
    };
    let edit = {
        let (set, e) = (set.clone(), e.clone());
        move |f: &dyn Fn(&mut Entry)| {
            let mut x = set.entry(&e.id).unwrap_or_else(|| e.clone());
            f(&mut x);
            set.update(&x);
        }
    };
    let arrangements: Vec<String> = {
        let mut v: Vec<String> = data::ARRS.iter().map(|a| a.0.to_string()).collect();
        if let (Some(lib), Some(r)) = (&ctx.lib, &e.song) {
            for a in lib.use_of(&r.owner, &r.id).arrangements {
                if !v.contains(&a) {
                    v.push(a);
                }
            }
        }
        v
    };
    let song_route = e.song.as_ref().map(|r| {
        if r.owner == ctx.here {
            Route::SongPage { id: r.id.clone() }
        } else {
            Route::SharedSong {
                owner: r.owner.clone(),
                id: r.id.clone(),
            }
        }
    });
    let e1 = edit.clone();
    let e2 = edit.clone();
    let e3 = edit.clone();
    let e4 = edit.clone();
    let e5 = edit.clone();
    let e6 = edit.clone();
    let e7 = edit.clone();
    let e8 = edit.clone();
    let (m1, m2, rm) = (set.clone(), set.clone(), set.clone());
    let (id1, id2, id3) = (e.id.clone(), e.id.clone(), e.id.clone());
    rsx! {
        div { class: "editor",
            div { class: "edhead",
                if e.is_song() {
                    div { class: "mid",
                        b { "{e.title}" }
                        span { class: "s1",
                            "{e.artist.clone().unwrap_or_default()}"
                            if let Some(r) = &e.song && r.owner != ctx.set.org {
                                " · from {orgs::org_name(&r.owner)}"
                            }
                        }
                    }
                    if let Some(r) = song_route {
                        button {
                            class: "seeall",
                            onclick: move |_| {
                                app.close();
                                navigator().push(r.clone());
                            },
                            "Song "
                            Icon { name: I::ChevRight, size: 14 }
                        }
                    }
                } else {
                    span { class: "edkind",
                        Icon { name: item_icon(e.kind), size: 16 }
                        "{e.kind.label()}"
                    }
                    input {
                        class: "edtitle",
                        aria_label: "Name",
                        value: "{e.title}",
                        onchange: move |v| {
                            let t = v.value();
                            e1(&|x| x.title = t.clone());
                        },
                    }
                }
            }
            if e.is_song() {
                div { class: "fsec",
                    "Key"
                    if let Some(own) = &p.own_key {
                        small { " · the song's own {own}" }
                    }
                }
                div { class: "keypick small",
                    for k in data::FIFTHS {
                        {
                            let on = p.key.as_deref().and_then(data::norm_key) == Some(k);
                            let e2 = e2.clone();
                            rsx! {
                                button {
                                    key: "{k}",
                                    class: if on { "kp on" } else { "kp" },
                                    style: "--h:{data::hue(k).unwrap_or(0)}",
                                    onclick: move |_| e2(&|x| x.over.key = Some(k.to_string())),
                                    b { "{k}" }
                                }
                            }
                        }
                    }
                }
                if e.over.key.is_some() {
                    button {
                        class: "linkbtn",
                        onclick: move |_| e3(&|x| x.over.key = None),
                        "Back to the org's key"
                    }
                }
                div { class: "fsec", "Arrangement" }
                div { class: "seg wrap",
                    for a in arrangements {
                        {
                            let on = p.arrangement == a;
                            let e4 = e4.clone();
                            let a2 = a.clone();
                            rsx! {
                                button {
                                    key: "{a}",
                                    class: "fopt",
                                    aria_pressed: "{on}",
                                    onclick: move |_| {
                                        let a = a2.clone();
                                        e4(&|x| x.over.arrangement = Some(a.clone()));
                                    },
                                    "{a}"
                                }
                            }
                        }
                    }
                }
                div { class: "edrow",
                    label {
                        span { "Leads it" }
                        select {
                            value: "{p.leader.clone().unwrap_or_default()}",
                            onchange: move |v| {
                                let l = v.value();
                                e5(&|x| x.over.leader = (!l.is_empty()).then(|| l.clone()));
                            },
                            option { value: "", "Nobody chosen" }
                            for (id , name) in people.iter() {
                                option { key: "{id}", value: "{id}", selected: p.leader.as_deref() == Some(id.as_str()), "{name}" }
                            }
                        }
                    }
                    label {
                        span { "Tempo" }
                        input {
                            r#type: "number",
                            min: "30",
                            max: "300",
                            value: "{p.tempo.map(|t| t.to_string()).unwrap_or_default()}",
                            onchange: move |v| {
                                let t = v.value().parse::<u32>().ok().filter(|t| *t > 0);
                                e6(&|x| x.over.tempo = t);
                            },
                        }
                    }
                }
            } else if e.kind != ItemKind::Section {
                div { class: "edrow",
                    label {
                        span { "Minutes" }
                        input {
                            r#type: "number",
                            min: "0",
                            max: "120",
                            value: "{e.length.map(|l| (l / 60).to_string()).unwrap_or_default()}",
                            onchange: move |v| {
                                let m = v.value().parse::<u32>().ok().filter(|m| *m > 0);
                                e6(&|x| x.length = m.map(|m| m * 60));
                            },
                        }
                    }
                }
            }
            if e.kind != ItemKind::Section {
                div { class: "fsec", "Notes" }
                textarea {
                    class: "ednotes",
                    rows: "3",
                    aria_label: "Notes",
                    value: "{e.notes}",
                    onchange: move |v| {
                        let n = v.value();
                        e7(&|x| x.notes = n.clone());
                    },
                }
                div { class: "fsec", "Into the next" }
                div { class: "seg wrap",
                    for t in ["", "Segue", "Pad under", "Count off", "Stop"] {
                        {
                            let on = e.transition == t;
                            let e8 = e8.clone();
                            rsx! {
                                button {
                                    key: "{t}",
                                    class: "fopt",
                                    aria_pressed: "{on}",
                                    onclick: move |_| e8(&|x| x.transition = t.to_string()),
                                    if t.is_empty() {
                                        "Nothing"
                                    } else {
                                        "{t}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "edmove",
                button {
                    class: "secondary",
                    aria_label: "Move up",
                    onclick: move |_| m1.move_by(&id1, -1),
                    Icon { name: I::ArrowUp, size: 16 }
                    "Up"
                }
                button {
                    class: "secondary",
                    aria_label: "Move down",
                    onclick: move |_| m2.move_by(&id2, 1),
                    span { style: "display:inline-flex;transform:rotate(180deg)",
                        Icon { name: I::ArrowUp, size: 16 }
                    }
                    "Down"
                }
                button {
                    class: "secondary danger",
                    onclick: move |_| {
                        rm.remove(&id3);
                        let mut p = app.picked;
                        p.set(None);
                        app.close();
                        app.say("Taken out of the set");
                    },
                    Icon { name: I::Trash, size: 16 }
                    "Take out"
                }
            }
            if !ctx.own() {
                div { class: "s1 edfoot", "Shared by {orgs::org_name(&at.owner)}: your edits are theirs too." }
            }
        }
    }
}

//! The sheets that edit a set: a song's key and arrangement, an entry,
//! adding songs and items, the band, the set's details, sharing it,
//! printing or copying it, making a new set, booking a rehearsal.

use dioxus::prelude::*;

use crate::Route;
use crate::data::{self, DAY, FIFTHS, MON, TODAY, dur};
use crate::icons::{I, Icon};
use crate::orgs;
use crate::pages::set::{Ctx, EntryEditor, initials, item_icon, use_ctx};
use crate::setlist::{self, Entry, Info, ItemKind, Kind, Seat};
use crate::sets;
use crate::state::{Booking, SetRef, use_app};

/// A sheet's card: taps inside stay inside.
#[component]
fn BSheet(label: String, children: Element) -> Element {
    rsx! {
        div { class: "bsheet", role: "dialog", aria_label: "{label}", {children} }
    }
}

/// A song's key in a set: the twelve round the circle, how far from its own.
#[component]
pub fn KeySheet(set: SetRef, entry: String) -> Element {
    let app = use_app();
    let ctx = use_ctx(&set.owner, &set.id);
    let Some(e) = ctx.set.entry(&entry) else {
        return rsx! {};
    };
    let p = ctx.played(&e);
    let own = p.own_key.as_deref().and_then(data::norm_key).unwrap_or("C");
    let cur = p.key.as_deref().and_then(data::norm_key).unwrap_or(own);
    let doc = ctx.set.clone();
    let pick = move |k: Option<&'static str>| {
        let (doc, e) = (doc.clone(), e.clone());
        move |_| {
            let mut x = doc.entry(&e.id).unwrap_or_else(|| e.clone());
            x.over.key = k.map(str::to_string);
            doc.update(&x);
            app.close();
        }
    };
    let moved = ctx.set.entry(&entry).is_some_and(|x| x.over.key.is_some());
    rsx! {
        BSheet { label: "Key",
            div { class: "bhead",
                b { "{p.key.clone().unwrap_or_default()} · {ctx.set.entry(&entry).map(|x| x.title).unwrap_or_default()}" }
                span { class: "bsub", "The song's own: {own}" }
            }
            div { class: "keypick",
                for k in FIFTHS {
                    {
                        let d = data::steps(own, k);
                        let mut cls = vec!["kp"];
                        if k == cur {
                            cls.push("on");
                        }
                        if k == own {
                            cls.push("orig");
                        }
                        let sign = if d > 0 { "+" } else { "−" };
                        rsx! {
                            button {
                                key: "{k}",
                                class: "{cls.join(\" \")}",
                                style: "--h:{data::hue(k).unwrap_or(0)}",
                                onclick: pick(Some(k)),
                                b { "{k}" }
                                small {
                                    if k == own {
                                        "Its own"
                                    } else {
                                        "{sign}{d.abs()}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if moved {
                button {
                    class: "secondary wide",
                    style: "margin-top:14px",
                    onclick: pick(None),
                    "Back to the org's key"
                }
            }
        }
    }
}

/// A song's arrangement in a set, and what each does to its length.
#[component]
pub fn ArrSheet(set: SetRef, entry: String) -> Element {
    let app = use_app();
    let ctx = use_ctx(&set.owner, &set.id);
    let Some(e) = ctx.set.entry(&entry) else {
        return rsx! {};
    };
    let cur = ctx.played(&e).arrangement;
    let base = e.length.unwrap_or(270) as i32;
    rsx! {
        BSheet { label: "Arrangement",
            div { class: "bhead",
                b { "{e.title}" }
                span { class: "bsub", "Arrangement" }
            }
            for (a , dt) in data::ARRS {
                {
                    let note = if dt == 0 {
                        format!("{} · as written", dur((base + dt) as f64))
                    } else {
                        format!(
                            "{} · {}{}",
                            dur((base + dt) as f64),
                            if dt > 0 { "+" } else { "−" },
                            dur(dt.abs() as f64),
                        )
                    };
                    let (doc, e) = (ctx.set.clone(), e.clone());
                    rsx! {
                        button {
                            key: "{a}",
                            class: "mi big",
                            onclick: move |_| {
                                let mut x = doc.entry(&e.id).unwrap_or_else(|| e.clone());
                                x.over.arrangement = Some(a.to_string());
                                doc.update(&x);
                                app.close();
                            },
                            span { class: "ic",
                                Icon { name: I::Branch, size: 18 }
                            }
                            span { class: "mid",
                                span { class: "t1", "{a}" }
                                span { class: "s1", "{note}" }
                            }
                            if a == cur {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An entry, on a phone.
#[component]
pub fn EntrySheet(set: SetRef, entry: String) -> Element {
    let ctx = use_ctx(&set.owner, &set.id);
    rsx! {
        BSheet { label: "In the set",
            EntryEditor { ctx, entry }
        }
    }
}

/// Add songs from the library (this org's and what others share with it),
/// a section, or an item that is not a song.
#[component]
pub fn AddSongs(set: SetRef) -> Element {
    let app = use_app();
    let here = (app.here)();
    let ctx = use_ctx(&set.owner, &set.id);
    let mut q = use_signal(String::new);
    let mut item = use_signal(|| ItemKind::Reading);
    let mut name = use_signal(String::new);
    let lib = crate::songs::use_library_of(&here);
    let owners = crate::team::use_owners_sharing(&here);
    let in_set: Vec<String> = ctx
        .set
        .songs()
        .into_iter()
        .filter_map(|e| e.song.map(|r| r.text()))
        .collect();
    let needle = q().trim().to_lowercase();
    let mine: Vec<crate::songs::Entry> = lib
        .entries()
        .into_iter()
        .filter(|e| matches(e, &needle))
        .take(if needle.is_empty() { 8 } else { 30 })
        .collect();
    let doc = ctx.set.clone();
    let add_item = move |_| {
        let t = name.peek().trim().to_string();
        let k = *item.peek();
        let title = if t.is_empty() {
            k.label().to_string()
        } else {
            t
        };
        doc.insert(None, &Entry::item(k, &title));
        name.set(String::new());
        app.say(format!("{title} added"));
    };
    rsx! {
        BSheet { label: "Add to the set",
            div { class: "bhead",
                b { "Add to {ctx.set.info().title}" }
                button { onclick: move |_| app.close(), "Done" }
            }
            label { class: "search lib",
                Icon { name: I::Search, size: 16 }
                input {
                    r#type: "search",
                    placeholder: "Search the library",
                    aria_label: "Search the library",
                    value: "{q}",
                    oninput: move |e| q.set(e.value()),
                }
            }
            div { class: "fsec", "{orgs::org_name(&here)}" }
            if mine.is_empty() {
                div { class: "s1", "Nothing called “{needle}”." }
            }
            for e in mine {
                AddRow {
                    key: "{e.id}",
                    ctx: ctx.clone(),
                    owner: here.clone(),
                    song: e.clone(),
                    added: in_set.contains(&orgs::song_ref(&here, &e.id)),
                }
            }
            for owner in owners() {
                SharedAdds {
                    key: "{owner}",
                    ctx: ctx.clone(),
                    owner: owner.clone(),
                    to: here.clone(),
                    needle: needle.clone(),
                    in_set: in_set.clone(),
                }
            }
            div { class: "fsec", "Something else" }
            div { class: "seg wrap",
                for k in [ItemKind::Section].into_iter().chain(ItemKind::ITEMS) {
                    button {
                        key: "{k.as_str()}",
                        class: "fopt",
                        aria_pressed: "{item() == k}",
                        onclick: move |_| item.set(k),
                        Icon { name: item_icon(k), size: 15 }
                        "{k.label()}"
                    }
                }
            }
            div { class: "addrow",
                input {
                    class: "edtitle",
                    placeholder: "{item().label()}: a name (optional)",
                    aria_label: "Name",
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                }
                button { class: "primary", onclick: add_item, "Add" }
            }
        }
    }
}

fn matches(e: &crate::songs::Entry, needle: &str) -> bool {
    needle.is_empty()
        || e.title.to_lowercase().contains(needle)
        || e.artist
            .as_deref()
            .is_some_and(|a| a.to_lowercase().contains(needle))
}

/// What one org shares with this one, to add.
#[component]
fn SharedAdds(ctx: Ctx, owner: String, to: String, needle: String, in_set: Vec<String>) -> Element {
    let digest = crate::team::use_digest(&owner, &to);
    let list: Vec<crate::songs::Entry> = digest
        .entries()
        .into_iter()
        .filter(|e| matches(e, &needle))
        .take(if needle.is_empty() { 4 } else { 20 })
        .collect();
    if list.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "fsec", "Shared by {orgs::org_name(&owner)}" }
        for e in list {
            AddRow {
                key: "{e.id}",
                ctx: ctx.clone(),
                owner: owner.clone(),
                song: e.clone(),
                added: in_set.contains(&orgs::song_ref(&owner, &e.id)),
            }
        }
    }
}

#[component]
fn AddRow(ctx: Ctx, owner: String, song: crate::songs::Entry, added: bool) -> Element {
    let app = use_app();
    let doc = ctx.set.clone();
    let e = Entry::of_song(&owner, &song);
    rsx! {
        button {
            class: "mi big addsong",
            onclick: move |_| {
                doc.insert(None, &e);
                app.say(format!("{} added", e.title));
            },
            span { class: "ic",
                if added {
                    Icon { name: I::Check, size: 18, w: 2.4 }
                } else {
                    Icon { name: I::Plus, size: 18 }
                }
            }
            span { class: "mid",
                span { class: "t1", "{song.title}" }
                span { class: "s1",
                    "{song.artist.clone().unwrap_or_default()}"
                    if added {
                        " · in the set"
                    }
                }
            }
            if let Some(k) = &song.key {
                span { class: "kc", style: "--h:{data::hue(k).unwrap_or(0)}", "{k}" }
            }
        }
    }
}

/// Who plays the set, and what: the org's team, a tap to seat or unseat
/// someone, their parts in this set; or everyone in a group at once.
#[component]
pub fn BandSheet(set: SetRef) -> Element {
    let ctx = use_ctx(&set.owner, &set.id);
    let band = ctx.set.band();
    let own = ctx.own();
    let people = if own { ctx.team.people() } else { Vec::new() };
    let groups = if own { ctx.team.groups() } else { Vec::new() };
    let doc = ctx.set.clone();
    rsx! {
        BSheet { label: "The band",
            div { class: "bhead",
                b { "The band" }
            }
            if !own {
                div { class: "s1", "{orgs::org_name(&ctx.set.org)}'s team plays this set." }
                for s in band.iter() {
                    div { key: "{s.person}", class: "person",
                        span { class: "pmark", style: "--h:{data::str_hue(&s.name)};width:34px;height:34px", "{initials(&s.name)}" }
                        span { class: "mid",
                            span { class: "t1", "{s.name}" }
                            span { class: "s1", "{s.parts.join(\", \")}" }
                        }
                    }
                }
            }
            if !groups.is_empty() {
                div { class: "seg wrap",
                    for g in groups {
                        {
                            let (doc, people, gid) = (doc.clone(), people.clone(), g.id.clone());
                            rsx! {
                                button {
                                    key: "{g.id}",
                                    class: "fopt",
                                    onclick: move |_| {
                                        for s in setlist::band_from_group(&people, &gid) {
                                            doc.put_seat(&s);
                                        }
                                    },
                                    "Everyone in {g.name}"
                                }
                            }
                        }
                    }
                }
            }
            for p in people {
                {
                    let seat = band.iter().find(|s| s.person == p.id).cloned();
                    let on = seat.is_some();
                    let (d1, p1, s1) = (doc.clone(), p.clone(), seat.clone());
                    let parts: Vec<String> = {
                        let mut v = p.parts.clone();
                        for x in seat.iter().flat_map(|s| s.parts.iter()) {
                            if !v.contains(x) {
                                v.push(x.clone());
                            }
                        }
                        v
                    };
                    rsx! {
                        div { key: "{p.id}", class: "bandperson",
                            button {
                                class: "person",
                                onclick: move |_| {
                                    if s1.is_some() {
                                        d1.remove_seat(&p1.id);
                                    } else {
                                        d1.put_seat(&Seat {
                                            person: p1.id.clone(),
                                            name: p1.name.clone(),
                                            parts: p1.parts.clone(),
                                        });
                                    }
                                },
                                span { class: "pmark", style: "--h:{p.hue};width:34px;height:34px", "{p.initials()}" }
                                span { class: "mid",
                                    span { class: "t1", "{p.name}" }
                                    span { class: "s1", "{p.role.label()}" }
                                }
                                if on {
                                    span { class: "ck",
                                        Icon { name: I::Check, size: 18, w: 2.4 }
                                    }
                                }
                            }
                            if let Some(s) = seat {
                                div { class: "chips",
                                    for part in parts {
                                        {
                                            let has = s.parts.contains(&part);
                                            let (doc, s, part2) = (doc.clone(), s.clone(), part.clone());
                                            rsx! {
                                                button {
                                                    key: "{part}",
                                                    class: "tagchip orgchip",
                                                    aria_pressed: "{has}",
                                                    onclick: move |_| {
                                                        let mut x = s.clone();
                                                        if has {
                                                            x.parts.retain(|y| *y != part2);
                                                        } else {
                                                            x.parts.push(part2.clone());
                                                        }
                                                        doc.put_seat(&x);
                                                    },
                                                    "{part}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A set's details: what it is, when and where, who leads it, its notes;
/// saving it as a template.
#[component]
pub fn SetInfoSheet(set: SetRef) -> Element {
    let app = use_app();
    let ctx = use_ctx(&set.owner, &set.id);
    let index = sets::use_index_of(&set.owner);
    let info = ctx.set.info();
    let doc = ctx.set.clone();
    let people = ctx.team.people();
    let groups = ctx.team.groups();
    let put = {
        let doc = doc.clone();
        move |f: &dyn Fn(&mut Info)| {
            let mut i = doc.info();
            f(&mut i);
            doc.put_info(&i);
        }
    };
    let (p1, p2, p3, p4, p5, p6, p7, p8, p9) = (
        put.clone(),
        put.clone(),
        put.clone(),
        put.clone(),
        put.clone(),
        put.clone(),
        put.clone(),
        put.clone(),
        put.clone(),
    );
    let template = {
        let (doc, index, owner) = (doc.clone(), index.clone(), set.owner.clone());
        move |_| {
            let src = doc.peek();
            let i = doc.info();
            let t = Info {
                id: sets::new_id(),
                title: format!("{} (template)", i.title),
                kind: Kind::Template,
                place: i.place.clone(),
                starts: i.starts.clone(),
                call: i.call.clone(),
                group: i.group.clone(),
                size: i.size,
                hue: i.hue,
                ..Info::default()
            };
            sets::create(&owner, &t, Some(&src), &index);
            app.close();
            app.say("Saved as a template");
        }
    };
    rsx! {
        BSheet { label: "The set",
            div { class: "bhead",
                b { "The set" }
                button { onclick: move |_| app.close(), "Done" }
            }
            div { class: "settings",
                label { class: "fieldrow",
                    span { "Title" }
                    input {
                        value: "{info.title}",
                        onchange: move |v| {
                            let t = v.value();
                            p1(&|i| i.title = t.clone());
                        },
                    }
                }
                div { class: "seg wrap", style: "margin:8px 0",
                    for k in Kind::ALL {
                        {
                            let p2 = p2.clone();
                            rsx! {
                                button {
                                    key: "{k.as_str()}",
                                    class: "fopt",
                                    aria_pressed: "{info.kind == k}",
                                    onclick: move |_| p2(&|i| i.kind = k),
                                    "{k.label()}"
                                }
                            }
                        }
                    }
                }
                if info.kind != Kind::Template {
                    label { class: "fieldrow",
                        span { "Date" }
                        input {
                            r#type: "date",
                            value: "{info.date}",
                            onchange: move |v| {
                                let d = v.value();
                                p3(&|i| i.date = d.clone());
                            },
                        }
                    }
                }
                label { class: "fieldrow",
                    span { "Starts" }
                    input {
                        value: "{info.starts}",
                        placeholder: "10:30",
                        onchange: move |v| {
                            let t = v.value();
                            p4(&|i| i.starts = t.clone());
                        },
                    }
                }
                label { class: "fieldrow",
                    span { "Band call" }
                    input {
                        value: "{info.call}",
                        onchange: move |v| {
                            let t = v.value();
                            p5(&|i| i.call = t.clone());
                        },
                    }
                }
                label { class: "fieldrow",
                    span { "Where" }
                    input {
                        value: "{info.place}",
                        onchange: move |v| {
                            let t = v.value();
                            p6(&|i| i.place = t.clone());
                        },
                    }
                }
                label { class: "fieldrow",
                    span { "Leads it" }
                    select {
                        onchange: move |v| {
                            let l = v.value();
                            p7(&|i| i.leader = (!l.is_empty()).then(|| l.clone()));
                        },
                        option { value: "", "Nobody chosen" }
                        for p in people {
                            option { key: "{p.id}", value: "{p.id}", selected: info.leader.as_deref() == Some(p.id.as_str()), "{p.name}" }
                        }
                    }
                }
                label { class: "fieldrow",
                    span { "Played by" }
                    select {
                        onchange: move |v| {
                            let g = v.value();
                            p8(&|i| i.group = g.clone());
                        },
                        option { value: "", "Any group" }
                        for g in groups {
                            option { key: "{g.id}", value: "{g.id}", selected: info.group == g.id, "{g.name}" }
                        }
                    }
                }
                label { class: "fieldrow",
                    span { "Songs it wants" }
                    input {
                        r#type: "number",
                        min: "0",
                        max: "40",
                        value: "{info.size}",
                        onchange: move |v| {
                            let n = v.value().parse().unwrap_or(0);
                            p9(&|i| i.size = n);
                        },
                    }
                }
            }
            div { class: "fsec", "Notes" }
            textarea {
                class: "ednotes",
                rows: "3",
                value: "{info.notes}",
                onchange: move |v| {
                    let n = v.value();
                    put(&|i| i.notes = n.clone());
                },
            }
            if info.kind != Kind::Template {
                button { class: "secondary wide", style: "margin-top:12px", onclick: template,
                    Icon { name: I::Layers, size: 16 }
                    "Save as a template"
                }
            }
        }
    }
}

/// Share a set with other orgs (a joint service).
#[component]
pub fn ShareSetSheet(set: SetRef) -> Element {
    let app = use_app();
    let ctx = use_ctx(&set.owner, &set.id);
    let with = ctx.set.shared_with();
    let orgs_list: Vec<String> = (app.orgs)()
        .into_iter()
        .filter(|o| *o != set.owner)
        .collect();
    rsx! {
        BSheet { label: "Share the set",
            div { class: "bhead",
                b { "Share {ctx.set.info().title}" }
            }
            div { class: "s1",
                "An org it is shared with sees the set and edits it with you, and has it in its calendar. Its songs from {orgs::org_name(&set.owner)}'s library go with it."
            }
            for o in orgs_list {
                {
                    let on = with.contains(&o);
                    let doc = ctx.set.clone();
                    let o2 = o.clone();
                    rsx! {
                        button {
                            key: "{o}",
                            class: "mi big",
                            onclick: move |_| doc.share(&o2, !on),
                            span { class: "omark", style: "--h:{orgs::org_hue(&o)}", "{orgs::org_name(&o).chars().next().unwrap_or('?')}" }
                            "{orgs::org_name(&o)}"
                            if on {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The set as plain text, to copy; the printable page.
#[component]
pub fn ExportSheet(set: SetRef) -> Element {
    let app = use_app();
    let ctx = use_ctx(&set.owner, &set.id);
    let text = plain(&ctx);
    let t2 = text.clone();
    rsx! {
        BSheet { label: "Print or export",
            div { class: "bhead",
                b { "Print or export" }
            }
            pre { class: "plaintext", "{text}" }
            div { class: "edmove",
                button {
                    class: "primary",
                    onclick: move |_| {
                        let js = format!(
                            "navigator.clipboard && navigator.clipboard.writeText({})",
                            serde_json_str(&t2)
                        );
                        document::eval(&js);
                        app.say("Copied as text");
                    },
                    Icon { name: I::Download, size: 16 }
                    "Copy as text"
                }
                button {
                    class: "secondary",
                    onclick: move |_| {
                        app.close();
                        navigator().push(Route::PrintSet {
                            owner: set.owner.clone(),
                            id: set.id.clone(),
                        });
                    },
                    Icon { name: I::Printer, size: 16 }
                    "Printable page"
                }
            }
        }
    }
}

/// A string as a JavaScript literal.
fn serde_json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The set as plain text.
pub fn plain(ctx: &Ctx) -> String {
    let info = ctx.set.info();
    setlist::plain_text(
        &info,
        &ctx.set.entries(),
        &|e| ctx.played(e),
        &ctx.set.band(),
        &|p| ctx.name(p),
    )
}

/// A new set: what it is, for which recurring event's meeting or what
/// date, and what it starts from (nothing, last week's, a template).
/// `preset`: a recurring event's id, or `<series>@<date>`.
#[component]
pub fn NewSetSheet(preset: Option<String>) -> Element {
    let app = use_app();
    let here = (app.here)();
    let index = sets::use_index();
    let series = index.series();
    let faces = index.faces();
    let (p_series, p_date) = match preset.as_deref().map(|p| p.split_once('@')) {
        Some(Some((s, d))) => (Some(s.to_string()), setlist::parse_day(d)),
        Some(None) => (preset.clone(), None),
        None => (None, None),
    };
    let mut for_series = use_signal(move || p_series.clone());
    let mut kind = use_signal(|| Kind::Service);
    let mut title = use_signal(String::new);
    let mut date = use_signal(move || p_date.unwrap_or(TODAY.plus(3)).iso());
    let mut from = use_signal(|| None::<String>);
    let sr = for_series().and_then(|id| series.iter().find(|s| s.id == id).cloned());
    // A meeting: its day (the next one without a set, unless one was given).
    let meeting_day = sr.as_ref().map(|s| {
        p_date
            .filter(|_| {
                preset
                    .as_deref()
                    .is_some_and(|p| p.starts_with(&format!("{}@", s.id)))
            })
            .unwrap_or_else(|| {
                s.dates(TODAY, TODAY.plus(60))
                    .into_iter()
                    .find(|d| {
                        !faces
                            .iter()
                            .any(|f| f.id == setlist::meeting_id(&s.id, *d) && f.songs > 0)
                    })
                    .unwrap_or(TODAY)
            })
    });
    let last = match (&sr, meeting_day) {
        (Some(s), Some(d)) => setlist::previous(&faces, &s.id, d).cloned(),
        _ => None,
    };
    let templates = index.templates();
    let info = match (&sr, meeting_day) {
        (Some(s), Some(d)) => s.meeting(d),
        _ => Info {
            id: sets::new_id(),
            title: title(),
            kind: kind(),
            date: if kind() == Kind::Template {
                String::new()
            } else {
                date()
            },
            hue: data::str_hue(&title()),
            ..Info::default()
        },
    };
    let ready_title = sr.is_some() || !title().trim().is_empty();
    rsx! {
        BSheet { label: "New set",
            div { class: "bhead",
                b { "New set" }
            }
            if !series.is_empty() {
                div { class: "fsec", "For" }
                div { class: "seg wrap",
                    button {
                        class: "fopt",
                        aria_pressed: "{for_series().is_none()}",
                        onclick: move |_| for_series.set(None),
                        "One date"
                    }
                    for s in series.iter() {
                        {
                            let id = s.id.clone();
                            rsx! {
                                button {
                                    key: "{s.id}",
                                    class: "fopt",
                                    aria_pressed: "{for_series().as_deref() == Some(s.id.as_str())}",
                                    onclick: move |_| {
                                        for_series.set(Some(id.clone()));
                                        from.set(None);
                                    },
                                    "{s.name}"
                                }
                            }
                        }
                    }
                }
            }
            if let (Some(s), Some(d)) = (&sr, meeting_day) {
                div { class: "s1 newfor",
                    "{s.name_on(d)}, {DAY[d.weekday()]} {MON[d.month0()]} {d.mday()} · {s.room}"
                }
            } else {
                div { class: "seg wrap", style: "margin-top:10px",
                    for k in Kind::ALL {
                        button {
                            key: "{k.as_str()}",
                            class: "fopt",
                            aria_pressed: "{kind() == k}",
                            onclick: move |_| kind.set(k),
                            "{k.label()}"
                        }
                    }
                }
                div { class: "settings",
                    label { class: "fieldrow",
                        span { "Title" }
                        input {
                            value: "{title}",
                            placeholder: "Christmas Eve, a joint service, a recital",
                            oninput: move |v| title.set(v.value()),
                        }
                    }
                    if kind() != Kind::Template {
                        label { class: "fieldrow",
                            span { "Date" }
                            input {
                                r#type: "date",
                                value: "{date}",
                                oninput: move |v| date.set(v.value()),
                            }
                        }
                    }
                }
            }
            div { class: "fsec", "Start from" }
            button {
                class: "mi big",
                onclick: move |_| from.set(None),
                span { class: "ic",
                    Icon { name: I::Plus, size: 18 }
                }
                "Nothing"
                if from().is_none() {
                    span { class: "ck",
                        Icon { name: I::Check, size: 17, w: 2.2 }
                    }
                }
            }
            if let Some(l) = last {
                {
                    let id = l.id.clone();
                    rsx! {
                        button {
                            class: "mi big",
                            onclick: move |_| from.set(Some(id.clone())),
                            span { class: "ic",
                                Icon { name: I::Loop, size: 18 }
                            }
                            span { class: "mid",
                                span { class: "t1", "Last time's: {l.title}" }
                                span { class: "s1", "{l.date} · {l.first.join(\", \")}" }
                            }
                            if from().as_deref() == Some(l.id.as_str()) {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    }
                }
            }
            for t in templates {
                {
                    let id = t.id.clone();
                    rsx! {
                        button {
                            key: "{t.id}",
                            class: "mi big",
                            onclick: move |_| from.set(Some(id.clone())),
                            span { class: "ic",
                                Icon { name: I::Layers, size: 18 }
                            }
                            span { class: "mid",
                                span { class: "t1", "{t.title}" }
                                span { class: "s1", "{t.songs} songs" }
                            }
                            if from().as_deref() == Some(t.id.as_str()) {
                                span { class: "ck",
                                    Icon { name: I::Check, size: 17, w: 2.2 }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(src) = from() {
                MakeFrom { key: "{src}", here: here.clone(), src, info: info.clone(), ok: ready_title }
            } else {
                button {
                    class: "primary wide",
                    style: "margin-top:12px",
                    disabled: !ready_title,
                    onclick: move |_| {
                        let id = sets::create(&here, &info, None, &index);
                        app.close();
                        navigator().push(Route::SetPage { id });
                    },
                    "Make the set"
                }
            }
        }
    }
}

/// The make button when starting from another set: it waits for that set
/// to arrive.
#[component]
fn MakeFrom(here: String, src: String, info: Info, ok: bool) -> Element {
    let app = use_app();
    let index = sets::use_index();
    let source = sets::use_set(&here, &src);
    let ready = source.ready() && !source.entries().is_empty();
    rsx! {
        button {
            class: "primary wide",
            style: "margin-top:12px",
            disabled: !(ok && ready),
            onclick: move |_| {
                let id = sets::create(&here, &info, Some(&source.peek()), &index);
                app.close();
                navigator().push(Route::SetPage { id });
            },
            if ready {
                "Make the set"
            } else {
                "Opening it…"
            }
        }
    }
}

/// Book a rehearsal: for which set, what day, what time, where. Kept in
/// the set, and in Drive's calendar.
#[component]
pub fn BookSheet() -> Element {
    let app = use_app();
    let here = (app.here)();
    let me = crate::team::use_me();
    let index = sets::use_index();
    let mut booking = app.booking;
    let mut list: Vec<setlist::Face> = index
        .faces()
        .into_iter()
        .filter(|f| f.day().is_some_and(|d| (0..14).contains(&d.from_today())))
        .collect();
    let mine = |f: &setlist::Face| me.as_ref().is_some_and(|m| f.band.contains(m));
    list.sort_by_key(|f| (!mine(f), f.date.clone()));
    list.truncate(6);
    list.sort_by(|a, b| a.date.cmp(&b.date));
    let fallback = list
        .iter()
        .find(|f| mine(f) && f.day().is_some_and(|d| d.from_today() >= 1))
        .or_else(|| {
            list.iter()
                .find(|f| f.day().is_some_and(|d| d.from_today() >= 1))
        })
        .or(list.first())
        .map(|f| Booking {
            set: SetRef::new(&here, &f.id),
            title: f.title.clone(),
            date: f.day(),
            day: f.day().map_or(0, |d| (d.from_today() - 1).max(0) as usize),
            time: "7:00 PM",
            room: "Room 210",
        });
    let Some(b) = booking().or(fallback) else {
        return rsx! {
            BSheet { label: "Book a rehearsal",
                div { class: "bhead",
                    b { "Book a rehearsal" }
                }
                div { class: "s1", "No set in the next two weeks." }
            }
        };
    };
    let last = b.date.map_or(13, |d| d.from_today().max(0) as usize);
    let day = b.day.min(last);
    rsx! {
        BSheet { label: "Book a rehearsal",
            div { class: "bhead",
                b { "Book a rehearsal" }
            }
            div { class: "fsec", "For" }
            div { class: "seg",
                for f in list {
                    {
                        let b = b.clone();
                        let here = here.clone();
                        let on = b.set.id == f.id;
                        rsx! {
                            button {
                                key: "{f.id}",
                                class: "fopt",
                                aria_pressed: "{on}",
                                onclick: move |_| {
                                    booking.set(Some(Booking {
                                        set: SetRef::new(&here, &f.id),
                                        title: f.title.clone(),
                                        date: f.day(),
                                        day: f.day().map_or(0, |d| (d.from_today() - 1).max(0) as usize),
                                        ..b.clone()
                                    }))
                                },
                                "{f.title}"
                                small { "{f.day().map(|d| d.short()).unwrap_or_default()}" }
                            }
                        }
                    }
                }
            }
            div { class: "fsec", "Day" }
            div { class: "seg days",
                for k in 0..=last {
                    {
                        let d = TODAY.plus(k as i32);
                        let top = if k == 0 { "Today".to_string() } else { DAY[d.weekday()][..3].to_string() };
                        let b = b.clone();
                        rsx! {
                            button {
                                key: "{k}",
                                class: "fopt",
                                aria_pressed: "{day == k}",
                                onclick: move |_| booking.set(Some(Booking { day: k, ..b.clone() })),
                                "{top}"
                                small { "{MON[d.month0()]} {d.mday()}" }
                            }
                        }
                    }
                }
            }
            div { class: "fsec", "Time" }
            div { class: "seg",
                for t in ["10:00 AM", "1:00 PM", "6:00 PM", "7:00 PM"] {
                    {
                        let b = b.clone();
                        rsx! {
                            button {
                                key: "{t}",
                                class: "fopt",
                                aria_pressed: "{b.time == t}",
                                onclick: move |_| booking.set(Some(Booking { time: t, ..b.clone() })),
                                "{t}"
                            }
                        }
                    }
                }
            }
            div { class: "fsec", "Where" }
            div { class: "seg",
                for t in ["Room 210", "Chapel", "Worship Center", "The Loft"] {
                    {
                        let b = b.clone();
                        rsx! {
                            button {
                                key: "{t}",
                                class: "fopt",
                                aria_pressed: "{b.room == t}",
                                onclick: move |_| booking.set(Some(Booking { room: t, ..b.clone() })),
                                "{t}"
                            }
                        }
                    }
                }
            }
            BookButtonFor { b: b.clone(), day }
        }
    }
}

/// The Book button: writes into the set's own document.
#[component]
fn BookButtonFor(b: Booking, day: usize) -> Element {
    let app = use_app();
    let set = sets::use_set(&b.set.owner, &b.set.id);
    let index = sets::use_index_of(&b.set.owner);
    rsx! {
        button {
            class: "primary wide",
            onclick: move |_| {
                let r = setlist::Rehearsal {
                    id: String::new(),
                    date: TODAY.plus(day as i32).iso(),
                    time: b.time.into(),
                    room: b.room.into(),
                };
                set.put_rehearsal(&r);
                // The sheet closes now: its face and calendar follow here.
                let d = set.peek();
                index.put_face(&setlist::face_of(&d));
                let i = set.info();
                let evs: Vec<drive_spine::Event> = setlist::rehearsals(&d)
                    .iter()
                    .map(|r| setlist::rehearsal_event(&i, r, sets::BASE))
                    .collect();
                app.sync.put_events(&evs);
                app.close();
                app.say(format!("Rehearsal booked for {}", b.title));
            },
            "Book"
        }
    }
}

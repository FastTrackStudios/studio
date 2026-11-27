//! The orgs: every org at a glance (All orgs), the people of the org
//! Studio is in and what they play, its settings, and the sheets that
//! edit a person and how the org plays a song.

use dioxus::prelude::*;

use crate::Route;
use crate::data::{self, FIFTHS};
use crate::icons::{I, Icon};
use crate::orgs::{self, Group, PARTS, Person, Role, Settings};
use crate::pieces::{Bar, Page, Tool};
use crate::songs::use_library_of;
use crate::state::{Sheet, use_app};
use crate::team::{Team, use_digest, use_owners_sharing, use_team, use_team_of};

/// A round mark in an org's colour.
#[component]
pub fn OrgMark(org: String, #[props(default = 22)] size: u32) -> Element {
    let name = orgs::org_name(&org);
    let h = orgs::org_hue(&org);
    rsx! {
        span {
            class: "omark",
            style: "--h:{h};width:{size}px;height:{size}px;font-size:{size * 11 / 20}px;",
            "{name.chars().next().unwrap_or('?')}"
        }
    }
}

/// A person's round mark.
#[component]
pub fn PersonMark(person: Person, #[props(default = 36)] size: u32) -> Element {
    rsx! {
        span {
            class: "pmark",
            style: "--h:{person.hue};width:{size}px;height:{size}px;font-size:{size * 2 / 5}px;",
            "{person.initials()}"
        }
    }
}

// ── a new organization ──────────────────────────────────────────────────

/// Create an organization: a name, and Create (or Enter). It is made at
/// the account server, the account signed in here its owner, and Studio
/// goes into it (`done` then runs: a sheet closes). What STUDIO_ORG_CREATE
/// does.
#[component]
pub fn NewOrg(#[props(default)] done: Option<EventHandler<()>>) -> Element {
    #[cfg(feature = "drive")]
    {
        let app = use_app();
        let mut name = use_signal(String::new);
        let mut busy = use_signal(|| false);
        let mut said = use_signal(|| None::<String>);
        let mut make = move || {
            if busy() || name().trim().is_empty() {
                return;
            }
            busy.set(true);
            said.set(None);
            let typed = name();
            spawn(async move {
                match crate::org_create::create_here(&typed, None).await {
                    Ok(made) => {
                        crate::org_create::joined(app, &made);
                        app.switch_org(&made.org);
                        name.set(String::new());
                        if let Some(done) = done {
                            done.call(());
                        }
                    }
                    Err(why) => said.set(Some(format!("Not created: {why}."))),
                }
                busy.set(false);
            });
        };
        rsx! {
            div { class: "addrow",
                input {
                    class: "edtitle",
                    placeholder: "New organization: its name",
                    aria_label: "New organization's name",
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key().to_string() == "Enter" {
                            make();
                        }
                    },
                }
                button {
                    class: "primary",
                    disabled: busy() || name().trim().is_empty(),
                    onclick: move |_| make(),
                    if busy() { "Creating…" } else { "Create" }
                }
            }
            if let Some(s) = said() {
                div { class: "s1", role: "alert", "{s}" }
            }
        }
    }
    #[cfg(not(feature = "drive"))]
    {
        let _ = done;
        rsx! {}
    }
}

// ── All orgs ────────────────────────────────────────────────────────────

#[component]
pub fn AllOrgs() -> Element {
    let app = use_app();
    let list = (app.orgs)();
    let here = (app.here)();
    rsx! {
        Page {
            Bar { title: "All orgs" }
            div { class: "body",
                p { class: "lede",
                    "Every org you are in: its songs, its people, what it shares and what is shared with it. Each keeps its own library; a song belongs to one org and can be shared with others."
                }
                div { class: "orgcards",
                    for o in list {
                        OrgCard { key: "{o}", here: o == here, org: o }
                    }
                }
                NewOrg {}
            }
        }
    }
}

/// One org: its songs, people and leaders, live from its documents.
#[component]
fn OrgCard(org: String, here: bool) -> Element {
    let app = use_app();
    let lib = use_library_of(&org);
    let team = use_team_of(&org);
    let incoming = use_owners_sharing(&org);
    let songs = lib.entries();
    let people = team.people();
    let leaders: Vec<String> = people
        .iter()
        .filter(|p| p.leading())
        .map(|p| p.name.clone())
        .collect();
    let out: usize = songs.iter().filter(|e| !e.shared_with.is_empty()).count();
    let mut to: Vec<String> = songs
        .iter()
        .flat_map(|e| e.shared_with.iter().cloned())
        .collect();
    to.sort();
    to.dedup();
    let from = incoming();
    let name = team.name();
    let go = org.clone();
    rsx! {
        div { class: if here { "orgcard here" } else { "orgcard" },
            div { class: "orghead",
                OrgMark { org: org.clone(), size: 34 }
                span { class: "mid",
                    span { class: "t1", "{name}" }
                    span { class: "s1", "{org}" }
                }
                if here {
                    span { class: "herechip", "Here" }
                }
            }
            div { class: "orgstats",
                span {
                    b { "{songs.len()}" }
                    if songs.len() == 1 { " song" } else { " songs" }
                }
                span {
                    b { "{people.len()}" }
                    if people.len() == 1 { " person" } else { " people" }
                }
                span {
                    b { "{out}" }
                    " shared"
                }
            }
            if !leaders.is_empty() {
                div { class: "s1", "Led by {leaders.join(\", \")}" }
            }
            if !to.is_empty() {
                div { class: "sharerow",
                    span { class: "s1", "Shares with" }
                    for o in to {
                        span { key: "{o}", class: "orgtag", style: "--h:{orgs::org_hue(&o)}", "{orgs::org_name(&o)}" }
                    }
                }
            }
            if !from.is_empty() {
                div { class: "sharerow",
                    span { class: "s1", "Gets songs from" }
                    for o in from {
                        span { key: "{o}", class: "orgtag", style: "--h:{orgs::org_hue(&o)}", "{orgs::org_name(&o)}" }
                    }
                }
            }
            button {
                class: if here { "secondary wide" } else { "primary wide" },
                onclick: move |_| {
                    app.switch_org(&go);
                    navigator().push(Route::LibraryHome {});
                },
                if here {
                    "Open its library"
                } else {
                    "Switch to {name}"
                }
            }
        }
    }
}

// ── People ──────────────────────────────────────────────────────────────

#[component]
pub fn People() -> Element {
    let app = use_app();
    let team = use_team();
    let who = (app.who)();
    let mut group = use_signal(|| None::<String>);
    let groups = team.groups();
    let me = team.me(&who).map(|p| p.id);
    let people: Vec<Person> = team
        .people()
        .into_iter()
        .filter(|p| group().is_none_or(|g| p.groups.contains(&g)))
        .collect();
    let mut name = use_signal(String::new);
    let team2 = team.clone();
    rsx! {
        Page {
            Bar {
                title: "People",
                tools: rsx! {
                    Tool { label: "Add someone", icon: I::Plus, onclick: move |_| app.open(Sheet::Person(String::new())) }
                },
            }
            div { class: "body",
                p { class: "lede",
                    "Who makes the music at {team.name()}: what each plays, who leads. A person with an account is matched to whoever signs in with it."
                }
                if !groups.is_empty() {
                    div { class: "tagrow",
                        button {
                            class: "tagchip",
                            aria_pressed: "{group().is_none()}",
                            onclick: move |_| group.set(None),
                            "Everyone"
                        }
                        for g in groups.iter().cloned() {
                            {
                                let on = group().as_deref() == Some(g.id.as_str());
                                rsx! {
                                    button {
                                        key: "{g.id}",
                                        class: "tagchip",
                                        aria_pressed: "{on}",
                                        onclick: move |_| group.set(if on { None } else { Some(g.id.clone()) }),
                                        "{g.name}"
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(g) = group().and_then(|g| groups.iter().find(|x| x.id == g).cloned()) {
                    GroupLine { team: team.clone(), group: g }
                }
                div { class: "list people",
                    for p in people {
                        PersonRow { key: "{p.id}", me: me.as_deref() == Some(p.id.as_str()), groups: groups.clone(), person: p }
                    }
                }
                label { class: "newsong",
                    Icon { name: I::Plus, size: 16 }
                    input {
                        placeholder: "Add someone by name",
                        value: "{name}",
                        aria_label: "New person's name",
                        oninput: move |e| name.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            let n = name().trim().to_string();
                            if e.key().to_string() == "Enter" && !n.is_empty() {
                                let id = team2.add_person(&n);
                                if let Some(g) = group() {
                                    if let Some(mut p) = team2.person(&id) {
                                        p.groups.push(g);
                                        team2.put_person(&p);
                                    }
                                }
                                name.set(String::new());
                                app.open(Sheet::Person(id));
                            }
                        },
                    }
                }
            }
        }
    }
}

/// A group's line: what kind it is, who leads it, the parts it has.
#[component]
fn GroupLine(team: Team, group: Group) -> Element {
    let doc = team.loro();
    let leads: Vec<String> = orgs::leaders(&doc, &group.id)
        .into_iter()
        .map(|p| p.name)
        .collect();
    let covered: Vec<&str> = PARTS
        .iter()
        .copied()
        .filter(|part| !orgs::playing(&doc, part, Some(&group.id)).is_empty())
        .collect();
    rsx! {
        div { class: "groupline",
            span { class: "s1", "{data::cap(&group.kind)}" }
            if !leads.is_empty() {
                span { class: "s1", " · led by {leads.join(\", \")}" }
            }
            div { class: "parts",
                for part in covered {
                    span { key: "{part}", class: "part", "{part}" }
                }
            }
        }
    }
}

#[component]
fn PersonRow(person: Person, me: bool, groups: Vec<Group>) -> Element {
    let app = use_app();
    let id = person.id.clone();
    let leads: Vec<String> = person
        .leads
        .iter()
        .filter_map(|g| groups.iter().find(|x| &x.id == g).map(|x| x.name.clone()))
        .collect();
    let sub = if person.parts.is_empty() {
        "No part yet".to_string()
    } else {
        person.parts.join(" · ")
    };
    rsx! {
        button {
            class: "person",
            onclick: move |_| app.open(Sheet::Person(id.clone())),
            PersonMark { person: person.clone() }
            span { class: "mid",
                span { class: "t1",
                    "{person.name}"
                    if me {
                        span { class: "youchip", "You" }
                    }
                }
                span { class: "s1", "{sub}" }
            }
            span { class: "pright",
                span { class: if person.leading() { "role lead" } else { "role" }, "{person.role.label()}" }
                if !leads.is_empty() {
                    span { class: "leads", "Leads {leads.join(\", \")}" }
                }
            }
        }
    }
}

/// Edit a person: name, role, parts, groups, what they lead, their account.
#[component]
pub fn PersonSheet(id: String) -> Element {
    let app = use_app();
    let team = use_team();
    let who = (app.who)();
    let groups = team.groups();
    let Some(p) = (if id.is_empty() {
        None
    } else {
        team.person(&id)
    }) else {
        return rsx! {
            NewPerson { team }
        };
    };
    let put = {
        let team = team.clone();
        move |f: &dyn Fn(&mut Person)| {
            if let Some(mut x) = team.person(&id) {
                f(&mut x);
                team.put_person(&x);
            }
        }
    };
    let put_name = put.clone();
    let mine = !who.is_empty() && p.account.as_deref() == Some(who.as_str());
    rsx! {
        div { class: "bsheet person-sheet", role: "dialog", aria_label: "Person",
            div { class: "phead",
                PersonMark { person: p.clone(), size: 48 }
                input {
                    class: "sename",
                    value: "{p.name}",
                    aria_label: "Name",
                    onchange: move |e| {
                        let v = e.value();
                        put_name(&|x: &mut Person| x.name = v.trim().to_string());
                    },
                }
            }
            div { class: "songh", "Role" }
            div { class: "chips",
                for r in Role::ALL {
                    {
                        let put = put.clone();
                        rsx! {
                            button {
                                key: "{r.as_str()}",
                                class: "tagchip",
                                aria_pressed: "{p.role == r}",
                                onclick: move |_| put(&|x: &mut Person| x.role = r),
                                "{r.label()}"
                            }
                        }
                    }
                }
            }
            div { class: "songh", "Plays" }
            div { class: "chips",
                for part in PARTS {
                    {
                        let put = put.clone();
                        let on = p.parts.iter().any(|x| x == part);
                        rsx! {
                            button {
                                key: "{part}",
                                class: "tagchip",
                                aria_pressed: "{on}",
                                onclick: move |_| put(&|x: &mut Person| toggle(&mut x.parts, part)),
                                "{part}"
                            }
                        }
                    }
                }
            }
            if !groups.is_empty() {
                div { class: "songh", "In" }
                div { class: "grouprows",
                    for g in groups {
                        {
                            let (put_in, put_lead) = (put.clone(), put.clone());
                            let inside = p.groups.contains(&g.id);
                            let leads = p.leads.contains(&g.id);
                            let (gi, gl) = (g.id.clone(), g.id.clone());
                            rsx! {
                                div { key: "{g.id}", class: "grouprow",
                                    button {
                                        class: "tagchip",
                                        aria_pressed: "{inside}",
                                        onclick: move |_| put_in(&|x: &mut Person| {
                                            toggle(&mut x.groups, &gi);
                                            if !x.groups.contains(&gi) {
                                                x.leads.retain(|l| l != &gi);
                                            }
                                        }),
                                        "{g.name}"
                                    }
                                    if inside {
                                        button {
                                            class: if leads { "leadbtn on" } else { "leadbtn" },
                                            onclick: move |_| put_lead(&|x: &mut Person| toggle(&mut x.leads, &gl)),
                                            if leads {
                                                "Leads it"
                                            } else {
                                                "Make leader"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "songh", "Account" }
            div { class: "s1 acct",
                match (&p.account, mine) {
                    (Some(_), true) => rsx! { "This is you (your account)." },
                    (Some(a), false) => rsx! { "Matched to {a}." },
                    (None, _) => rsx! { "No account: they are on the team without signing in." },
                }
            }
            if !who.is_empty() && !mine {
                {
                    let (team, pid, who) = (team.clone(), p.id.clone(), who.clone());
                    rsx! {
                        button { class: "secondary wide", onclick: move |_| team.link_account(&pid, &who), "This is me" }
                    }
                }
            }
            {
                let (team, pid) = (team.clone(), p.id.clone());
                rsx! {
                    button {
                        class: "secondary wide danger",
                        onclick: move |_| {
                            team.remove_person(&pid);
                            app.close();
                        },
                        "Remove from {team.name()}"
                    }
                }
            }
        }
    }
}

fn toggle(list: &mut Vec<String>, v: &str) {
    if let Some(i) = list.iter().position(|x| x == v) {
        list.remove(i);
    } else {
        list.push(v.to_string());
    }
}

#[component]
fn NewPerson(team: Team) -> Element {
    let app = use_app();
    let mut name = use_signal(String::new);
    rsx! {
        div { class: "bsheet", role: "dialog", aria_label: "Add someone",
            div { class: "bhead",
                b { "Add someone to {team.name()}" }
            }
            input {
                class: "sename",
                placeholder: "Their name",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            button {
                class: "primary wide",
                onclick: move |_| {
                    let n = name().trim().to_string();
                    if !n.is_empty() {
                        let id = team.add_person(&n);
                        app.open(Sheet::Person(id));
                    }
                },
                "Add"
            }
        }
    }
}

// ── the org's settings ──────────────────────────────────────────────────

#[component]
pub fn OrgSettings() -> Element {
    let app = use_app();
    let team = use_team();
    let here = (app.here)();
    let st = team.settings();
    let groups = team.groups();
    let mut new_group = use_signal(String::new);
    let save = {
        let team = team.clone();
        move |f: &dyn Fn(&mut Settings)| {
            let mut s = team.settings();
            f(&mut s);
            team.put_settings(&s);
        }
    };
    let (s1, s2, s3) = (save.clone(), save.clone(), save.clone());
    let team2 = team.clone();
    rsx! {
        Page {
            Bar { title: "Organization" }
            div { class: "body settings",
                div { class: "orghead big",
                    OrgMark { org: here.clone(), size: 44 }
                    span { class: "mid",
                        span { class: "t1", "{team.name()}" }
                        span { class: "s1", "{here}" }
                    }
                }
                label { class: "fieldrow",
                    span { "Name in Studio" }
                    input {
                        value: "{st.name}",
                        placeholder: "{orgs::org_name(&here)}",
                        onchange: move |e| {
                            let v = e.value();
                            s1(&|s: &mut Settings| s.name = v.trim().to_string());
                        },
                    }
                }
                label { class: "fieldrow",
                    span { "Default arrangement" }
                    select {
                        value: "{st.arrangement}",
                        onchange: move |e| {
                            let v = e.value();
                            s2(&|s: &mut Settings| s.arrangement = v.clone());
                        },
                        for (a , _) in data::ARRS {
                            option { key: "{a}", value: "{a}", selected: st.arrangement == a, "{a}" }
                        }
                    }
                }
                label { class: "fieldrow",
                    span { "Rehearses in" }
                    input {
                        value: "{st.room}",
                        placeholder: "A room",
                        onchange: move |e| {
                            let v = e.value();
                            s3(&|s: &mut Settings| s.room = v.trim().to_string());
                        },
                    }
                }
                div { class: "songh", "Groups" }
                div { class: "list",
                    for g in groups {
                        {
                            let leads: Vec<String> = orgs::leaders(&team.loro(), &g.id).into_iter().map(|p| p.name).collect();
                            let n = team.people().iter().filter(|p| p.groups.contains(&g.id)).count();
                            rsx! {
                                div { key: "{g.id}", class: "tagrowx",
                                    span { class: "hash", "#" }
                                    span { class: "t1", "{g.name}" }
                                    span { class: "n",
                                        "{n} in it"
                                        if !leads.is_empty() {
                                            " · led by {leads.join(\", \")}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                label { class: "newsong",
                    Icon { name: I::Plus, size: 16 }
                    input {
                        placeholder: "New group (a band, a team, a class)",
                        value: "{new_group}",
                        oninput: move |e| new_group.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            let n = new_group().trim().to_string();
                            if e.key().to_string() == "Enter" && !n.is_empty() {
                                team2.put_group(&Group {
                                    id: session_sync::slug::slugify(&n),
                                    name: n,
                                    kind: "band".into(),
                                });
                                new_group.set(String::new());
                            }
                        },
                    }
                }
            }
        }
    }
}

// ── how the org plays a song ────────────────────────────────────────────

/// Its key, its arrangement and who leads it here; for a song this org
/// owns, who it is shared with.
#[component]
pub fn OrgSongSheet(owner: String, id: String) -> Element {
    let app = use_app();
    let here = (app.here)();
    let lib = crate::songs::use_library();
    let team = use_team();
    let digest = use_digest(&owner, &here);
    let own = owner == here;
    let entry = if own { lib.get(&id) } else { digest.get(&id) };
    let Some(e) = entry else {
        return rsx! {
            div { class: "bsheet", "This song is not shared with {team.name()}." }
        };
    };
    let u = lib.use_of(&owner, &id);
    let st = team.settings();
    let key = orgs::key_here(&u, &e).unwrap_or_default();
    let arr = orgs::arrangement_here(&u, &st);
    let leaders: Vec<Person> = team.people();
    let put = {
        let (lib, owner, id) = (lib.clone(), owner.clone(), id.clone());
        move |f: &dyn Fn(&mut orgs::Use)| {
            let mut u = lib.use_of(&owner, &id);
            f(&mut u);
            lib.put_use(&owner, &id, &u);
        }
    };
    let (pk, pa, pl) = (put.clone(), put.clone(), put.clone());
    let others: Vec<String> = (app.orgs)().into_iter().filter(|o| *o != here).collect();
    let song_key = e.key.clone().unwrap_or_default();
    rsx! {
        div { class: "bsheet", role: "dialog", aria_label: "In this org",
            div { class: "bhead",
                b { "{e.title} at {team.name()}" }
            }
            if !own {
                div { class: "s1", "Shared by {orgs::org_name(&owner)}: its form and chart are theirs; the key, arrangement and leader here are {team.name()}'s." }
            }
            div { class: "songh", "Key here" }
            div { class: "chips keys",
                for k in FIFTHS {
                    {
                        let pk = pk.clone();
                        let song_key = song_key.clone();
                        rsx! {
                            button {
                                key: "{k}",
                                class: "tagchip",
                                aria_pressed: "{key == k}",
                                onclick: move |_| {
                                    let v = (k != song_key).then(|| k.to_string());
                                    pk(&|u: &mut orgs::Use| u.key = v.clone());
                                },
                                "{k}"
                            }
                        }
                    }
                }
            }
            div { class: "songh", "Arrangement here" }
            div { class: "chips",
                for (a , _) in data::ARRS {
                    {
                        let pa = pa.clone();
                        rsx! {
                            button {
                                key: "{a}",
                                class: "tagchip",
                                aria_pressed: "{arr == a}",
                                onclick: move |_| {
                                    pa(&|u: &mut orgs::Use| {
                                        u.arrangement = Some(a.to_string());
                                        if !u.arrangements.iter().any(|x| x == a) {
                                            u.arrangements.push(a.to_string());
                                        }
                                    })
                                },
                                "{a}"
                            }
                        }
                    }
                }
            }
            div { class: "songh", "Who leads it here" }
            div { class: "chips",
                for p in leaders {
                    {
                        let pl = pl.clone();
                        let on = u.leader.as_deref() == Some(p.id.as_str());
                        let pid = p.id.clone();
                        rsx! {
                            button {
                                key: "{p.id}",
                                class: "tagchip",
                                aria_pressed: "{on}",
                                onclick: move |_| {
                                    let v = (!on).then(|| pid.clone());
                                    pl(&|u: &mut orgs::Use| u.leader = v.clone());
                                },
                                "{p.name}"
                            }
                        }
                    }
                }
            }
            if own {
                div { class: "songh", "Shared with" }
                div { class: "chips",
                    for o in others {
                        {
                            let lib = lib.clone();
                            let on = e.shared_with.contains(&o);
                            let id = id.clone();
                            rsx! {
                                button {
                                    key: "{o}",
                                    class: "tagchip orgchip",
                                    style: "--h:{orgs::org_hue(&o)}",
                                    aria_pressed: "{on}",
                                    onclick: move |_| lib.share(&id, &o, !on),
                                    "{orgs::org_name(&o)}"
                                }
                            }
                        }
                    }
                }
                div { class: "s1", "A shared song stays {team.name()}'s: the other org sees it, opens its session, and keeps its own key and arrangement." }
            }
        }
    }
}

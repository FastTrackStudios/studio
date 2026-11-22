//! A song's Files section (grouped by kind, each file with where it is
//! and a preview), finding a song's files across every computer, and a
//! set's readiness (per song and overall, with keeping it on Drive or
//! warming it here).

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::files::{self, FileKind, Hit, NewLink, Proposal, Ready, SongReady};
use crate::files_live::{Row, SongFiles, use_rows, use_song_files};
use crate::icons::{I, Icon};
use crate::songs::Entry;
use crate::state::use_app;

fn kind_icon(k: FileKind) -> I {
    match k {
        FileKind::ChartPdf | FileKind::Keyflow => I::Chart,
        FileKind::Lyrics => I::Type,
        FileKind::Click => I::Metronome,
        FileKind::Guide => I::Cues,
        FileKind::Backing => I::Play,
        FileKind::Stems => I::Layers,
        FileKind::Multitrack => I::Sliders,
        FileKind::Reference => I::Wave,
        FileKind::Midi => I::Keys,
        FileKind::Session => I::Disc,
        FileKind::Video => I::Video,
        FileKind::Other => I::Folder,
    }
}

fn ready_class(r: Option<Ready>) -> &'static str {
    match r {
        Some(Ready::Here) => "rdy here",
        Some(Ready::Kept) => "rdy kept",
        Some(Ready::Reachable) => "rdy reach",
        Some(Ready::Offline) => "rdy off",
        Some(Ready::Missing) => "rdy miss",
        None => "rdy none",
    }
}

/// A readiness badge.
#[component]
pub fn ReadyBadge(r: Option<Ready>, #[props(default)] label: Option<String>) -> Element {
    let text = label.unwrap_or_else(|| r.map_or("No files", Ready::label).to_string());
    rsx! {
        span { class: ready_class(r), "{text}" }
    }
}

/// Where a file is, in a few words: the computer, the folder, whether it
/// is on, and whether Drive keeps it.
fn where_line(h: &Hit) -> String {
    if h.location == files::TREE {
        return "In Drive".into();
    }
    let dev = if h.device_name.is_empty() {
        "A computer".to_string()
    } else {
        h.device_name.clone()
    };
    let loc = h
        .location_name
        .strip_prefix(&format!("{dev} · "))
        .unwrap_or(&h.location_name)
        .to_string();
    let mut s = if loc.is_empty() || loc == dev {
        dev
    } else {
        format!("{dev} · {loc}")
    };
    s.push_str(if h.online { " · on" } else { " · off" });
    s
}

// ── a song's Files section ──────────────────────────────────────────────

#[component]
pub fn SongFilesSection(entry: Entry, owner: String) -> Element {
    let sf = use_song_files(&owner, &entry.id);
    let refresh = use_signal(|| 0u64);
    let rows = use_rows(&sf, refresh);
    let mut finding = use_signal(|| false);
    let mut status = use_signal(String::new);
    let links = sf.links();
    let own = sf.own();
    let list = rows.read().clone();
    // Group by the link's kind (a folder's files stay under it).
    let mut groups: BTreeMap<FileKind, Vec<files::Link>> = BTreeMap::new();
    for l in &links {
        groups.entry(l.kind).or_default().push(l.clone());
    }
    let total: i64 = list
        .as_ref()
        .map_or(0, |r| r.iter().map(|x| x.hit.size).sum());
    let nfiles = list.as_ref().map_or(0, Vec::len);
    let upload = {
        let (sf, entry) = (sf.clone(), entry.clone());
        move |ev: FormEvent| {
            let (sf, entry) = (sf.clone(), entry.clone());
            let mut refresh = refresh;
            spawn(async move {
                for f in ev.files() {
                    let name = f.name();
                    if f.size() > 50 * 1024 * 1024 {
                        status.set(format!(
                            "{name} is over 50 MB: link it where it is instead."
                        ));
                        continue;
                    }
                    status.set(format!("Uploading {name}…"));
                    match upload_one(&sf, &entry, &f).await {
                        Ok(()) => status.set(format!("{name} is in Drive, linked to the song.")),
                        Err(e) => status.set(format!("{name}: {e}")),
                    }
                }
                refresh += 1;
            });
        }
    };
    rsx! {
        div { class: "sf",
            div { class: "sfhead",
                h2 { class: "songh", "Files" }
                span { class: "s1",
                    if links.is_empty() {
                        "None linked yet"
                    } else {
                        "{files::count(nfiles)} · {files::bytes(total)}"
                    }
                }
                span { class: "grow" }
                if own {
                    button {
                        class: if finding() { "sfbtn on" } else { "sfbtn" },
                        onclick: move |_| finding.toggle(),
                        Icon { name: I::Search, size: 14, w: 2.2 }
                        "Find files"
                    }
                    label { class: "sfbtn",
                        Icon { name: I::Import, size: 14, w: 2.2 }
                        "Upload"
                        input {
                            r#type: "file",
                            multiple: true,
                            style: "display:none",
                            onchange: upload,
                        }
                    }
                }
            }
            if !status().is_empty() {
                p { class: "hint sfstatus", "{status}" }
            }
            if finding() && own {
                FindPanel { sf: sf.clone(), entry: entry.clone(), onlinked: move |_| {
                    let mut r = refresh;
                    r += 1;
                } }
            }
            if links.is_empty() && !finding() {
                p { class: "empty sfempty",
                    if own {
                        "Link this song's charts, lyrics, click and guide, stems and sessions where they already are, on any computer, or upload a chart or LRC here."
                    } else {
                        "{crate::orgs::org_name(&owner)} has linked no files to this song."
                    }
                }
            }
            for (kind , ls) in groups {
                div { key: "{kind.id()}", class: "sfgroup",
                    div { class: "sfgh",
                        Icon { name: kind_icon(kind), size: 15, w: 2.0 }
                        "{kind.name()}"
                    }
                    for l in ls {
                        LinkBlock {
                            key: "{l.part}",
                            sf: sf.clone(),
                            link: l.clone(),
                            rows: list.as_ref().map(|r| r.iter().filter(|x| x.link.part == l.part).cloned().collect::<Vec<_>>()),
                        }
                    }
                }
            }
            if !links.is_empty() {
                Aliases { sf: sf.clone() }
            }
        }
    }
}

#[cfg(feature = "web")]
async fn upload_one(
    sf: &SongFiles,
    entry: &Entry,
    f: &dioxus::html::FileData,
) -> Result<(), String> {
    let b = f.read_bytes().await.map_err(|e| e.to_string())?;
    let hash = crate::files_live::web::put_blob(&b).await?;
    sf.add_uploaded(entry, &f.name(), b.len() as u64, &hash)
        .map(|_| ())
        .ok_or_else(|| "could not add it to the song".to_string())
}

#[cfg(not(feature = "web"))]
async fn upload_one(
    sf: &SongFiles,
    entry: &Entry,
    f: &dioxus::html::FileData,
) -> Result<(), String> {
    // No Drive node here: the file is linked where it is on this device.
    let path = f.path();
    if path.as_os_str().is_empty() {
        return Err("no Drive on this device yet".into());
    }
    sf.link(
        entry,
        NewLink {
            space: sf.owner.clone(),
            location: "this-device".into(),
            path: path.display().to_string(),
            dir: false,
            kind: None,
        },
    )
    .map(|_| ())
    .ok_or_else(|| "could not link it".into())
}

/// One link: a file, or a folder with its files under it.
#[component]
fn LinkBlock(sf: SongFiles, link: files::Link, rows: Option<Vec<Row>>) -> Element {
    let mut open = use_signal(|| !link.dir);
    let own = sf.own();
    let n = rows.as_ref().map_or(0, Vec::len);
    let size: i64 = rows
        .as_ref()
        .map_or(0, |r| r.iter().map(|x| x.hit.size).sum());
    let missing = rows.as_ref().is_some_and(|r| r.iter().all(|x| x.missing));
    let first = rows.as_ref().and_then(|r| r.first().cloned());
    let (sf2, part) = (sf.clone(), link.part.clone());
    let (sf3, part3) = (sf.clone(), link.part.clone());
    rsx! {
        div { class: if link.dir { "sflink dir" } else { "sflink" },
            if link.dir {
                div { class: "sfrow sffolder",
                    button { class: "sfopen", onclick: move |_| open.toggle(), aria_label: "Show files",
                        Icon { name: if open() { I::ChevDown } else { I::ChevRight }, size: 14 }
                    }
                    Icon { name: I::Folder, size: 18 }
                    span { class: "mid",
                        span { class: "t1", "{link.name()}" }
                        span { class: "s1",
                            if let Some(f) = &first {
                                "{where_line(&f.hit)} · "
                            }
                            if missing {
                                "not found in any index"
                            } else if rows.is_none() {
                                "listing…"
                            } else {
                                "{files::count(n)} · {files::bytes(size)}"
                            }
                            if link.available {
                                " · kept on Drive"
                            }
                        }
                    }
                    if own {
                        KeepToggle { sf: sf.clone(), part: link.part.clone(), on: link.available }
                        KindPick { value: link.kind, onpick: move |k| sf2.set_kind(&part, k) }
                        button { class: "rb danger", title: "Unlink (the folder stays where it is)", aria_label: "Unlink", onclick: move |_| sf3.unlink(&part3), "✕" }
                    }
                }
                if open() {
                    div { class: "sfkids",
                        for r in rows.clone().unwrap_or_default().into_iter().take(200) {
                            FileRow { key: "{r.hit.path}", sf: sf.clone(), row: r, nested: true }
                        }
                    }
                }
            } else {
                match rows.as_ref().and_then(|r| r.first().cloned()) {
                    Some(r) => rsx! {
                        FileRow { sf: sf.clone(), row: r, nested: false }
                    },
                    None => rsx! {
                        div { class: "sfrow",
                            Icon { name: kind_icon(link.kind), size: 18 }
                            span { class: "mid",
                                span { class: "t1", "{link.name()}" }
                                span { class: "s1", "listing…" }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// Keep a link on Drive (the node), so it can be had whatever is off.
#[component]
fn KeepToggle(sf: SongFiles, part: String, on: bool) -> Element {
    rsx! {
        button {
            class: if on { "rb keep on" } else { "rb keep" },
            title: if on { "Kept on Drive: stop keeping it" } else { "Keep on Drive, reachable when its computer is off" },
            aria_label: "Keep on Drive",
            onclick: move |_| sf.keep(std::slice::from_ref(&part), !on),
            Icon { name: I::Pin, size: 13, w: 2.2 }
        }
    }
}

#[component]
fn KindPick(value: FileKind, onpick: EventHandler<FileKind>) -> Element {
    rsx! {
        select {
            class: "sfkind",
            aria_label: "Kind",
            onchange: move |ev| {
                if let Some(k) = FileKind::parse(&ev.value()) {
                    onpick.call(k);
                }
            },
            for k in FileKind::ALL {
                option { key: "{k.id()}", value: k.id(), selected: k == value, "{k.name()}" }
            }
        }
    }
}

/// A file: its name, where it is, its kind, and a preview.
#[component]
fn FileRow(sf: SongFiles, row: Row, nested: bool) -> Element {
    let mut open = use_signal(|| false);
    let own = sf.own();
    let h = row.hit.clone();
    let audio = files::is_audio(&h.name);
    let (sf2, part) = (sf.clone(), row.link.part.clone());
    let (sf3, target) = (sf.clone(), row.target());
    // Off here: another copy may be on a computer that is on.
    let h5 = row.hit.clone();
    let check = !row.missing && !h5.online && h5.available.is_none() && h5.copies > 1;
    let others = use_resource(move || {
        let h = h5.clone();
        async move {
            if check {
                copies_of(&h).await
            } else {
                Vec::new()
            }
        }
    });
    let state = row.state(&others.read().clone().unwrap_or_default());
    let r = files::file_ready(&state);
    rsx! {
        div { class: if nested { "sfrow nested" } else { "sfrow" },
            button {
                class: "sfplay",
                aria_label: if audio { "Play" } else { "Preview" },
                disabled: row.missing,
                onclick: move |_| open.toggle(),
                Icon { name: if audio { I::Play } else { kind_icon(row.kind) }, size: 16, w: 2.0 }
            }
            span { class: "mid",
                span { class: "t1", "{h.name}" }
                span { class: "s1",
                    if row.missing {
                        "not found in any index"
                    } else {
                        "{where_line(&h)}"
                        if h.size > 0 {
                            " · {files::bytes(h.size)}"
                        }
                        if h.copies > 1 {
                            " · {h.copies} copies"
                        }
                    }
                }
            }
            ReadyBadge { r: Some(r) }
            if own && nested {
                KindPick { value: row.kind, onpick: move |k| sf3.set_file_kind(&target, Some(k)) }
            } else if own {
                if !row.link.in_tree() {
                    KeepToggle { sf: sf.clone(), part: row.link.part.clone(), on: row.link.available }
                }
                KindPick { value: row.kind, onpick: move |k| sf2.set_kind(&part, k) }
                {
                    let (sf4, part4) = (sf.clone(), row.link.part.clone());
                    rsx! {
                        button { class: "rb danger", title: "Unlink (the file stays where it is)", aria_label: "Unlink", onclick: move |_| sf4.unlink(&part4), "✕" }
                    }
                }
            } else {
                span { class: "sfchip", "{row.kind.name()}" }
            }
        }
        if open() {
            Preview { hit: h.clone(), kind: row.kind }
        }
    }
}

/// A file's preview: a waveform and a player for audio, the first page
/// and Open for a PDF, a picture for an image, and where its copies are.
#[component]
pub fn Preview(hit: Hit, kind: FileKind) -> Element {
    #[cfg(feature = "web")]
    {
        use crate::files_live::web;
        let audio = files::is_audio(&hit.name);
        let ext = files::extension(&hit.name);
        let url = web::file_url(&hit);
        let h2 = hit.clone();
        let wave = use_resource(move || {
            let h = h2.clone();
            async move {
                if files::is_audio(&h.name) {
                    Some(web::peaks(&h, 240).await)
                } else {
                    None
                }
            }
        });
        let h3 = hit.clone();
        let copies = use_resource(move || {
            let h = h3.clone();
            async move { web::copies(&h).await }
        });
        let thumb = (ext == "pdf"
            || matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif")
            || kind == FileKind::Video)
            .then(|| web::sidecar_url(&hit, "thumb-1024"))
            .flatten();
        let text = matches!(ext.as_str(), "lrc" | "kf" | "txt" | "md");
        let h4 = hit.clone();
        let body = use_resource(move || {
            let h = h4.clone();
            async move {
                if !text || h.size > 256 * 1024 {
                    return None;
                }
                crate::files_live::web::text(&h).await
            }
        });
        rsx! {
            div { class: "sfprev",
                if audio {
                    match &*wave.read() {
                        Some(Some(Ok((cols, secs)))) => rsx! {
                            Waveform { cols: cols.clone() }
                            span { class: "s1", "{crate::songs::clock(*secs)}" }
                        },
                        Some(Some(Err(why))) => rsx! {
                            p { class: "s1", "No waveform: {why}" }
                        },
                        _ => rsx! {
                            p { class: "s1", "Drawing the waveform…" }
                        },
                    }
                    audio { controls: true, preload: "none", src: "{url}", class: "sfaudio" }
                }
                if let Some(t) = thumb {
                    img { class: "sfthumb", src: "{t}", alt: "", loading: "lazy" }
                }
                if let Some(Some(t)) = &*body.read() {
                    pre { class: "sftext", "{t}" }
                }
                div { class: "sfacts",
                    if ext == "pdf" {
                        {
                            let u = url.clone();
                            let n = hit.name.clone();
                            rsx! {
                                button { class: "sfbtn", onclick: move |_| {
                                    let (u, n) = (u.clone(), n.clone());
                                    spawn(async move { web::open_pdf(&u, &n).await; });
                                },
                                    Icon { name: I::Open, size: 14, w: 2.2 }
                                    "Open chart"
                                }
                            }
                        }
                    }
                    a { class: "sfbtn", href: "{url}", download: "{hit.name}",
                        Icon { name: I::Download, size: 14, w: 2.2 }
                        "Download"
                    }
                }
                match &*copies.read() {
                    Some(c) if !c.is_empty() => rsx! {
                        div { class: "sfcopies",
                            span { class: "s1", "Also on:" }
                            for (i , c) in c.iter().enumerate() {
                                span { key: "{i}", class: if c.online { "sfcopy on" } else { "sfcopy" },
                                    "{c.device_name} · {c.location_name}"
                                }
                            }
                        }
                    },
                    _ => rsx! {},
                }
            }
        }
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = kind;
        rsx! {
            div { class: "sfprev",
                p { class: "s1", "{hit.path}" }
                LocalOpen { target: hit.target() }
            }
        }
    }
}

/// On a device: open the file through Drive's agent (a real path).
#[cfg(not(feature = "web"))]
#[component]
fn LocalOpen(target: String) -> Element {
    let mut said = use_signal(String::new);
    rsx! {
        button {
            class: "sfbtn",
            onclick: move |_| {
                #[cfg(feature = "local")]
                {
                    let r = crate::files_live::local::Holder::connect()
                        .and_then(|h| h.open(&target));
                    said.set(match r {
                        Ok(p) => p.display().to_string(),
                        Err(e) => e,
                    });
                }
                #[cfg(not(feature = "local"))]
                said.set(format!("{target}: no Drive agent on this device"));
            },
            "Open"
        }
        if !said().is_empty() {
            p { class: "s1", "{said}" }
        }
    }
}

/// Peaks as bars.
#[component]
fn Waveform(cols: Vec<(f32, f32)>) -> Element {
    let n = cols.len().max(1);
    let mut d = String::new();
    for (i, (lo, hi)) in cols.iter().enumerate() {
        let x = i as f32 + 0.5;
        let (y1, y2) = (
            20.0 - hi.clamp(-1.0, 1.0) * 19.0,
            20.0 - lo.clamp(-1.0, 1.0) * 19.0,
        );
        d.push_str(&format!("M{x:.1} {y1:.1}V{:.1}", y2.max(y1 + 0.6)));
    }
    rsx! {
        svg { class: "sfwave", view_box: "0 0 {n} 40", preserve_aspect_ratio: "none",
            path { d: "{d}", stroke: "currentColor", stroke_width: "0.8", fill: "none" }
        }
    }
}

/// Other names the song goes by, for finding its files.
#[component]
fn Aliases(sf: SongFiles) -> Element {
    let mut draft = use_signal(String::new);
    let list = sf.aliases();
    let sf2 = sf.clone();
    rsx! {
        div { class: "sfalias",
            span { class: "s1", "Also known as" }
            for a in list {
                {
                    let (sf, a2) = (sf.clone(), a.clone());
                    rsx! {
                        span { key: "{a}", class: "sfchip",
                            "{a}"
                            if sf.own() {
                                button { class: "chipx", aria_label: "Remove", onclick: move |_| sf.set_alias(&a2, false), "×" }
                            }
                        }
                    }
                }
            }
            if sf.own() {
                input {
                    class: "sfin small",
                    placeholder: "Add a name",
                    value: "{draft}",
                    oninput: move |ev| draft.set(ev.value()),
                    onkeydown: move |ev| {
                        if ev.key().to_string() == "Enter" {
                            sf2.set_alias(&draft(), true);
                            draft.set(String::new());
                        }
                    },
                }
            }
        }
    }
}

// ── finding a song's files ──────────────────────────────────────────────

/// Search every computer's indexes for the song (its title, artist,
/// aliases; or what is typed, in Drive's search language) and propose
/// what to link.
#[component]
fn FindPanel(sf: SongFiles, entry: Entry, onlinked: EventHandler<()>) -> Element {
    let app = use_app();
    let mut q = use_signal(String::new);
    let mut ran = use_signal(|| 0u64);
    let mut preview = use_signal(|| None::<String>);
    let mut status = use_signal(String::new);
    let orgs = (app.orgs)();
    let mut spaces = use_signal({
        let owner = sf.owner.clone();
        let orgs = orgs.clone();
        move || {
            // The song's org and the person's own computers by default.
            let mut v = vec![owner.clone()];
            for o in orgs.iter().filter(|o| o.as_str() == "codywright") {
                v.push(o.clone());
            }
            v.dedup();
            v
        }
    });
    let aliases = sf.aliases();
    let auto = files::queries(&entry.title, &aliases);
    let found = {
        let entry = entry.clone();
        let aliases = aliases.clone();
        use_resource(move || {
            let (entry, aliases) = (entry.clone(), aliases.clone());
            let typed = q();
            let sp = spaces();
            let _ = ran();
            async move { find(&entry, &aliases, &typed, &sp).await }
        })
    };
    let links = sf.links();
    let result = found.read().clone();
    rsx! {
        div { class: "sffind",
            div { class: "sffrow",
                input {
                    class: "sfin",
                    placeholder: "Search: {auto.join(\" · \")}   (or kind:audio path:Multitracks/ NOT backup …)",
                    value: "{q}",
                    oninput: move |ev| q.set(ev.value()),
                }
                button { class: "sfbtn", onclick: move |_| ran += 1, "Search again" }
            }
            div { class: "sfspaces",
                span { class: "s1", "In:" }
                for o in orgs.clone() {
                    {
                        let on = spaces().contains(&o);
                        let o2 = o.clone();
                        rsx! {
                            button {
                                key: "{o}",
                                class: if on { "sfchip pick on" } else { "sfchip pick" },
                                onclick: move |_| {
                                    let mut v = spaces();
                                    if v.contains(&o2) { v.retain(|x| *x != o2); } else { v.push(o2.clone()); }
                                    spaces.set(v);
                                },
                                "{crate::orgs::org_name(&o)}"
                            }
                        }
                    }
                }
            }
            match result {
                None => rsx! {
                    p { class: "s1", "Searching every computer…" }
                },
                Some(Err(e)) => rsx! {
                    p { class: "s1 err", "{e}" }
                },
                Some(Ok((props, asked))) => rsx! {
                    p { class: "s1", "{props.len()} matches for {asked}" }
                    if !status().is_empty() {
                        p { class: "hint", "{status}" }
                    }
                    div { class: "sfprops",
                        for p in props.into_iter().filter(|p| !links.iter().any(|l| l.covers(&p.hit.space, &p.hit.location, &p.hit.path))) {
                            {
                                let key = format!("{}:{}", p.hit.location, p.hit.path);
                                let showing = preview().as_deref() == Some(key.as_str());
                                let (sf, entry, p2) = (sf.clone(), entry.clone(), p.clone());
                                let k2 = key.clone();
                                rsx! {
                                    ProposalRow {
                                        key: "{key}",
                                        p: p.clone(),
                                        showing,
                                        onpreview: move |_| preview.set(if showing { None } else { Some(k2.clone()) }),
                                        onlink: move |kind: FileKind| {
                                            let l = NewLink {
                                                space: p2.hit.space.clone(),
                                                location: p2.hit.location.clone(),
                                                path: p2.hit.path.clone(),
                                                dir: p2.hit.dir,
                                                kind: Some(kind),
                                            };
                                            if let Some(part) = sf.link(&entry, l) {
                                                status.set(format!("Linked {} where it is: nothing copied.", p2.hit.name));
                                                if p2.hit.dir && kind == FileKind::Other {
                                                    refine_folder(sf.clone(), part, p2.hit.clone());
                                                }
                                                onlinked.call(());
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ProposalRow(
    p: Proposal,
    showing: bool,
    onpreview: EventHandler<()>,
    onlink: EventHandler<FileKind>,
) -> Element {
    let mut kind = use_signal(|| p.kind);
    let h = p.hit.clone();
    rsx! {
        div { class: "sfrow prop",
            button { class: "sfplay", aria_label: "Preview", disabled: h.dir, onclick: move |_| onpreview.call(()),
                Icon { name: if h.dir { I::Folder } else if files::is_audio(&h.name) { I::Play } else { kind_icon(p.kind) }, size: 16, w: 2.0 }
            }
            span { class: "mid",
                span { class: "t1", "{h.name}" }
                span { class: "s1 path", title: "{h.path}", "{h.path}" }
                span { class: "s1",
                    "{where_line(&h)}"
                    if h.size > 0 {
                        " · {files::bytes(h.size)}"
                    }
                    if !p.also.is_empty() {
                        " · also on {p.also.len()} more"
                    }
                    " · {p.why}"
                }
            }
            KindPick { value: kind(), onpick: move |k| kind.set(k) }
            button { class: "sfbtn link", onclick: move |_| onlink.call(kind()),
                Icon { name: I::Link, size: 14, w: 2.2 }
                "Link"
            }
        }
        if showing && !h.dir {
            Preview { hit: h.clone(), kind: p.kind }
        }
    }
}

/// A folder whose name said nothing: its kind from what is in it.
fn refine_folder(sf: SongFiles, part: String, h: Hit) {
    #[cfg(feature = "web")]
    spawn(async move {
        let kids = crate::files_live::web::list(&h.space, &h.location, &h.path).await;
        let names: Vec<(&str, bool)> = kids.iter().map(|k| (k.name.as_str(), k.dir)).collect();
        let k = files::recognise_folder(&h.name, &names);
        if k != FileKind::Other {
            sf.set_kind(&part, k);
        }
    });
    #[cfg(not(feature = "web"))]
    let _ = (sf, part, h);
}

type Found = Result<(Vec<Proposal>, String), String>;

#[cfg(feature = "web")]
async fn find(entry: &Entry, aliases: &[String], typed: &str, spaces: &[String]) -> Found {
    use crate::files_live::web;
    let typed_q = files::typed(typed)?;
    let mut hits: Vec<Hit> = Vec::new();
    let asked;
    match &typed_q {
        Some(t) if !t.plain => {
            asked = format!("“{typed}”");
            let qs = if t.queries.is_empty() {
                vec![String::new()]
            } else {
                t.queries.clone()
            };
            for sp in spaces {
                for q in &qs {
                    hits.extend(web::search(sp, q, &t.exts, 300).await);
                }
            }
            hits.retain(|h| t.matches(h));
        }
        _ => {
            let qs = if typed.trim().is_empty() {
                files::queries(&entry.title, aliases)
            } else {
                vec![files::fold(typed)]
            };
            asked = qs.join(" · ");
            for sp in spaces {
                for q in &qs {
                    hits.extend(web::search(sp, q, &[], 300).await);
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    hits.retain(|h| seen.insert((h.space.clone(), h.location.clone(), h.path.clone())));
    let props = if typed_q.as_ref().is_some_and(|t| !t.plain) {
        // The language chose them: show them as found, best name first.
        let mut v = files::propose(&entry.title, entry.artist.as_deref(), aliases, &hits, &[]);
        let proposed: std::collections::HashSet<String> =
            v.iter().map(|p| p.hit.target()).collect();
        v.extend(
            hits.iter()
                .filter(|h| !proposed.contains(&h.target()))
                .take(60)
                .map(|h| Proposal {
                    hit: h.clone(),
                    also: Vec::new(),
                    kind: files::recognise(&h.path, h.dir),
                    score: 0,
                    why: "matches the search".into(),
                }),
        );
        v
    } else {
        let title = if typed.trim().is_empty() {
            entry.title.clone()
        } else {
            typed.to_string()
        };
        files::propose(&title, entry.artist.as_deref(), aliases, &hits, &[])
    };
    Ok((props, asked))
}

#[cfg(not(feature = "web"))]
async fn find(_entry: &Entry, _aliases: &[String], _typed: &str, _spaces: &[String]) -> Found {
    Err("Finding files searches Drive's indexes of every computer; this build has no Drive client yet (use the web build).".into())
}

// ── a set's readiness ───────────────────────────────────────────────────

/// One song of a set, for its readiness.
#[derive(Clone, Debug, PartialEq)]
pub struct SetSong {
    pub owner: String,
    pub id: String,
    pub title: String,
}

/// What each song's probe found: its readiness and its parts.
#[derive(Clone, Debug, PartialEq)]
struct Probe {
    ready: SongReady,
    parts: Vec<String>,
    targets: Vec<String>,
}

/// A set's readiness: per song and overall, "ready offline?", and the
/// action that gets it there: keep its files on Drive (the web), or warm
/// them onto this device (a desktop with Drive's agent).
#[component]
pub fn SetReadiness(songs: Vec<SetSong>, tracks: bool) -> Element {
    let probes = use_signal(BTreeMap::<String, Probe>::new);
    let mut open = use_signal(|| false);
    let here = (use_app().here)();
    let got: Vec<(SetSong, Option<Probe>)> = songs
        .iter()
        .map(|s| {
            (
                s.clone(),
                probes.read().get(&format!("{}/{}", s.owner, s.id)).cloned(),
            )
        })
        .collect();
    let readies: Vec<SongReady> = got
        .iter()
        .filter_map(|(_, p)| p.as_ref().map(|p| p.ready.clone()))
        .collect();
    let all = files::set_ready(&readies);
    let checked = readies.len();
    let headline = if checked < songs.len() {
        format!("Checking {} of {} songs…", checked, songs.len())
    } else if all.without_files == songs.len() {
        "No files linked to these songs yet".to_string()
    } else if all.ready_offline + all.without_files == songs.len() {
        "Ready whatever is switched off".to_string()
    } else {
        format!(
            "{} of {} songs ready offline",
            all.ready_offline,
            songs.len() - all.without_files
        )
    };
    let own_parts: Vec<(String, Vec<String>)> = got
        .iter()
        .filter(|(s, _)| s.owner == here)
        .filter_map(|(s, p)| p.as_ref().map(|p| (s.id.clone(), p.parts.clone())))
        .collect();
    let targets: Vec<String> = got
        .iter()
        .filter_map(|(_, p)| p.as_ref())
        .flat_map(|p| p.targets.clone())
        .collect();
    rsx! {
        div { class: "setready",
            for s in songs.iter() {
                ReadyProbe { key: "{s.owner}/{s.id}", song: s.clone(), tracks, probes }
            }
            button { class: "srhead", onclick: move |_| open.toggle(),
                if checked < songs.len() {
                    ReadyBadge { r: None, label: Some("Checking…".to_string()) }
                } else {
                    ReadyBadge { r: all.state, label: Some(all.state.map_or("No files", Ready::label).to_string()) }
                }
                span { class: "mid",
                    span { class: "t1", "{headline}" }
                    span { class: "s1",
                        if all.to_warm > 0 {
                            "{all.to_warm} files ({files::bytes(all.bytes_to_warm)}) not yet kept"
                        } else {
                            "Every linked file is kept"
                        }
                        if !all.needs.is_empty() {
                            " · needs {all.needs.join(\", \")} switched on"
                        }
                    }
                }
                Icon { name: if open() { I::ChevDown } else { I::ChevRight }, size: 16 }
            }
            WarmButton { own_parts, targets, to_warm: all.to_warm }
            if open() {
                div { class: "srlist",
                    for (s , p) in got {
                        div { key: "{s.owner}/{s.id}", class: "srrow",
                            span { class: "t1", "{s.title}" }
                            match p {
                                None => rsx! { span { class: "s1", "checking…" } },
                                Some(p) => rsx! {
                                    span { class: "s1",
                                        if p.ready.files == 0 {
                                            "no files"
                                        } else {
                                            "{files::count(p.ready.files)} · {files::bytes(p.ready.bytes)}"
                                        }
                                        if !p.ready.lacks.is_empty() {
                                            " · no {p.ready.lacks.iter().map(|k| k.name().to_lowercase()).collect::<Vec<_>>().join(\", \")}"
                                        }
                                    }
                                    ReadyBadge { r: p.ready.state }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Reads one song's files and says how ready it is.
#[component]
fn ReadyProbe(song: SetSong, tracks: bool, probes: Signal<BTreeMap<String, Probe>>) -> Element {
    let sf = use_song_files(&song.owner, &song.id);
    let refresh = use_signal(|| 0u64);
    let rows = use_rows(&sf, refresh);
    let key = format!("{}/{}", song.owner, song.id);
    let list = rows.read().clone();
    let links = sf.links();
    use_effect(use_reactive!(|list| {
        let Some(list) = list else {
            return;
        };
        let key = key.clone();
        let parts: Vec<String> = links.iter().map(|l| l.part.clone()).collect();
        let mut probes = probes;
        spawn(async move {
            let mut states = Vec::new();
            let mut asked = 0;
            for r in &list {
                // Only for what this place cannot give now: another copy
                // may be on a computer that is on.
                let others =
                    if !r.missing && !r.hit.online && r.hit.available.is_none() && asked < 40 {
                        asked += 1;
                        copies_of(&r.hit).await
                    } else {
                        Vec::new()
                    };
                states.push(r.state(&others));
            }
            let ready = files::song_ready(&states, &files::wanted(tracks));
            let targets = list
                .iter()
                .filter(|r| !r.missing)
                .map(Row::target)
                .collect();
            probes.write().insert(
                key,
                Probe {
                    ready,
                    parts,
                    targets,
                },
            );
        });
    }));
    rsx! {}
}

#[cfg(feature = "web")]
async fn copies_of(h: &Hit) -> Vec<files::Copy> {
    crate::files_live::web::copies(h).await
}

#[cfg(not(feature = "web"))]
async fn copies_of(_h: &Hit) -> Vec<files::Copy> {
    Vec::new()
}

/// Keep the set's files on Drive (the web: the computers that have them
/// put them on the node), or warm them here (a desktop with Drive's agent,
/// held while the set is open).
#[component]
fn WarmButton(
    own_parts: Vec<(String, Vec<String>)>,
    targets: Vec<String>,
    to_warm: usize,
) -> Element {
    let app = use_app();
    let here = (app.here)();
    let mut said = use_signal(String::new);
    #[cfg(not(feature = "local"))]
    let _ = &targets;
    #[cfg(feature = "local")]
    let _ = (&here, &own_parts);
    let label = if cfg!(feature = "local") {
        "Warm on this device"
    } else {
        "Keep on Drive"
    };
    #[cfg(feature = "local")]
    use_drop(release_set);
    rsx! {
        div { class: "srwarm",
            button {
                class: "sfbtn warm",
                disabled: to_warm == 0,
                onclick: move |_| {
                    #[cfg(feature = "local")]
                    {
                        said.set(local_warm(targets.clone()));
                    }
                    #[cfg(not(feature = "local"))]
                    {
                        let n: usize = own_parts.iter().map(|(_, p)| p.len()).sum();
                        for (song, parts) in &own_parts {
                            KeepParts::keep(&here, song, parts);
                        }
                        said.set(format!(
                            "{n} linked files and folders marked to keep on Drive: each computer that has them puts them on the node when it is on."
                        ));
                    }
                },
                Icon { name: I::Download, size: 14, w: 2.2 }
                "{label}"
            }
            if !said().is_empty() {
                span { class: "s1", "{said}" }
            }
        }
    }
}

/// Marks parts available in the org's registry, outside a component.
struct KeepParts;

impl KeepParts {
    #[cfg_attr(feature = "local", allow(dead_code))]
    fn keep(org: &str, song: &str, parts: &[String]) {
        let id = drive_files::space_doc(org);
        #[cfg(feature = "web")]
        {
            let Some(ctx) = try_consume_context::<drive_dioxus::DriveCtx>() else {
                return;
            };
            let doc = ctx.drive.open(&id);
            files::set_available(&doc, song, parts, true);
            ctx.drive.commit(&id);
        }
        #[cfg(not(feature = "web"))]
        {
            if let Some(doc) = crate::songs::backing::peek(&id) {
                files::set_available(&doc, song, parts, true);
                doc.commit();
            }
        }
    }
}

/// What the open set holds through Drive's agent: its connection, whose
/// leases keep the set's files pinned until the set closes.
#[cfg(feature = "local")]
static HELD: std::sync::Mutex<Option<std::sync::Arc<crate::files_live::local::Holder>>> =
    std::sync::Mutex::new(None);

/// Warm a set's files with Drive's agent, held while the set is open.
#[cfg(feature = "local")]
fn local_warm(targets: Vec<String>) -> String {
    use crate::files_live::local::Holder;
    let h = match Holder::connect() {
        Ok(h) => std::sync::Arc::new(h),
        Err(e) => return e,
    };
    if let Ok(mut held) = HELD.lock() {
        *held = Some(h.clone());
    }
    let n = targets.len();
    std::thread::spawn(move || {
        let (tx, _rx) = std::sync::mpsc::channel();
        let _ = h.warm(targets, tx);
    });
    format!("Warming {n} files on this device; they stay pinned while the set is open.")
}

/// The set closed: let its files go (the agent's leases end with the
/// connection).
#[cfg(feature = "local")]
pub fn release_set() {
    if let Ok(mut held) = HELD.lock() {
        *held = None;
    }
}

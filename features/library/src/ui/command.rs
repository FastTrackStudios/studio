//! The keyboard: the shortcuts, ⌘K (every command, fuzzily), ⌘O (open
//! any project, setlist, song, session or album by name) and `?` (the
//! shortcuts, listed).
//!
//! The library's root takes the keys ([`on_key`]) and the overlays type
//! into a query it keeps, drawn with a caret, rather than into an
//! `<input>`: Blitz cannot focus a field from code (`autofocus` is
//! ignored, `set_focus` panics from an event handler), and a palette you
//! must tap before you can type is not a palette. A field that has the
//! focus stops a plain key at itself, so typing a `?` into a comment
//! never opens the sheet.

use dioxus::prelude::*;
use lucide_dioxus::{AudioWaveform, CornerDownLeft, Disc3, FolderKanban, ListMusic, Music, Search};

use super::kit::*;
use super::{Nav, Open, Route, open, use_library, use_nav};
use crate::model::{Library, Piece, fuzzy};

/// Which overlay is up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    None,
    /// ⌘K: every command.
    Palette,
    /// ⌘O: open anything by name.
    Finder,
    /// `?`: the shortcuts.
    Help,
}

/// The overlays' state: which is up, what has been typed, which row is
/// picked.
#[derive(Clone, Copy)]
pub struct Keys {
    pub overlay: Signal<Overlay>,
    query: Signal<String>,
    picked: Signal<usize>,
}

impl Keys {
    pub fn new() -> Self {
        Self {
            overlay: Signal::new(Overlay::None),
            query: Signal::new(String::new()),
            picked: Signal::new(0),
        }
    }

    pub fn show(mut self, which: Overlay) {
        self.query.set(String::new());
        self.picked.set(0);
        self.overlay.set(which);
    }

    pub fn close(mut self) {
        self.overlay.set(Overlay::None);
    }
}

/// What a command does when it runs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    Go(Route),
    Back,
    Show(Overlay),
    Leave(Open),
}

/// One command: what it says, where it is grouped, its keys (as drawn),
/// and what it does.
#[derive(Clone, PartialEq, Debug)]
pub struct Command {
    pub label: String,
    pub group: &'static str,
    pub keys: &'static [&'static str],
    pub act: Act,
}

/// The sidebar's destinations, by ⌘ and their number.
const PLACES: [(Route, &str, &str); 6] = [
    (Route::Home, "Home", "1"),
    (Route::Projects, "Projects", "2"),
    (Route::Setlists, "Setlists", "3"),
    (Route::Songs, "Songs", "4"),
    (Route::Sessions, "Sessions", "5"),
    (Route::Rigs, "Rigs", "6"),
];

/// Every command there is from where the library is: the global ones, and
/// those of the page (play a set on each of my rigs, rehearse it, open its
/// project; a song's arrangements; a session in Session).
pub fn commands(l: &Library, here: Route) -> Vec<Command> {
    let mut cmds = Vec::new();
    for (route, name, key) in PLACES {
        let keys: &'static [&'static str] = match key {
            "1" => &["⌘", "1"],
            "2" => &["⌘", "2"],
            "3" => &["⌘", "3"],
            "4" => &["⌘", "4"],
            "5" => &["⌘", "5"],
            _ => &["⌘", "6"],
        };
        cmds.push(Command {
            label: format!("Go to {name}"),
            group: "Go",
            keys,
            act: Act::Go(route),
        });
    }
    cmds.push(Command {
        label: "Back".into(),
        group: "Go",
        keys: &["⌘", "["],
        act: Act::Back,
    });
    cmds.push(Command {
        label: "Open a project, setlist, song or session…".into(),
        group: "Go",
        keys: &["⌘", "O"],
        act: Act::Show(Overlay::Finder),
    });
    cmds.push(Command {
        label: "Keyboard shortcuts".into(),
        group: "Help",
        keys: &["?"],
        act: Act::Show(Overlay::Help),
    });

    match here {
        Route::Setlist(id, _) => {
            if let Some(set) = l.setlist(id) {
                for p in l.parts_of(id).into_iter().filter(|p| p.player == l.me) {
                    if let Some(rig) = l.rig(p.rig) {
                        cmds.push(Command {
                            label: format!("Play {} on {}", set.title, rig.kind.label()),
                            group: "This setlist",
                            keys: &[],
                            act: Act::Leave(Open::Part(p.id)),
                        });
                    }
                }
                cmds.push(Command {
                    label: format!("Rehearse {}", set.title),
                    group: "This setlist",
                    keys: &[],
                    act: Act::Leave(Open::Rehearse(id)),
                });
                if let Some(p) = l.project_of_setlist(id) {
                    cmds.push(Command {
                        label: format!("Go to its project, {}", p.title),
                        group: "This setlist",
                        keys: &[],
                        act: Act::Go(Route::Project(p.id)),
                    });
                }
                for e in &set.entries {
                    if let Piece::Song { song, arrangement } = e.piece
                        && let Some(s) = l.song(song)
                    {
                        cmds.push(Command {
                            label: format!("Open song {}", s.title),
                            group: "This setlist",
                            keys: &[],
                            act: Act::Go(Route::Song(song, Some(arrangement))),
                        });
                    }
                }
            }
        }
        Route::Song(id, _) => {
            if let Some(s) = l.song(id) {
                for a in &s.arrangements {
                    cmds.push(Command {
                        label: format!("Show the {} arrangement", a.name),
                        group: "This song",
                        keys: &[],
                        act: Act::Go(Route::Song(id, Some(a.id))),
                    });
                    if let Some(session) = a.session {
                        cmds.push(Command {
                            label: format!("Open the {} session in Session", a.name),
                            group: "This song",
                            keys: &[],
                            act: Act::Leave(Open::Session(session)),
                        });
                    }
                }
            }
        }
        Route::Session(id) => cmds.push(Command {
            label: "Open in Session".into(),
            group: "This session",
            keys: &[],
            act: Act::Leave(Open::Session(id)),
        }),
        _ => {}
    }
    cmds
}

/// What the finder can open: (title, kind, detail, route).
fn openables(l: &Library) -> Vec<(String, &'static str, String, Route)> {
    let mut all = Vec::new();
    for p in &l.projects {
        all.push((
            p.title.clone(),
            "Project",
            p.kind.label().to_owned(),
            Route::Project(p.id),
        ));
    }
    for s in &l.setlists {
        all.push((
            s.title.clone(),
            "Setlist",
            s.when.clone().unwrap_or_default(),
            Route::Setlist(s.id, None),
        ));
    }
    for s in &l.songs {
        all.push((
            s.title.clone(),
            "Song",
            s.artist.clone(),
            Route::Song(s.id, None),
        ));
    }
    for s in &l.sessions {
        all.push((
            s.title.clone(),
            "Session",
            s.stage.label().to_owned(),
            Route::Session(s.id),
        ));
    }
    for a in &l.albums {
        all.push((
            a.title.clone(),
            "Album",
            format!("{} · {}", a.artist, a.year),
            Route::Album(a.id),
        ));
    }
    all
}

/// The rows an overlay lists for `query`, best first: (label, group or
/// kind, detail, keys, what it does).
fn rows(
    l: &Library,
    here: Route,
    which: Overlay,
    query: &str,
) -> Vec<(String, &'static str, String, &'static [&'static str], Act)> {
    let mut scored: Vec<(
        i32,
        (String, &'static str, String, &'static [&'static str], Act),
    )> = match which {
        Overlay::Palette => commands(l, here)
            .into_iter()
            .filter_map(|c| {
                Some((
                    fuzzy(query, &c.label)?,
                    (c.label, c.group, String::new(), c.keys, c.act),
                ))
            })
            .collect(),
        Overlay::Finder => openables(l)
            .into_iter()
            .filter_map(|(title, kind, detail, route)| {
                Some((
                    fuzzy(query, &title)?,
                    (title, kind, detail, &[][..], Act::Go(route)),
                ))
            })
            .collect(),
        _ => Vec::new(),
    };
    // Stable: equal scores keep the list's own order (places first).
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, r)| r).take(40).collect()
}

fn run(act: Act, nav: Nav, keys: Keys) {
    keys.close();
    match act {
        Act::Go(route) => nav.go(route),
        Act::Back => nav.go_back(),
        Act::Show(which) => keys.show(which),
        Act::Leave(what) => open(what),
    }
}

/// Whether ⌘ (or Ctrl, off a Mac) is held. A Mac's ⌘ arrives as META
/// from some windowing layers and SUPER from others: either counts.
pub(crate) fn command_held(e: &KeyboardEvent) -> bool {
    let m = e.modifiers();
    m.contains(Modifiers::META) || m.contains(Modifiers::SUPER) || m.contains(Modifiers::CONTROL)
}

/// The library root's key handler.
pub(crate) fn on_key(e: &KeyboardEvent, l: &Library, nav: Nav, mut keys: Keys) {
    let cmd = command_held(e);
    let overlay = *keys.overlay.peek();
    if overlay != Overlay::None {
        match e.key() {
            Key::Escape => keys.close(),
            Key::ArrowDown => {
                let n = rows(l, *nav.route.peek(), overlay, &keys.query.peek()).len();
                let p = *keys.picked.peek();
                keys.picked.set(if n == 0 { 0 } else { (p + 1).min(n - 1) });
            }
            Key::ArrowUp => {
                let p = *keys.picked.peek();
                keys.picked.set(p.saturating_sub(1));
            }
            Key::Enter => {
                let list = rows(l, *nav.route.peek(), overlay, &keys.query.peek());
                if let Some(row) = list.get(*keys.picked.peek()) {
                    run(row.4, nav, keys);
                }
            }
            Key::Backspace => {
                keys.query.write().pop();
                keys.picked.set(0);
            }
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("k") => keys.show(Overlay::Palette),
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("o") => keys.show(Overlay::Finder),
            Key::Character(c) if !cmd && overlay != Overlay::Help => {
                keys.query.write().push_str(&c);
                keys.picked.set(0);
            }
            _ => return,
        }
        e.prevent_default();
        return;
    }
    match e.key() {
        Key::Character(c) if cmd && c.eq_ignore_ascii_case("k") => keys.show(Overlay::Palette),
        Key::Character(c) if cmd && c.eq_ignore_ascii_case("o") => keys.show(Overlay::Finder),
        Key::Character(c) if cmd && c == "[" => nav.go_back(),
        Key::Character(c) if cmd && PLACES.iter().any(|p| p.2 == c) => {
            if let Some((route, ..)) = PLACES.iter().find(|p| p.2 == c) {
                nav.jump(*route);
            }
        }
        Key::Character(c) if !cmd && c == "?" => keys.show(Overlay::Help),
        _ => return,
    }
    e.prevent_default();
}

/// A key, drawn as a keycap.
#[component]
pub(crate) fn Kbd(text: &'static str) -> Element {
    rsx! {
        span {
            style: "min-width: 22px; height: 22px; padding: 0 6px; border-radius: 6px; border: 1px solid {RULE_STRONG}; \
                    border-bottom-width: 2px; background: {SHEET_2}; color: {INK_2}; font-size: 12px; font-weight: 650; \
                    display: inline-flex; align-items: center; justify-content: center; box-sizing: border-box;",
            "{text}"
        }
    }
}

/// The overlay that is up, over the whole library.
#[component]
pub(crate) fn Overlays() -> Element {
    let keys = use_context::<Keys>();
    match (keys.overlay)() {
        Overlay::None => rsx! {},
        Overlay::Help => rsx! { HelpSheet {} },
        which => rsx! { Picker { which } },
    }
}

/// A scrim over the page; a press on it closes the overlay.
#[component]
fn Scrim(children: Element) -> Element {
    let keys = use_context::<Keys>();
    rsx! {
        div {
            style: "position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; z-index: 50; \
                    background: rgba(0,0,0,0.55); display: flex; justify-content: center; align-items: flex-start;",
            onclick: move |_| keys.close(),
            div {
                style: "margin-top: 10vh; width: 640px; max-width: 92vw; border-radius: 16px; background: {SHEET}; \
                        border: 1px solid {RULE_STRONG}; box-shadow: 0 24px 64px rgba(0,0,0,0.55), 0 2px 8px rgba(0,0,0,0.4); \
                        overflow: hidden; display: flex; flex-direction: column;",
                // A press inside the panel is the panel's.
                onclick: move |e| e.stop_propagation(),
                {children}
            }
        }
    }
}

/// ⌘K and ⌘O: a query, typed at the library, and the rows that match it.
#[component]
fn Picker(which: Overlay) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let keys = use_context::<Keys>();
    let mut picked = keys.picked;
    let query = (keys.query)();
    let here = (nav.route)();
    let list = rows(&lib.read(), here, which, &query);
    let at = picked().min(list.len().saturating_sub(1));
    let hint = match which {
        Overlay::Finder => "Open a project, setlist, song or session",
        _ => "Type a command",
    };
    let count = list.len();
    rsx! {
        Scrim {
            div { style: "height: 60px; display: flex; align-items: center; gap: 12px; padding: 0 18px; border-bottom: 1px solid {RULE};",
                Search { size: 20, color: INK_3 }
                div { style: "flex: 1; min-width: 0; display: flex; align-items: center; font-size: 18px; {ONE_LINE}",
                    if query.is_empty() {
                        span { style: "width: 2px; height: 22px; background: {PICK}; margin-right: 2px;" }
                        span { style: "color: {INK_3};", "{hint}" }
                    } else {
                        span { style: "color: {INK};", "{query}" }
                        span { style: "width: 2px; height: 22px; background: {PICK}; margin-left: 1px;" }
                    }
                }
                Kbd { text: "esc" }
            }
            div { style: "max-height: 56vh; overflow-y: scroll; padding: 6px;",
                if count == 0 {
                    div { style: "padding: 28px 16px; {META} color: {INK_3}; text-align: center;", "Nothing matches “{query}”." }
                }
                for (i, (label, group, detail, cap, act)) in list.into_iter().enumerate() {
                    {
                        let on = i == at;
                        let bg = if on { PICK_BG } else { "transparent" };
                        let nav = nav;
                        let keys = keys;
                        rsx! {
                            div {
                                key: "{i}",
                                class: "lp",
                                style: "height: 48px; display: flex; align-items: center; gap: 12px; padding: 0 12px; \
                                        border-radius: 10px; background: {bg}; cursor: pointer;",
                                onmouseenter: move |_| picked.set(i),
                                onclick: move |_| run(act, nav, keys),
                                {kind_icon(group)}
                                span { style: "flex: 1; min-width: 0; {BODY} font-size: 15px; {ONE_LINE}", "{label}" }
                                if !detail.is_empty() {
                                    span { style: "{META} color: {INK_3}; {ONE_LINE} max-width: 180px;", "{detail}" }
                                }
                                if which == Overlay::Palette {
                                    span { style: "{META} color: {INK_3}; white-space: nowrap;", "{group}" }
                                }
                                for (j, k) in cap.iter().enumerate() { Kbd { key: "{j}", text: k } }
                                if on { CornerDownLeft { size: 16, color: INK_3 } }
                            }
                        }
                    }
                }
            }
            div { style: "height: 40px; display: flex; align-items: center; gap: 14px; padding: 0 16px; border-top: 1px solid {RULE}; \
                          {META} color: {INK_3};",
                span { style: "display: flex; align-items: center; gap: 6px;", Kbd { text: "↑" } Kbd { text: "↓" } "move" }
                span { style: "display: flex; align-items: center; gap: 6px;", Kbd { text: "return" } "open" }
                span { style: "flex: 1;" }
                if which == Overlay::Palette {
                    span { style: "display: flex; align-items: center; gap: 6px;", Kbd { text: "⌘" } Kbd { text: "O" } "open by name" }
                } else {
                    span { style: "display: flex; align-items: center; gap: 6px;", Kbd { text: "⌘" } Kbd { text: "K" } "commands" }
                }
            }
        }
    }
}

fn kind_icon(group: &str) -> Element {
    match group {
        "Project" => rsx! { FolderKanban { size: 18, color: INK_3 } },
        "Setlist" | "This setlist" => rsx! { ListMusic { size: 18, color: INK_3 } },
        "Song" | "This song" => rsx! { Music { size: 18, color: INK_3 } },
        "Session" | "This session" => rsx! { AudioWaveform { size: 18, color: INK_3 } },
        "Album" => rsx! { Disc3 { size: 18, color: INK_3 } },
        _ => rsx! { span { style: "width: 18px;" } },
    }
}

/// `?`: every shortcut, grouped.
#[component]
fn HelpSheet() -> Element {
    let groups: [(&str, Vec<(&'static [&'static str], &str)>); 3] = [
        (
            "Anywhere",
            vec![
                (&["⌘", "K"], "Command palette"),
                (
                    &["⌘", "O"],
                    "Open a project, setlist, song or session by name",
                ),
                (&["?"], "These shortcuts"),
                (&["esc"], "Close what is open"),
            ],
        ),
        (
            "Go",
            vec![
                (&["⌘", "1"], "Home"),
                (&["⌘", "2"], "Projects"),
                (&["⌘", "3"], "Setlists"),
                (&["⌘", "4"], "Songs"),
                (&["⌘", "5"], "Sessions"),
                (&["⌘", "6"], "Rigs"),
                (&["⌘", "["], "Back"),
            ],
        ),
        (
            "In a list",
            vec![(&["↑", "↓"], "Move"), (&["return"], "Open")],
        ),
    ];
    rsx! {
        Scrim {
            div { style: "height: 60px; display: flex; align-items: center; padding: 0 20px; border-bottom: 1px solid {RULE};",
                span { style: "{HEADING} flex: 1;", "Keyboard shortcuts" }
                Kbd { text: "esc" }
            }
            div { style: "padding: 8px 20px 18px; display: flex; flex-direction: column;",
                for (g, (name, items)) in groups.into_iter().enumerate() {
                    div { key: "{g}",
                        div { style: "{COLUMN} color: {INK_3}; padding: 16px 0 6px;", "{name}" }
                        for (i, (cap, what)) in items.into_iter().enumerate() {
                            div { key: "{i}", style: "height: 40px; display: flex; align-items: center; gap: 6px; border-top: 1px solid {RULE};",
                                span { style: "flex: 1; font-size: 14px; color: {INK};", "{what}" }
                                for (j, k) in cap.iter().enumerate() { Kbd { key: "{j}", text: k } }
                            }
                        }
                    }
                }
            }
        }
    }
}

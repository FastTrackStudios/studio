//! The keyboard: the shortcuts, ⌘K (every command, fuzzily), ⌘O (open
//! any setlist, session, part, song, project or album by name) and `?`
//! (the shortcuts and every command, listed).
//!
//! The commands are the library's own and the app's ([`AppCommands`]: in
//! Studio, Session's Action List — the actions its keys, the CLI and a
//! model over MCP run), in one list. ⌘O opens what it picks the way a
//! press in the library does ([`Open`]): a setlist or a session straight
//! into the DAW, a part into its rig.
//!
//! The library's root takes the keys ([`on_key`]) and the overlays type
//! into a query it keeps, drawn with a caret, rather than into an
//! `<input>`: Blitz cannot focus a field from code (`autofocus` is
//! ignored, `set_focus` panics from an event handler), and a palette you
//! must tap before you can type is not a palette. A field that has the
//! focus stops a plain key at itself, so typing a `?` into a comment
//! never opens the sheet. Over another page (a song open in the DAW) the
//! same menus are the [`super::CommandLayer`]'s.

use dioxus::prelude::*;
use lucide_dioxus::{
    AudioWaveform, CornerDownLeft, Disc3, FolderKanban, Guitar, ListMusic, Music, Search,
};

use super::kit::*;
use super::{AppCommand, AppCommands, LibraryData, Nav, Open, Opener, Route, ShowLibrary};
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
#[derive(Clone, PartialEq, Debug)]
pub enum Act {
    Go(Route),
    Back,
    Show(Overlay),
    Leave(Open),
    /// One of the app's commands, by its id ([`AppCommands`]).
    App(String),
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
/// project; a song's arrangements; a session in the DAW).
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
        label: "Open a setlist, session, part or song…".into(),
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
                            label: format!("Open the {} session", a.name),
                            group: "This song",
                            keys: &[],
                            act: Act::Leave(Open::Session(session)),
                        });
                    }
                }
            }
        }
        Route::Session(id) => cmds.push(Command {
            label: "Open the session".into(),
            group: "This session",
            keys: &[],
            act: Act::Leave(Open::Session(id)),
        }),
        _ => {}
    }
    cmds
}

/// One thing the finder can open: its title, its kind, a line about it,
/// and what picking it does.
#[derive(Clone, PartialEq, Debug)]
pub struct Openable {
    pub title: String,
    pub kind: &'static str,
    pub detail: String,
    pub act: Act,
}

/// What the finder can open. What plays opens the way a press on it in
/// the library does ([`Open`]): a setlist rehearsed in the DAW, a session
/// in the DAW, my part on a setlist in its rig, a song in the session of
/// its first arrangement that has one. What does not play (a project, an
/// album, a song with no session) is a page of the library.
pub fn openables(l: &Library) -> Vec<Openable> {
    let mut all = Vec::new();
    for s in &l.setlists {
        all.push(Openable {
            title: s.title.clone(),
            kind: "Setlist",
            detail: s.when.clone().unwrap_or_default(),
            act: Act::Leave(Open::Rehearse(s.id)),
        });
    }
    for s in &l.sessions {
        all.push(Openable {
            title: s.title.clone(),
            kind: "Session",
            detail: s.stage.label().to_owned(),
            act: Act::Leave(Open::Session(s.id)),
        });
    }
    for p in l.parts.iter().filter(|p| p.player == l.me) {
        if let (Some(set), Some(rig)) = (l.setlist(p.setlist), l.rig(p.rig)) {
            all.push(Openable {
                title: format!("{} on {}", set.title, rig.kind.label()),
                kind: "Part",
                detail: rig.profile.clone(),
                act: Act::Leave(Open::Part(p.id)),
            });
        }
    }
    for s in &l.songs {
        let session = s.arrangements.iter().find_map(|a| a.session);
        all.push(Openable {
            title: s.title.clone(),
            kind: "Song",
            detail: s.artist.clone(),
            act: session.map_or(Act::Go(Route::Song(s.id, None)), |id| {
                Act::Leave(Open::Session(id))
            }),
        });
    }
    for p in &l.projects {
        all.push(Openable {
            title: p.title.clone(),
            kind: "Project",
            detail: p.kind.label().to_owned(),
            act: Act::Go(Route::Project(p.id)),
        });
    }
    for a in &l.albums {
        all.push(Openable {
            title: a.title.clone(),
            kind: "Album",
            detail: format!("{} · {}", a.artist, a.year),
            act: Act::Go(Route::Album(a.id)),
        });
    }
    all
}

/// One row of an overlay.
#[derive(Clone, PartialEq, Debug)]
pub struct Row {
    pub label: String,
    /// Its group (⌘K) or its kind (⌘O).
    pub group: String,
    pub detail: String,
    pub keys: Vec<String>,
    pub act: Act,
    /// Whether it can run now; a row that cannot says so and does nothing.
    pub enabled: bool,
}

/// An app command's shortcut as keycaps: `"Shift+G"` → `["Shift", "G"]`.
fn caps(shortcut: &str) -> Vec<String> {
    shortcut
        .split('+')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The rows an overlay lists for `query`, best first: the library's
/// commands, then the app's (`app`), or what there is to open.
pub fn rows(l: &Library, here: Route, app: &[AppCommand], which: Overlay, query: &str) -> Vec<Row> {
    let mut scored: Vec<(i32, Row)> = match which {
        Overlay::Palette => commands(l, here)
            .into_iter()
            .map(|c| Row {
                label: c.label,
                group: c.group.to_owned(),
                detail: String::new(),
                keys: c.keys.iter().map(|&k| k.to_owned()).collect(),
                act: c.act,
                enabled: true,
            })
            .chain(app.iter().map(|a| Row {
                label: a.label.clone(),
                group: a.group.clone(),
                detail: String::new(),
                keys: caps(&a.keys),
                act: Act::App(a.id.clone()),
                enabled: a.enabled,
            }))
            .filter_map(|r| Some((fuzzy(query, &r.label)?, r)))
            .collect(),
        Overlay::Finder => openables(l)
            .into_iter()
            .filter_map(|o| {
                Some((
                    fuzzy(query, &o.title)?,
                    Row {
                        label: o.title,
                        group: o.kind.to_owned(),
                        detail: o.detail,
                        keys: Vec::new(),
                        act: o.act,
                        enabled: true,
                    },
                ))
            })
            .collect(),
        _ => Vec::new(),
    };
    // Stable: equal scores keep the list's own order (places first).
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, r)| r).take(60).collect()
}

/// A key the menus read, from the page (a DOM key event) or from the
/// window (an app that hears the window's keys itself, as the DAW does).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MenuKey {
    Escape,
    Up,
    Down,
    Enter,
    Backspace,
    /// What the key types (`k`, `?`, ` `).
    Char(String),
}

/// The menus and everything they reach, taken from the context once
/// ([`use_menus`]), so a key heard outside any component (the window's)
/// works the menus as one heard on the page does.
#[derive(Clone, Copy)]
pub struct Menus {
    lib: Signal<Library>,
    pub(crate) nav: Nav,
    pub(crate) keys: Keys,
    app: Option<AppCommands>,
    opener: Option<Opener>,
    show: Option<ShowLibrary>,
}

/// The menus of the window this is drawn in.
pub fn use_menus() -> Menus {
    use_hook(|| Menus {
        lib: consume_context::<LibraryData>().0,
        nav: consume_context::<Nav>(),
        keys: consume_context::<Keys>(),
        app: try_consume_context::<AppCommands>(),
        opener: try_consume_context::<Opener>(),
        show: try_consume_context::<ShowLibrary>(),
    })
}

impl Menus {
    /// The app's commands, as it lists them now; none when it has none.
    fn app_commands(&self) -> Vec<AppCommand> {
        self.app.map_or_else(Vec::new, |a| a.list.call(()))
    }

    /// What the menu that is up lists now.
    fn rows(&self, which: Overlay) -> Vec<Row> {
        rows(
            &self.lib.read(),
            *self.nav.route.peek(),
            &self.app_commands(),
            which,
            &self.keys.query.peek(),
        )
    }

    fn run(&self, act: Act) {
        let (nav, keys) = (self.nav, self.keys);
        keys.close();
        // Somewhere in the library, from over another page: the library
        // comes back to show it.
        let show_library = || {
            if let Some(ShowLibrary(cb)) = self.show {
                cb.call(());
            }
        };
        match act {
            Act::Go(route) => {
                nav.go(route);
                show_library();
            }
            Act::Back => {
                nav.go_back();
                show_library();
            }
            Act::Show(which) => keys.show(which),
            Act::Leave(what) => {
                if let Some(Opener(cb)) = self.opener {
                    cb.call(what);
                }
            }
            Act::App(id) => {
                if let Some(app) = self.app {
                    app.run.call(id);
                }
            }
        }
    }

    /// Whether a menu is up (and takes the keys).
    #[must_use]
    pub fn up(&self) -> bool {
        *self.keys.overlay.peek() != Overlay::None
    }

    /// One key: the keys of a menu that is up, and those that put one up
    /// (⌘K, ⌘O, `?` — the `?` only when `typing` is false: a text field
    /// has the keyboard). `cmd`: ⌘ (or Ctrl, off a Mac) is held. `true`
    /// when the key was the menus'.
    pub fn key(&self, key: &MenuKey, cmd: bool, typing: bool) -> bool {
        let mut keys = self.keys;
        let overlay = *keys.overlay.peek();
        let is = |c: &str, want: &str| c.eq_ignore_ascii_case(want);
        if overlay != Overlay::None {
            match key {
                MenuKey::Escape => keys.close(),
                MenuKey::Down => {
                    let n = self.rows(overlay).len();
                    let p = *keys.picked.peek();
                    keys.picked.set(if n == 0 { 0 } else { (p + 1).min(n - 1) });
                }
                MenuKey::Up => {
                    let p = *keys.picked.peek();
                    keys.picked.set(p.saturating_sub(1));
                }
                MenuKey::Enter => {
                    if let Some(row) = self.rows(overlay).get(*keys.picked.peek())
                        && row.enabled
                    {
                        self.run(row.act.clone());
                    }
                }
                MenuKey::Backspace => {
                    keys.query.write().pop();
                    keys.picked.set(0);
                }
                MenuKey::Char(c) if cmd && is(c, "k") => keys.show(Overlay::Palette),
                MenuKey::Char(c) if cmd && is(c, "o") => keys.show(Overlay::Finder),
                MenuKey::Char(c) if !cmd && overlay != Overlay::Help => {
                    keys.query.write().push_str(c);
                    keys.picked.set(0);
                }
                _ => return false,
            }
            return true;
        }
        match key {
            MenuKey::Char(c) if cmd && is(c, "k") => keys.show(Overlay::Palette),
            MenuKey::Char(c) if cmd && is(c, "o") => keys.show(Overlay::Finder),
            MenuKey::Char(c) if !cmd && !typing && c == "?" => keys.show(Overlay::Help),
            _ => return false,
        }
        true
    }
}

/// A page's key, as the menus read it.
fn menu_key_of(e: &KeyboardEvent) -> Option<MenuKey> {
    Some(match e.key() {
        Key::Escape => MenuKey::Escape,
        Key::ArrowUp => MenuKey::Up,
        Key::ArrowDown => MenuKey::Down,
        Key::Enter => MenuKey::Enter,
        Key::Backspace => MenuKey::Backspace,
        Key::Character(c) => MenuKey::Char(c),
        _ => return None,
    })
}

/// Whether ⌘ (or Ctrl, off a Mac) is held. A Mac's ⌘ arrives as META
/// from some windowing layers and SUPER from others: either counts.
pub(crate) fn command_held(e: &KeyboardEvent) -> bool {
    let m = e.modifiers();
    m.contains(Modifiers::META) || m.contains(Modifiers::SUPER) || m.contains(Modifiers::CONTROL)
}

/// A page's key for the menus ([`Menus::key`]); `true` when it was theirs
/// (and it goes no further).
pub(crate) fn on_menu_key(e: &KeyboardEvent, menus: Menus, typing: bool) -> bool {
    let Some(key) = menu_key_of(e) else {
        return false;
    };
    let taken = menus.key(&key, command_held(e), typing);
    if taken {
        e.prevent_default();
        e.stop_propagation();
    }
    taken
}

/// The library page's own keys: ⌘[ back, ⌘1–6 the places.
pub(crate) fn on_page_key(e: &KeyboardEvent, nav: Nav) -> bool {
    if !command_held(e) {
        return false;
    }
    match e.key() {
        Key::Character(c) if c == "[" => nav.go_back(),
        Key::Character(c) if PLACES.iter().any(|p| p.2 == c) => {
            if let Some((route, ..)) = PLACES.iter().find(|p| p.2 == c) {
                nav.jump(*route);
            }
        }
        _ => return false,
    }
    e.prevent_default();
    true
}

/// The library root's key handler, when no [`super::CommandLayer`] is
/// over it: the menus' keys and the page's.
pub(crate) fn on_key(e: &KeyboardEvent, menus: Menus) {
    if !on_menu_key(e, menus, false) {
        on_page_key(e, menus.nav);
    }
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
    let menus = use_menus();
    let keys = menus.keys;
    let mut picked = keys.picked;
    let query = (keys.query)();
    let here = (menus.nav.route)();
    let app = menus.app_commands();
    let list = rows(&menus.lib.read(), here, &app, which, &query);
    let at = picked().min(list.len().saturating_sub(1));
    let hint = match which {
        Overlay::Finder => "Open a setlist, session, part or song",
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
                for (i, row) in list.into_iter().enumerate() {
                    {
                        let on = i == at;
                        let bg = if on { PICK_BG } else { "transparent" };
                        let ink = if row.enabled { INK } else { INK_3 };
                        let Row { label, group, detail, keys: cap, act, enabled } = row;
                        rsx! {
                            div {
                                key: "{i}",
                                class: "lp",
                                style: "height: 48px; flex-shrink: 0; display: flex; align-items: center; gap: 12px; padding: 0 12px; \
                                        border-radius: 10px; background: {bg}; cursor: pointer;",
                                onmouseenter: move |_| picked.set(i),
                                onclick: move |_| if enabled { menus.run(act.clone()) },
                                {kind_icon(&group)}
                                span { style: "flex: 1; min-width: 0; {BODY} font-size: 15px; color: {ink}; {ONE_LINE}", "{label}" }
                                if !detail.is_empty() {
                                    span { style: "{META} color: {INK_3}; {ONE_LINE} max-width: 180px;", "{detail}" }
                                }
                                if which == Overlay::Palette {
                                    span { style: "{META} color: {INK_3}; white-space: nowrap; overflow: hidden; max-width: 140px;", "{group}" }
                                }
                                for (j, k) in cap.into_iter().enumerate() { Keycap { key: "{j}", text: k } }
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

/// A key the app names (an action's shortcut), drawn as [`Kbd`] draws one.
#[component]
fn Keycap(text: String) -> Element {
    rsx! {
        span {
            style: "min-width: 22px; height: 22px; padding: 0 6px; border-radius: 6px; border: 1px solid {RULE_STRONG}; \
                    border-bottom-width: 2px; background: {SHEET_2}; color: {INK_2}; font-size: 12px; font-weight: 650; \
                    display: inline-flex; align-items: center; justify-content: center; box-sizing: border-box; flex-shrink: 0;",
            "{text}"
        }
    }
}

fn kind_icon(group: &str) -> Element {
    match group {
        "Project" => rsx! { FolderKanban { size: 18, color: INK_3 } },
        "Setlist" | "This setlist" => rsx! { ListMusic { size: 18, color: INK_3 } },
        "Song" | "This song" => rsx! { Music { size: 18, color: INK_3 } },
        "Session" | "This session" => rsx! { AudioWaveform { size: 18, color: INK_3 } },
        "Part" => rsx! { Guitar { size: 18, color: INK_3 } },
        "Album" => rsx! { Disc3 { size: 18, color: INK_3 } },
        _ => rsx! { span { style: "width: 18px; flex-shrink: 0;" } },
    }
}

/// The app's commands, grouped as the app groups them, in its order.
fn grouped(app: Vec<AppCommand>) -> Vec<(String, Vec<AppCommand>)> {
    let mut groups: Vec<(String, Vec<AppCommand>)> = Vec::new();
    for a in app {
        match groups.iter_mut().find(|(g, _)| *g == a.group) {
            Some((_, list)) => list.push(a),
            None => groups.push((a.group.clone(), vec![a])),
        }
    }
    groups
}

/// `?`: every shortcut, grouped — the library's, then every command of
/// the app's (its Action List), with the keys that reach it.
#[component]
fn HelpSheet() -> Element {
    let groups: [(&str, Vec<(&'static [&'static str], &str)>); 3] = [
        (
            "Anywhere",
            vec![
                (&["⌘", "K"], "Command palette"),
                (&["⌘", "O"], "Open a setlist, session, part or song by name"),
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
    let app = grouped(use_menus().app_commands());
    rsx! {
        Scrim {
            div { style: "height: 60px; flex-shrink: 0; display: flex; align-items: center; padding: 0 20px; border-bottom: 1px solid {RULE};",
                span { style: "{HEADING} flex: 1;", "Keyboard shortcuts" }
                Kbd { text: "esc" }
            }
            div { style: "max-height: 70vh; overflow-y: scroll; padding: 8px 20px 18px; display: flex; flex-direction: column;",
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
                for (name, items) in app {
                    div { key: "app-{name}",
                        div { style: "{COLUMN} color: {INK_3}; padding: 16px 0 6px;", "{name}" }
                        for a in items {
                            div { key: "{a.id}", style: "min-height: 40px; display: flex; align-items: center; gap: 6px; border-top: 1px solid {RULE};",
                                span { style: "flex: 1; min-width: 0; font-size: 14px; color: {INK}; {ONE_LINE}", "{a.label}" }
                                for (j, k) in caps(&a.keys).into_iter().enumerate() { Keycap { key: "{j}", text: k } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Act, Overlay, caps, openables, rows};
    use crate::fixture;
    use crate::ui::{AppCommand, Open, Route};

    #[test]
    fn what_plays_opens_and_the_rest_is_a_page() {
        let l = fixture::library();
        let all = openables(&l);
        for o in &all {
            match o.kind {
                "Setlist" => assert!(matches!(o.act, Act::Leave(Open::Rehearse(_)))),
                "Session" => assert!(matches!(o.act, Act::Leave(Open::Session(_)))),
                "Part" => assert!(matches!(o.act, Act::Leave(Open::Part(_)))),
                "Song" => assert!(matches!(
                    o.act,
                    Act::Leave(Open::Session(_)) | Act::Go(Route::Song(..))
                )),
                _ => assert!(matches!(o.act, Act::Go(_))),
            }
        }
        // Every setlist and session is there, and my parts.
        assert!(all.iter().filter(|o| o.kind == "Setlist").count() == l.setlists.len());
        assert!(all.iter().filter(|o| o.kind == "Session").count() == l.sessions.len());
    }

    #[test]
    fn the_palette_holds_the_apps_commands_beside_the_librarys() {
        let l = fixture::library();
        let app = vec![AppCommand {
            id: "SESSION_PLAY".into(),
            label: "Play or stop".into(),
            group: "Transport".into(),
            keys: "Space".into(),
            enabled: true,
        }];
        let all = rows(&l, Route::Home, &app, Overlay::Palette, "");
        assert!(all.iter().any(|r| r.label == "Go to Home"));
        let play = all
            .iter()
            .find(|r| r.act == Act::App("SESSION_PLAY".into()))
            .expect("the app's command is listed");
        assert_eq!(play.keys, vec!["Space".to_owned()]);
        let found = rows(&l, Route::Home, &app, Overlay::Palette, "play or");
        assert_eq!(found.first().map(|r| r.label.as_str()), Some("Play or stop"));
    }

    #[test]
    fn a_shortcut_is_said_as_its_keys() {
        assert_eq!(caps("Shift+G"), vec!["Shift".to_owned(), "G".to_owned()]);
        assert!(caps("").is_empty());
    }
}

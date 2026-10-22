//! Session's global app menu: File, Edit, View, … for a desktop shell's
//! top bar, built from the Action List.
//!
//! Nothing here is a menu kept by hand. Every item is an action in
//! [`crate::actions::list`], filed under a top-level menu by the group the
//! registry gives it (`Session` under File, `Comp` under Comp, …), and
//! showing the key the keymap binds it to (`crate::keys`). What is left
//! out is what a person cannot click: actions that only read (a model's
//! queries), and actions that need arguments there is no default for.
//!
//! [`entries`] and [`layout`] are the whole of it — registry in, tree
//! out — and [`bus`] serves the tree on the session bus as
//! `com.canonical.dbusmenu`, which is what the Loft shell (and Plasma's
//! global menu) reads for a window: see [`start`].

use std::collections::{BTreeSet, HashMap};
use std::hash::BuildHasher;

use architect::action::{ActionInfo, Effect};
use input::{KeyChord, KeyCode};

use crate::keys::Action;

pub mod bus;

pub use bus::start;

/// A key combination as dbusmenu's `shortcut` property spells it: each
/// chord in the order pressed, its modifiers first and then its key —
/// `[["Control", "Shift", "t"]]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shortcut(pub Vec<Vec<String>>);

impl Shortcut {
    /// The keys a binding presses, if every one of them has a name.
    #[must_use]
    pub fn of_chords(chords: &[KeyChord]) -> Option<Self> {
        chords
            .iter()
            .map(chord_keys)
            .collect::<Option<Vec<_>>>()
            .filter(|chords| !chords.is_empty())
            .map(Self)
    }

    /// The keys an action's shortcut label names (`"Ctrl+Shift+T"`,
    /// `"Space"`) — `None` for a label that is not one chord of keys
    /// (`"Mouse 5"`, `"[ / ]"`).
    #[must_use]
    pub fn of_label(label: &str) -> Option<Self> {
        let parts: Vec<&str> = label.split('+').map(str::trim).collect();
        let (key, modifiers) = parts.split_last()?;
        let mut chord = modifiers
            .iter()
            .map(|m| modifier_name(m))
            .collect::<Option<Vec<_>>>()?;
        chord.push(label_key(key)?);
        Some(Self(vec![chord]))
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many keys are held and pressed in all — the shorter, the
    /// better a binding is to show.
    fn presses(&self) -> usize {
        self.0.iter().map(Vec::len).sum()
    }
}

/// A chord's keys: its modifiers, then its key.
fn chord_keys(chord: &KeyChord) -> Option<Vec<String>> {
    let held = [
        (chord.modifiers.ctrl, "Control"),
        (chord.modifiers.alt, "Alt"),
        (chord.modifiers.shift, "Shift"),
        (chord.modifiers.meta, "Super"),
    ];
    let mut keys: Vec<String> = held
        .into_iter()
        .filter(|(down, _)| *down)
        .map(|(_, name)| name.to_owned())
        .collect();
    keys.push(key_name(&chord.key)?);
    Some(keys)
}

/// A key's name as dbusmenu (and X11's keysyms) spell it.
fn key_name(key: &KeyCode) -> Option<String> {
    Some(match key {
        KeyCode::Character(c) if c == " " => "space".to_owned(),
        KeyCode::Character(c) if !c.trim().is_empty() => c.clone(),
        KeyCode::Character(_) => return None,
        KeyCode::ArrowUp => "Up".to_owned(),
        KeyCode::ArrowDown => "Down".to_owned(),
        KeyCode::ArrowLeft => "Left".to_owned(),
        KeyCode::ArrowRight => "Right".to_owned(),
        KeyCode::Enter => "Return".to_owned(),
        KeyCode::Escape => "Escape".to_owned(),
        KeyCode::Tab => "Tab".to_owned(),
        KeyCode::Backspace => "BackSpace".to_owned(),
        KeyCode::Delete => "Delete".to_owned(),
        KeyCode::F(n) => format!("F{n}"),
    })
}

/// A modifier in a shortcut label, by dbusmenu's name for it.
fn modifier_name(label: &str) -> Option<String> {
    let name = match label {
        "Ctrl" | "Control" => "Control",
        "Alt" => "Alt",
        "Shift" => "Shift",
        "Cmd" | "Super" | "Meta" => "Super",
        _ => return None,
    };
    Some(name.to_owned())
}

/// The key in a shortcut label: one character, or a named key.
fn label_key(label: &str) -> Option<String> {
    let mut chars = label.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_whitespace() => Some(c.to_lowercase().collect()),
        _ if label == "Space" => Some("space".to_owned()),
        _ => [
            "Escape",
            "Delete",
            "Home",
            "End",
            "Up",
            "Down",
            "Left",
            "Right",
            "Return",
            "Tab",
            "BackSpace",
            "PageUp",
            "PageDown",
            "Insert",
        ]
        .contains(&label)
        .then(|| label.to_owned())
        .or_else(|| function_key(label)),
    }
}

/// `F1` … `F24`.
fn function_key(label: &str) -> Option<String> {
    let n: u8 = label.strip_prefix('F')?.parse().ok()?;
    (1..=24).contains(&n).then(|| label.to_owned())
}

/// The shortest binding the keymap has for each action the list runs
/// with nothing to pass — what an item shows as its shortcut.
#[must_use]
pub fn keyed(bindings: Vec<(Vec<KeyChord>, Action)>) -> HashMap<&'static str, Shortcut> {
    let mut shortest: HashMap<&'static str, Shortcut> = HashMap::new();
    for (id, shortcut) in bindings.into_iter().filter_map(|(chords, action)| {
        Some((
            crate::actions::plain_id(&action)?,
            Shortcut::of_chords(&chords)?,
        ))
    }) {
        let better = shortest
            .get(id)
            .is_none_or(|kept| (shortcut.presses(), &shortcut.0) < (kept.presses(), &kept.0));
        if better {
            shortest.insert(id, shortcut);
        }
    }
    shortest
}

/// One thing a person can click: an action, its label and its keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The action it runs (`SESSION_PLAY_STOP`).
    pub id: String,
    pub label: String,
    /// The registry's group, which says the menu it goes in.
    pub group: String,
    pub shortcut: Shortcut,
}

/// The actions a menu offers, from the list's `infos` and the keymap's
/// shortcuts ([`keyed`]); an action the keymap does not bind shows the
/// shortcut its registry entry names, if any.
#[must_use]
pub fn entries<S: BuildHasher>(
    infos: &[ActionInfo],
    keyed: &HashMap<&str, Shortcut, S>,
) -> Vec<Entry> {
    infos
        .iter()
        .filter(|info| clickable(info))
        .map(|info| Entry {
            id: info.id.clone(),
            label: info.display_name.clone(),
            group: group_of(info).to_owned(),
            shortcut: keyed
                .get(info.id.as_str())
                .cloned()
                .or_else(|| Shortcut::of_label(&info.shortcut))
                .unwrap_or_default(),
        })
        .collect()
}

/// Actions that take arguments, every one optional, whose defaults are
/// what a person means by clicking them: save the song where it is, a
/// new song from nothing.
const DEFAULTS_ARE_A_CLICK: &[&str] = &["SESSION_SAVE", "SESSION_NEW"];

/// Actions that act on what is under the mouse — which, from a menu, is
/// the menu.
const UNDER_THE_MOUSE: &[&str] = &["SESSION_TAKE_UPRANK_MOUSE", "SESSION_TAKE_DOWNRANK_MOUSE"];

/// Whether a person can click it: it changes something, and running it
/// with nothing passed means something.
fn clickable(info: &ActionInfo) -> bool {
    let id = info.id.as_str();
    info.effect != Effect::ReadOnly
        && !UNDER_THE_MOUSE.contains(&id)
        && (takes_nothing(info.params_schema.as_deref()) || DEFAULTS_ARE_A_CLICK.contains(&id))
}

/// Whether a params schema asks for nothing: there is none, or it is an
/// object with no properties (Undo's `{}`).
fn takes_nothing(schema: Option<&str>) -> bool {
    let Some(schema) = schema else {
        return true;
    };
    serde_json::from_str::<serde_json::Value>(schema).is_ok_and(|schema| {
        schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
    })
}

/// Undo and Redo: where every app keeps them, though the registry files
/// them with the session's own.
const HISTORY: &[&str] = &["SESSION_UNDO", "SESSION_REDO"];

/// The group [`HISTORY`] goes in.
const HISTORY_GROUP: &str = "History";

/// The group a menu files an action under.
fn group_of(info: &ActionInfo) -> &str {
    if HISTORY.contains(&info.id.as_str()) {
        HISTORY_GROUP
    } else {
        &info.group
    }
}

/// The menus, in the order they stand, each with the registry groups it
/// holds — a section apiece, in this order. A group not named here gets
/// a menu of its own, after these.
const MENUS: &[(&str, &[&str])] = &[
    ("_File", &["Session"]),
    ("_Edit", &[HISTORY_GROUP, "Edit"]),
    ("_View", &["View", "Navigate"]),
    ("_Transport", &["Transport", "Record"]),
    ("T_rack", &["Tracks", "Structure", "Tempo"]),
    ("_Comp", &["Comp"]),
    ("_Drums", &["Drums"]),
    (
        "_Mix",
        &["Mix", "FX", "Automation", "Vocals", "Bass", "Signal"],
    ),
];

/// One node of the menu tree, as dbusmenu numbers it: the root is 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: i32,
    pub kind: Kind,
    pub children: Vec<Self>,
}

/// What a node is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The menu bar.
    Root,
    /// A top-level menu; its label carries its mnemonic (`_File`).
    Menu {
        label: String,
    },
    /// An action to run.
    Item {
        label: String,
        action: String,
        shortcut: Shortcut,
    },
    Separator,
}

/// A property's value, as dbusmenu types it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prop {
    Str(String),
    Keys(Vec<Vec<String>>),
}

impl Node {
    /// The node numbered `id`, at any depth.
    #[must_use]
    pub fn find(&self, id: i32) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    /// The action the item numbered `id` runs.
    #[must_use]
    pub fn action(&self, id: i32) -> Option<&str> {
        match &self.find(id)?.kind {
            Kind::Item { action, .. } => Some(action),
            _ => None,
        }
    }

    /// Every node, depth first from this one.
    pub fn walk(&self) -> Box<dyn Iterator<Item = &Self> + '_> {
        Box::new(std::iter::once(self).chain(self.children.iter().flat_map(Self::walk)))
    }

    /// Its dbusmenu properties, by name. A property left out is the
    /// spec's default (enabled, visible, a plain item).
    #[must_use]
    pub fn props(&self) -> Vec<(&'static str, Prop)> {
        let submenu = ("children-display", Prop::Str("submenu".to_owned()));
        match &self.kind {
            Kind::Root => vec![submenu],
            Kind::Menu { label } => vec![("label", Prop::Str(label.clone())), submenu],
            Kind::Item {
                label, shortcut, ..
            } => std::iter::once(("label", Prop::Str(escape_mnemonic(label))))
                .chain((!shortcut.is_empty()).then(|| ("shortcut", Prop::Keys(shortcut.0.clone()))))
                .collect(),
            Kind::Separator => vec![("type", Prop::Str("separator".to_owned()))],
        }
    }
}

/// A label shown as written: `_` marks a mnemonic in dbusmenu, so a
/// literal one is doubled.
fn escape_mnemonic(label: &str) -> String {
    label.replace('_', "__")
}

/// The menu bar for `entries`.
///
/// The menus stand in [`MENUS`] order (then any group they do not name,
/// alphabetically), each holding its groups' items as sections between
/// separators, each section by label. A menu with nothing in it is left
/// out. Numbered depth first from the root's 0, so the same entries
/// always number the same.
#[must_use]
pub fn layout(entries: &[Entry]) -> Node {
    let mut ids = Ids::default();
    let root = ids.next();
    let menus = menu_groups(entries)
        .into_iter()
        .filter_map(|(label, groups)| menu(&label, &groups, entries, &mut ids))
        .collect();
    Node {
        id: root,
        kind: Kind::Root,
        children: menus,
    }
}

/// The menus and the groups each holds: [`MENUS`], then a menu for each
/// group they leave out.
fn menu_groups(entries: &[Entry]) -> Vec<(String, Vec<String>)> {
    let named: BTreeSet<&str> = MENUS.iter().flat_map(|(_, g)| g.iter().copied()).collect();
    let unnamed: BTreeSet<&str> = entries
        .iter()
        .map(|e| e.group.as_str())
        .filter(|g| !named.contains(g))
        .collect();
    MENUS
        .iter()
        .map(|(label, groups)| {
            (
                (*label).to_owned(),
                groups.iter().map(|g| (*g).to_owned()).collect(),
            )
        })
        .chain(
            unnamed
                .into_iter()
                .map(|g| (escape_mnemonic(g), vec![g.to_owned()])),
        )
        .collect()
}

/// One top-level menu: `groups`' entries, a section each; `None` when
/// none of them has any.
fn menu(label: &str, groups: &[String], entries: &[Entry], ids: &mut Ids) -> Option<Node> {
    let sections: Vec<Vec<&Entry>> = groups
        .iter()
        .map(|group| section(group, entries))
        .filter(|section| !section.is_empty())
        .collect();
    if sections.is_empty() {
        return None;
    }
    let id = ids.next();
    let mut children = Vec::new();
    for (i, section) in sections.into_iter().enumerate() {
        if i > 0 {
            children.push(Node {
                id: ids.next(),
                kind: Kind::Separator,
                children: Vec::new(),
            });
        }
        children.extend(section.into_iter().map(|entry| item(entry, ids)));
    }
    Some(Node {
        id,
        kind: Kind::Menu {
            label: label.to_owned(),
        },
        children,
    })
}

/// `group`'s entries in the order a menu lists them: [`HISTORY`]'s own
/// order first, then by label.
fn section<'e>(group: &str, entries: &'e [Entry]) -> Vec<&'e Entry> {
    let mut section: Vec<&Entry> = entries.iter().filter(|e| e.group == group).collect();
    section.sort_by_cached_key(|e| {
        (
            HISTORY
                .iter()
                .position(|id| *id == e.id)
                .unwrap_or(usize::MAX),
            e.label.to_lowercase(),
            e.id.clone(),
        )
    });
    section
}

fn item(entry: &Entry, ids: &mut Ids) -> Node {
    Node {
        id: ids.next(),
        kind: Kind::Item {
            label: entry.label.clone(),
            action: entry.id.clone(),
            shortcut: entry.shortcut.clone(),
        },
        children: Vec::new(),
    }
}

/// Node ids, handed out in order from 0.
#[derive(Default)]
struct Ids(i32);

impl Ids {
    const fn next(&mut self) -> i32 {
        let id = self.0;
        self.0 = id.saturating_add(1);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use input::Modifiers;

    fn info(id: &str, group: &str, effect: Effect, params: Option<&str>) -> ActionInfo {
        ActionInfo {
            id: id.to_owned(),
            display_name: id.to_lowercase(),
            description: String::new(),
            category: "Session".to_owned(),
            group: group.to_owned(),
            shortcut: String::new(),
            effect,
            params_schema: params.map(str::to_owned),
            result_schema: None,
            enabled: true,
            disabled_reason: None,
        }
    }

    fn entry(id: &str, label: &str, group: &str) -> Entry {
        Entry {
            id: id.to_owned(),
            label: label.to_owned(),
            group: group.to_owned(),
            shortcut: Shortcut::default(),
        }
    }

    fn labels(node: &Node) -> Vec<String> {
        node.children
            .iter()
            .map(|n| match &n.kind {
                Kind::Menu { label } | Kind::Item { label, .. } => label.clone(),
                Kind::Separator => "-".to_owned(),
                Kind::Root => "root".to_owned(),
            })
            .collect()
    }

    fn keys(chords: &[&[&str]]) -> Shortcut {
        Shortcut(
            chords
                .iter()
                .map(|c| c.iter().map(|k| (*k).to_owned()).collect())
                .collect(),
        )
    }

    #[test]
    fn a_menu_offers_what_a_person_can_click() {
        let required = r#"{"properties":{"track":{"type":"string"}},"required":["track"]}"#;
        let optional = r#"{"properties":{"to":{"anyOf":[{"type":"string"},{"type":"null"}]}}}"#;
        let infos = [
            info("SESSION_PLAY_STOP", "Transport", Effect::Mutating, None),
            info(
                "SESSION_UNDO",
                "Session",
                Effect::Mutating,
                Some(r#"{"properties":{}}"#),
            ),
            info("SESSION_SAVE", "Session", Effect::Mutating, Some(optional)),
            info("SESSION_OPEN", "Session", Effect::Mutating, Some(optional)),
            info("TRACK_RENAME", "Tracks", Effect::Mutating, Some(required)),
            info("PERF_RECENT", "Data", Effect::ReadOnly, None),
            info("SESSION_TAKE_UPRANK_MOUSE", "Comp", Effect::Mutating, None),
            info("SESSION_DELETE_ITEMS", "Edit", Effect::Destructive, None),
        ];
        let offered: Vec<(String, String)> = entries(&infos, &HashMap::new())
            .into_iter()
            .map(|e| (e.id, e.group))
            .collect();
        let pairs = |v: &[(&str, &str)]| -> Vec<(String, String)> {
            v.iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
                .collect()
        };
        assert_eq!(
            offered,
            pairs(&[
                ("SESSION_PLAY_STOP", "Transport"),
                ("SESSION_UNDO", "History"),
                ("SESSION_SAVE", "Session"),
                ("SESSION_DELETE_ITEMS", "Edit"),
            ])
        );
    }

    #[test]
    fn the_keymap_wins_and_the_registry_label_stands_in() {
        let mut bound = info("SESSION_SAVE", "Session", Effect::Mutating, None);
        bound.shortcut = "Ctrl+Alt+S".to_owned();
        let mut unbound = info("SESSION_PLAY_STOP", "Transport", Effect::Mutating, None);
        unbound.shortcut = "Space".to_owned();
        let mut mouse = info("SESSION_GO_TO_START", "Transport", Effect::Mutating, None);
        mouse.shortcut = "Mouse 5".to_owned();
        let keyed = HashMap::from([("SESSION_SAVE", keys(&[&["Control", "s"]]))]);
        let shown: Vec<Shortcut> = entries(&[bound, unbound, mouse], &keyed)
            .into_iter()
            .map(|e| e.shortcut)
            .collect();
        assert_eq!(
            shown,
            [
                keys(&[&["Control", "s"]]),
                keys(&[&["space"]]),
                Shortcut::default()
            ]
        );
    }

    #[test]
    fn shortcut_labels_read_as_chords() {
        assert_eq!(
            Shortcut::of_label("Ctrl+Shift+T"),
            Some(keys(&[&["Control", "Shift", "t"]]))
        );
        assert_eq!(Shortcut::of_label("Alt+G"), Some(keys(&[&["Alt", "g"]])));
        assert_eq!(Shortcut::of_label("Delete"), Some(keys(&[&["Delete"]])));
        assert_eq!(Shortcut::of_label("F5"), Some(keys(&[&["F5"]])));
        assert_eq!(Shortcut::of_label("Mouse 4"), None);
        assert_eq!(Shortcut::of_label("[ / ]"), None);
        assert_eq!(Shortcut::of_label(""), None);
    }

    #[test]
    fn the_shortest_binding_is_the_one_shown() {
        let chord = |key: &str, ctrl: bool| KeyChord {
            key: KeyCode::Character(key.to_owned()),
            modifiers: Modifiers {
                ctrl,
                ..Modifiers::NONE
            },
        };
        let bindings = vec![
            (vec![chord("x", false), chord("t", false)], Action::CompBest),
            (vec![chord("t", true)], Action::CompBest),
            (vec![chord(" ", false)], Action::PlayStop),
            // Takes an argument: not a menu's to show.
            (vec![chord("1", true)], Action::CompTake(1)),
        ];
        let keyed = keyed(bindings);
        assert_eq!(
            keyed.get("SESSION_COMP_BEST"),
            Some(&keys(&[&["Control", "t"]]))
        );
        assert_eq!(keyed.get("SESSION_PLAY_STOP"), Some(&keys(&[&["space"]])));
        assert_eq!(keyed.len(), 2);
    }

    #[test]
    fn menus_stand_in_app_order_with_a_section_per_group() {
        let entries = [
            entry("SESSION_REDO", "Redo", "History"),
            entry("SESSION_SELECT_ALL", "Select all items", "Edit"),
            entry("SESSION_UNDO", "Undo", "History"),
            entry("SESSION_SAVE", "Save the song", "Session"),
            entry("SESSION_NEW", "New project", "Session"),
            entry("SESSION_PLAY_STOP", "Play/stop", "Transport"),
            entry("SESSION_TOGGLE_RECORD", "Record", "Transport"),
            entry("SESSION_COMP_NEXT", "Comp the next take", "Comp"),
            entry("SOMETHING_NEW", "Something", "Lyrics"),
        ];
        let root = layout(&entries);
        assert_eq!(root.id, 0);
        assert_eq!(
            labels(&root),
            ["_File", "_Edit", "_Transport", "_Comp", "Lyrics"]
        );
        let edit = &root.children[1];
        assert_eq!(labels(edit), ["Undo", "Redo", "-", "Select all items"]);
        let file = &root.children[0];
        assert_eq!(labels(file), ["New project", "Save the song"]);
        let transport = &root.children[2];
        assert_eq!(labels(transport), ["Play/stop", "Record"]);
    }

    #[test]
    fn every_node_has_its_own_id_and_an_item_names_its_action() {
        let entries = [
            entry("SESSION_UNDO", "Undo", "History"),
            entry("SESSION_SELECT_ALL", "Select all items", "Edit"),
            entry("SESSION_PLAY_STOP", "Play/stop", "Transport"),
        ];
        let root = layout(&entries);
        let ids: Vec<i32> = root.walk().map(|n| n.id).collect();
        assert_eq!(ids, (0..7).collect::<Vec<_>>());
        let play = root
            .walk()
            .find(|n| matches!(&n.kind, Kind::Item { label, .. } if label == "Play/stop"))
            .map(|n| n.id);
        assert_eq!(
            play.and_then(|id| root.action(id)),
            Some("SESSION_PLAY_STOP")
        );
        assert_eq!(root.action(1), None, "a menu runs nothing");
        assert_eq!(root.action(99), None);
    }

    #[test]
    fn props_say_what_dbusmenu_reads() {
        let mut save = entry("SESSION_SAVE", "Save_as", "Session");
        save.shortcut = keys(&[&["Control", "s"]]);
        let root = layout(&[save, entry("SESSION_UNDO", "Undo", "History")]);
        let file = &root.children[0];
        assert_eq!(
            file.props(),
            [
                ("label", Prop::Str("_File".to_owned())),
                ("children-display", Prop::Str("submenu".to_owned())),
            ]
        );
        assert_eq!(
            file.children[0].props(),
            [
                ("label", Prop::Str("Save__as".to_owned())),
                (
                    "shortcut",
                    Prop::Keys(vec![vec!["Control".into(), "s".into()]])
                ),
            ]
        );
        let undo = &root.children[1].children[0];
        assert_eq!(undo.props(), [("label", Prop::Str("Undo".to_owned()))]);
    }

    /// Session's own registry and keymap, through the whole path.
    #[test]
    fn sessions_registry_makes_a_menu_bar() {
        let keyed = keyed(crate::keys::Keys::load().bindings());
        let root = layout(&entries(&crate::actions::list().infos(), &keyed));
        let menus = labels(&root);
        for menu in ["_File", "_Edit", "_View", "_Transport", "_Comp", "_Drums"] {
            assert!(menus.iter().any(|m| m == menu), "{menu} in {menus:?}");
        }
        let item = |action: &str| {
            root.walk()
                .find(|n| matches!(&n.kind, Kind::Item { action: a, .. } if a == action))
        };
        assert!(item("SESSION_PLAY_STOP").is_some());
        assert!(item("SESSION_UNDO").is_some());
        assert!(
            item("PERF_RECENT").is_none(),
            "a model's read is not a menu's"
        );
        assert!(item("TRACK_RENAME").is_none(), "it needs a track");
        let comp_keep = item("SESSION_COMP_KEEP").map(Node::props);
        assert!(
            comp_keep.is_some_and(|p| p.iter().any(|(k, _)| *k == "shortcut")),
            "Ctrl+Shift+T shown"
        );
    }
}

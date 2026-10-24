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
use architect_input::{KeyChord, KeyCode};

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

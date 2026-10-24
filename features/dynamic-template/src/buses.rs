//! The canonical FTS bus tree — where each instrument group hands off to the
//! mix.
//!
//! [`colors`](crate::colors) answers "what color is this group?" and
//! [`layouts`](crate::layouts) answers "what TCP/MCP layout does it get?".
//! This module answers the third per-group question: **what does it feed?**
//! Like those two it resolves by canonical group path (top-level first, e.g.
//! `["Vocals", "Lead"]`), most specific first, so a sub-group can attach
//! somewhere its parent does not.
//!
//! # Shape
//!
//! Group buses sum into one of two stem buses — `INST BUS` and `VOX BUS` —
//! which sum into `MIX BUS`. That middle tier is what makes an instrumental
//! and an acappella fall out of the session for free: mute one stem bus and
//! the other is already printed.
//!
//! ```text
//! MIX BUS                            (always present)
//! ├─ INST BUS
//! │  ├─ DRUM BUS      ← Drums
//! │  ├─ PERC BUS      ← Percussion
//! │  ├─ BASS BUS      ← Bass
//! │  ├─ GUITAR BUS    ← Guitars (steel, banjo, mandolin)
//! │  │  ├─ ACOUSTIC BUS ← Guitars/Acoustic
//! │  │  └─ ELECTRIC BUS ← Guitars/Electric
//! │  ├─ KEYS BUS      ← Keys, Synths
//! │  ├─ ORCH BUS      ← Orchestra, Horns, Fiddle, Harmonica
//! │  └─ FX BUS        ← SFX, Tracks
//! └─ VOX BUS
//!    ├─ LEAD VOX BUS  ← Vocals, Vocals/Lead
//!    └─ BGV BUS       ← Vocals/BGVs, Choir
//!
//! CLICK + GUIDE BUS   ← Guide        (parallel to MIX BUS, off master)
//! HEADPHONE MIXES     ← Headphones (per-player cue mixes)
//! TALKBACK BUS        ← Talkback
//! UTILITY BUS         ← Reference (incl. Reference/Stem Split)
//! ```
//!
//! `INST BUS` and `VOX BUS` have no group sources of their own — nothing
//! attaches to them directly, they only sum the group buses beneath them. They
//! materialize when any of those does, via the ancestor walk in
//! [`buses_for_paths`].
//!
//! # Buses are tracks
//!
//! Each bus is a real track, nested as a **folder track** inside the bus it
//! feeds: `DRUM BUS` is a child of `INST BUS`, which is a child of `MIX BUS`.
//! So a bus reaches its parent by the ordinary folder parent-send —
//! [`TemplateBus::parent`] is that nesting, not a send.
//!
//! Instrument folders are the other case: they sit elsewhere in the project
//! and reach their bus by an explicit send
//! ([`NodeRouting::to_bus`](crate::proto::NodeRouting::to_bus),
//! which drops the parent send). [`bus_nodes`] renders the bus tree as the
//! nested [`TemplateNode`] folder tracks a DAW layer can create directly.
//!
//! The monitor buses sit **beside** `MIX BUS` rather than inside it: click,
//! count-ins, cues, per-player headphone mixes, talkback chatter, the reference
//! master and stem-split imports are all monitor-only, and routing them through
//! `MIX BUS` would print them into any bounce or stem unless muted by hand.
//!
//! Some groups reach **no** bus at all, deliberately: a `VCA` carries no audio,
//! only fader control, so a send from one would be silent. See
//! [`is_deliberately_unrouted`].
//!
//! **No bus exists unless something feeds it.** Every bus is
//! [`WhenPopulated`](BusMaterialization::WhenPopulated), including `MIX BUS`:
//! [`buses_for_paths`] starts from the group paths a session actually contains
//! and keeps only the buses those reach, plus their ancestors. An empty project
//! gets no buses; an acoustic-only session gets `ACOUSTIC BUS` but no
//! `ELECTRIC BUS`, and no `VOX BUS` at all.

use std::collections::HashSet;

use crate::proto::{BusMaterialization, TemplateBus, TemplateNode};

use crate::colors::color_for_path;

/// Names of the buses in the canonical tree, so callers can reference one
/// without spelling the string.
pub mod names {
    /// The mix sum. Everything musical ends up here; always present.
    pub const MIX: &str = "MIX BUS";
    /// The instrumental stem: every group bus that is not a vocal.
    pub const INST: &str = "INST BUS";
    /// The vocal stem: lead and background vocals.
    pub const VOX: &str = "VOX BUS";
    /// The drum kit.
    pub const DRUM: &str = "DRUM BUS";
    /// Percussion — its own bus, so it can be balanced (or muted) apart
    /// from the kit.
    pub const PERC: &str = "PERC BUS";
    /// Bass in all its forms (electric, synth, upright).
    pub const BASS: &str = "BASS BUS";
    /// Every guitar. Acoustics and electrics sum through their own buses
    /// beneath this one; steel, banjo and mandolin attach here directly.
    pub const GUITAR: &str = "GUITAR BUS";
    /// Acoustic guitars.
    pub const ACOUSTIC: &str = "ACOUSTIC BUS";
    /// Electric guitars.
    pub const ELECTRIC: &str = "ELECTRIC BUS";
    /// Keys and synths.
    pub const KEYS: &str = "KEYS BUS";
    /// Orchestral and other acoustic-ensemble material.
    pub const ORCH: &str = "ORCH BUS";
    /// Sound design and backing/playback tracks.
    pub const FX: &str = "FX BUS";
    /// Lead vocal.
    pub const LEAD_VOX: &str = "LEAD VOX BUS";
    /// Background vocals and choir.
    pub const BGV: &str = "BGV BUS";
    /// Click, count-ins, cues, section markers. Off master, never in the mix.
    pub const GUIDE: &str = "CLICK + GUIDE BUS";
    /// Reference mixes and stem-split imports. Off master, never in the mix.
    pub const UTILITY: &str = "UTILITY BUS";
    /// Talkback / comms mics. Off master, never in the mix.
    pub const TALKBACK: &str = "TALKBACK BUS";
    /// Per-player monitor mixes. Off master, never in the mix.
    pub const HEADPHONES: &str = "HEADPHONE MIXES";
}

/// One entry in the bus tree: a bus, what feeds it, and where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusSpec {
    /// Bus name — one of [`names`].
    pub name: &'static str,
    /// The bus this one feeds, or `None` to go straight to the project master.
    pub parent: Option<&'static str>,
    /// Channel count (2 = stereo).
    pub channels: u32,
    /// Canonical group paths that attach here, most specific first. A group
    /// path attaches to the *longest* matching entry across the whole table.
    pub sources: &'static [&'static [&'static str]],
    /// Other names this bus goes by in projects that predate the taxonomy.
    /// Matched case-insensitively when looking for an existing bus, so
    /// applying the template to a session that already has a `GUITAR BUS`
    /// reuses it instead of creating a second `GTR BUS` beside it.
    pub aliases: &'static [&'static str],
    /// Whether the bus is emitted when nothing routes into it.
    pub materialize: BusMaterialization,
}

impl BusSpec {
    /// Every name this bus answers to: its canonical name first, then its
    /// aliases. [`apply_buses`](crate::apply::apply_buses) tries these in
    /// order when looking for a bus a project already has.
    pub fn known_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.name).chain(self.aliases.iter().copied())
    }

    /// Resolve this bus's color from the first of its source group paths that
    /// has one — so `DRUM BUS` takes the drums color, `GTR BUS` the guitars
    /// color. A bus that only sums other buses has no color of its own.
    #[must_use]
    pub fn color_hex(&self) -> Option<String> {
        self.sources
            .iter()
            .find_map(|path| color_for_path(path))
            .map(color_palette::Color::to_hex_string)
    }

    /// This spec as the pure-data proto type.
    pub fn to_template_bus(self) -> TemplateBus {
        TemplateBus {
            name: self.name.to_string(),
            channels: self.channels,
            parent: self.parent.map(str::to_string),
            sources: self
                .sources
                .iter()
                .map(|p| p.iter().map(|s| (*s).to_string()).collect())
                .collect(),
            color_hex: self.color_hex(),
            materialize: self.materialize,
        }
    }
}

const fn bus(
    name: &'static str,
    parent: Option<&'static str>,
    sources: &'static [&'static [&'static str]],
) -> BusSpec {
    BusSpec {
        name,
        parent,
        channels: 2,
        sources,
        aliases: &[],
        materialize: BusMaterialization::WhenPopulated,
    }
}

/// A bus that also answers to `aliases` — names the same bus already carries in
/// existing sessions.
const fn aliased(
    name: &'static str,
    parent: Option<&'static str>,
    sources: &'static [&'static [&'static str]],
    aliases: &'static [&'static str],
) -> BusSpec {
    BusSpec {
        aliases,
        ..bus(name, parent, sources)
    }
}

/// The canonical FTS bus tree, in display order: the mix sum first, then its
/// group buses, then the two off-master monitor buses.
///
/// Ordering within `sources` does not matter for resolution — [`bus_for_path`]
/// picks the longest match across the whole table — but it does decide a bus's
/// color, which is taken from the first source with one.
pub const BUS_TREE: &[BusSpec] = &[
    bus(names::MIX, None, &[]),
    // The instrumental stem and the group buses feeding it.
    bus(names::INST, Some(names::MIX), &[]),
    bus(names::DRUM, Some(names::INST), &[&["Drums"]]),
    aliased(
        names::PERC,
        Some(names::INST),
        &[&["Percussion"]],
        &["PERCUSSION BUS"],
    ),
    bus(names::BASS, Some(names::INST), &[&["Bass"]]),
    aliased(
        names::GUITAR,
        Some(names::INST),
        &[&["Guitars"]],
        &["GTR BUS", "GTRS BUS", "GUITARS BUS"],
    ),
    aliased(
        names::ACOUSTIC,
        Some(names::GUITAR),
        &[&["Guitars", "Acoustic"]],
        &[
            "Guitar A BUS",
            "GTR A BUS",
            "AC GTR BUS",
            "ACOUSTIC GTR BUS",
        ],
    ),
    aliased(
        names::ELECTRIC,
        Some(names::GUITAR),
        &[&["Guitars", "Electric"]],
        &[
            "Guitar E BUS",
            "GTR E BUS",
            "EL GTR BUS",
            "ELECTRIC GTR BUS",
        ],
    ),
    aliased(
        names::KEYS,
        Some(names::INST),
        &[&["Keys"], &["Synths"]],
        &["KEY BUS", "SYNTH BUS"],
    ),
    bus(
        names::ORCH,
        Some(names::INST),
        &[&["Orchestra"], &["Horns"], &["Fiddle"], &["Harmonica"]],
    ),
    aliased(
        names::FX,
        Some(names::INST),
        &[&["SFX"], &["Tracks"]],
        &["SFX BUS", "TRACKS BUS", "TRK BUS"],
    ),
    // The vocal stem and the group buses feeding it.
    bus(names::VOX, Some(names::MIX), &[]),
    // A bare "Vocals" track is one that classified as a vocal but not as Lead
    // or BGVs — treat it as a lead, which is what an unqualified vocal is.
    bus(
        names::LEAD_VOX,
        Some(names::VOX),
        &[&["Vocals", "Lead"], &["Vocals"]],
    ),
    bus(
        names::BGV,
        Some(names::VOX),
        &[&["Vocals", "BGVs"], &["Choir"]],
    ),
    // Off master, in parallel with MIX BUS — monitor-only, never printed.
    aliased(
        names::GUIDE,
        None,
        &[&["Guide"]],
        &["GUIDE BUS", "CLICK BUS", "CLICK/GUIDE BUS", "CLICK + GUIDE"],
    ),
    aliased(
        names::HEADPHONES,
        None,
        &[&["Headphones"]],
        &["HEADPHONE BUS", "CUE BUS", "HP BUS", "HEADPHONES"],
    ),
    aliased(
        names::TALKBACK,
        None,
        &[&["Talkback"]],
        &["TB BUS", "COMMS BUS"],
    ),
    bus(
        names::UTILITY,
        None,
        &[&["Reference"], &["Reference", "Stem Split"]],
    ),
];

/// The bus a canonical group path attaches to, or `None` if it attaches
/// nowhere (a sub-folder inherits its parent's attachment rather than getting
/// its own).
///
/// Matching is case-insensitive and takes the **longest** source path that is
/// a prefix of `path`, so `["Vocals", "BGVs", "Performer"]` resolves to
/// `BGV BUS` rather than the shorter `["Vocals"]` entry on `LEAD VOX BUS`.
///
/// # Example
/// ```
/// use crate::buses::{bus_for_path, names};
///
/// assert_eq!(bus_for_path(&["Drums".into()]), Some(names::DRUM));
/// assert_eq!(bus_for_path(&["Vocals".into(), "BGVs".into()]), Some(names::BGV));
///
/// // Electric has a bus of its own, so the longer source wins.
/// assert_eq!(bus_for_path(&["Guitars".into(), "Electric".into()]), Some(names::ELECTRIC));
/// // Steel does not, so it falls back to the shorter ["Guitars"] source.
/// assert_eq!(bus_for_path(&["Guitars".into(), "Steel".into()]), Some(names::GUITAR));
///
/// // A VCA carries no audio, so it deliberately attaches nowhere.
/// assert_eq!(bus_for_path(&["VCA".into()]), None);
/// ```
#[must_use]
pub fn bus_for_path(path: &[String]) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for spec in BUS_TREE {
        for source in spec.sources {
            if source.len() <= path.len()
                && source
                    .iter()
                    .zip(path)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
                && best.is_none_or(|(len, _)| source.len() > len)
            {
                best = Some((source.len(), spec.name));
            }
        }
    }
    best.map(|(_, name)| name)
}

/// Whether `path` is exactly a bus attachment point — the node that carries
/// the [`NodeRouting`](crate::proto::NodeRouting) into a bus, as
/// opposed to a descendant that merely inherits it.
#[must_use]
pub fn is_attachment_point(path: &[String]) -> bool {
    BUS_TREE.iter().any(|spec| {
        spec.sources.iter().any(|source| {
            source.len() == path.len()
                && source
                    .iter()
                    .zip(path)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
    })
}

/// The spec for a bus by canonical name or [`alias`](BusSpec::aliases),
/// case-insensitively.
#[must_use]
pub fn spec(name: &str) -> Option<&'static BusSpec> {
    let needle = name.trim();
    BUS_TREE.iter().find(|s| {
        s.name.eq_ignore_ascii_case(needle)
            || s.aliases.iter().any(|a| a.eq_ignore_ascii_case(needle))
    })
}

/// Canonical group paths that deliberately reach no bus.
///
/// A `VCA` is fader control, not audio: a send from one carries no signal, so
/// "no bus" is the finished state rather than a gap. Callers use this to tell a
/// track that is *done* from one that nothing recognised and which therefore
/// needs a human — the difference between leaving `BAND RECORD VCA` alone and
/// sweeping it into `UNSORTED`.
pub const UNROUTED_GROUPS: &[&str] = &["VCA"];

/// Whether `path` belongs to a group that deliberately reaches no bus.
#[must_use]
pub fn is_deliberately_unrouted(path: &[String]) -> bool {
    path.first()
        .is_some_and(|top| UNROUTED_GROUPS.iter().any(|g| g.eq_ignore_ascii_case(top)))
}

/// Whether `name` is a bus track — under its canonical name or any
/// [`alias`](BusSpec::aliases).
///
/// Bus tracks must be excluded before classifying a project's content, because
/// their names classify as the very thing they carry: `VOX BUS` matches the
/// vocal patterns, `DRUM BUS` the drum ones. Left in, an existing bus counts as
/// content justifying its own existence, and a project with buses but no vocals
/// still grows a vocal bus — defeating the whole point of building a bus only
/// when something feeds it.
#[must_use]
pub fn is_bus_name(name: &str) -> bool {
    spec(name).is_some()
}

/// The buses a session containing `paths` needs, in [`BUS_TREE`] order.
///
/// A bus is included when a group path attaches to it, when one of its
/// descendant buses is included, or when it is
/// [`Always`](BusMaterialization::Always) (nothing is, today — see the module
/// docs). Nothing else survives: a session of drums, bass, two guitars, keys
/// and one vocal gets ten buses, not the full thirteen, and a session with no
/// tracks at all gets none.
pub fn buses_for_paths<'a>(paths: impl IntoIterator<Item = &'a [String]>) -> Vec<TemplateBus> {
    let mut needed: HashSet<&'static str> = BUS_TREE
        .iter()
        .filter(|s| s.materialize == BusMaterialization::Always)
        .map(|s| s.name)
        .collect();

    for path in paths {
        if let Some(name) = bus_for_path(path) {
            needed.insert(name);
        }
    }

    // Pull in every ancestor of an included bus. The tree is shallow and
    // acyclic, so walking each included bus's parent chain once suffices.
    let direct: Vec<&'static str> = needed.iter().copied().collect();
    for name in direct {
        let mut cursor = spec(name).and_then(|s| s.parent);
        while let Some(parent) = cursor {
            needed.insert(parent);
            cursor = spec(parent).and_then(|s| s.parent);
        }
    }

    BUS_TREE
        .iter()
        .filter(|s| needed.contains(s.name))
        .map(|s| s.to_template_bus())
        .collect()
}

/// The full bus tree as proto types, ignoring materialization — every bus,
/// populated or not. Used for the golden template, which is the schema rather
/// than any one session.
#[must_use]
pub fn all_buses() -> Vec<TemplateBus> {
    BUS_TREE.iter().map(|s| s.to_template_bus()).collect()
}

/// Render `buses` as nested [`TemplateNode`] folder tracks, ready for a DAW
/// layer to create in order.
///
/// A bus becomes a child of the node named by its
/// [`parent`](TemplateBus::parent); buses with no parent come back as roots,
/// in `buses` order. A bus with children is a [`Folder`](crate::proto::NodeKind::Folder)
/// and reaches its parent by the folder parent-send; a leaf bus like
/// `DRUM BUS` is a [`Track`](crate::proto::NodeKind::Track).
///
/// A bus whose parent is not in `buses` is emitted as a root rather than
/// dropped, so a partial slice still renders everything it was given.
#[must_use]
pub fn bus_nodes(buses: &[TemplateBus]) -> Vec<TemplateNode> {
    fn build(bus: &TemplateBus, buses: &[TemplateBus]) -> TemplateNode {
        let children: Vec<TemplateNode> = buses
            .iter()
            .filter(|b| b.parent.as_deref() == Some(bus.name.as_str()))
            .map(|b| build(b, buses))
            .collect();

        // A bus track carries no canonical group path — it is a destination,
        // not a classification of anything.
        let mut node = if children.is_empty() {
            TemplateNode::track(&bus.name)
        } else {
            TemplateNode::folder(&bus.name, Vec::new())
        };
        node.defaults.color_hex.clone_from(&bus.color_hex);
        node.children = children;
        node
    }

    let known = |name: &str| buses.iter().any(|b| b.name == name);
    buses
        .iter()
        .filter(|b| b.parent.as_deref().is_none_or(|p| !known(p)))
        .map(|b| build(b, buses))
        .collect()
}

/// The bus tree a session containing `paths` needs, as nested folder tracks.
///
/// [`buses_for_paths`] then [`bus_nodes`] — the pruning and the nesting in one
/// call, which is what a DAW layer organizing a real project wants.
pub fn bus_nodes_for_paths<'a>(paths: impl IntoIterator<Item = &'a [String]>) -> Vec<TemplateNode> {
    bus_nodes(&buses_for_paths(paths))
}

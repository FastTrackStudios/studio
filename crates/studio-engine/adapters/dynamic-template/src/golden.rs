//! Derive the golden [`IdealFullSessionTemplate`] from the classification
//! [`Config`](crate::DynamicTemplateConfig).
//!
//! The configuration *is* the single source of truth: walking the Group tree
//! yields the structural folders (instrument groups and sub-groups), and each
//! group's dimension sub-groups (Performer / Section / Arrangement / Layers /
//! Channel) plus its multi-mic descriptors yield the per-project deepening
//! axes — with their configured descriptor vocabularies attached. The engine
//! later expands and collapses that schema per project.

use crate::proto::{IdealFullSessionTemplate, NodeRouting, TemplateNode};
use monarchy::Group;

use crate::buses::{all_buses, bus_for_path, is_attachment_point};
use crate::colors::color_for_path;
use crate::ItemMetadata;

/// Sub-group names that are per-project **dimensions** rather than structural
/// folders, in canonical deepening order. Multi-mic is handled separately
/// (it usually arrives via `field_value_descriptors`) but is also recognised
/// here for the configs that model it as a nested group.
const DIMENSION_NAMES: &[&str] = &[
    "Performer",
    "Section",
    "Arrangement",
    "Layers",
    "Channel",
    "MultiMic",
];

fn is_dimension(name: &str) -> bool {
    DIMENSION_NAMES.iter().any(|d| d.eq_ignore_ascii_case(name))
}

/// The slot band whose canonical path matches `path` exactly (case-insensitive,
/// element for element). Unlike `band_for_category`'s loose element matching,
/// this won't mistake `Synths/Lead` for the `Vocals/Lead` band or `Keys/Electric`
/// for `Guitars/Electric`.
fn band_for_exact_path(path: &[String]) -> Option<&'static music_catalog::groups::GroupSlotBand> {
    music_catalog::groups::SLOT_BANDS.iter().find(|b| {
        b.path.len() == path.len()
            && b.path
                .iter()
                .zip(path)
                .all(|(a, c)| a.eq_ignore_ascii_case(c))
    })
}

/// Build the canonical FTS golden session by walking [`default_config`].
///
/// [`default_config`]: crate::default_config
#[must_use]
pub fn golden_template() -> IdealFullSessionTemplate {
    let cfg = crate::default_config();
    let mut root = Vec::new();
    for g in &cfg.groups {
        // Skip metadata-only groups (e.g. the global "MetadataFields" patterns)
        // and transparent groups, which don't produce a folder of their own.
        if g.metadata_only || g.transparent {
            continue;
        }
        let mut path = Vec::new();
        root.push(build_node(g, &mut path));
    }

    IdealFullSessionTemplate {
        name: "FTS Golden Session".to_string(),
        version: 2,
        root,
        // The golden template is the schema, not a session, so it carries the
        // whole bus tree. Per-project pruning is
        // [`buses_for_paths`](crate::buses::buses_for_paths).
        buses: all_buses(),
    }
}

/// Build a [`TemplateNode`] subtree for one config group. `path` tracks the
/// canonical group path (top-level first) as we recurse.
fn build_node(g: &Group<ItemMetadata>, path: &mut Vec<String>) -> TemplateNode {
    path.push(g.name.clone());

    let group_path: Vec<&str> = path.iter().map(String::as_str).collect();
    let mut node = TemplateNode::folder(&g.name, path.clone());
    node.defaults.color_hex = color_for_path(&group_path).map(color_palette::Color::to_hex_string);

    // Group-slot membership where this node's full canonical path maps to a
    // 128-slot band (e.g. ["Guitars","Electric"] → Electric Gtr; ["Drums"] →
    // Drums). Matched on the full path so sub-groups like Synths/Lead or
    // Keys/Electric don't inherit an unrelated band.
    if let Some(band) = band_for_exact_path(path) {
        node = node.in_group(band.label);
    }

    // Bus attachment. Only the exact attachment point carries the send — a
    // descendant like Guitars/Electric feeds Guitars, which feeds GTR BUS, so
    // giving it its own send would double the guitars into the mix.
    if is_attachment_point(path) {
        if let Some(bus) = bus_for_path(path) {
            node.routing = NodeRouting::to_bus(bus);
        }
    }

    // Structural sub-groups become child folders (recurse); dimension
    // sub-groups feed the deepening chain. Config order is preserved.
    let mut children: Vec<TemplateNode> = Vec::new();
    for child in g.groups.iter().filter(|c| !is_dimension(&c.name)) {
        children.push(build_node(child, path));
    }

    let dimensions: Vec<&Group<ItemMetadata>> =
        g.groups.iter().filter(|c| is_dimension(&c.name)).collect();
    let mic_vocab: Vec<String> = g
        .field_value_descriptors
        .get("MultiMic")
        .map(|ds| ds.iter().map(|d| d.value.clone()).collect())
        .unwrap_or_default();

    if let Some(chain) = dimension_chain(&dimensions, &mic_vocab) {
        children.push(chain);
    }

    // A tagged collection (e.g. drum SUM) declares a conditional grouping over
    // its pattern vocabulary — the engine groups matching captures under it and
    // leaves the rest alongside.
    if let Some(tc) = &g.tagged_collection {
        children.push(TemplateNode::collection(&tc.name, tc.patterns.clone()));
    }

    node = node.with_children(children);

    path.pop();
    node
}

/// Fold the dimension sub-groups (outer → inner) into a nested chain of
/// dimension nodes, bottoming out at the multi-mic captures from
/// `field_value_descriptors` (if any). Each dimension carries its configured
/// `patterns` as its vocabulary. Returns `None` when there are no dimensions
/// and no multi-mic captures.
fn dimension_chain(dims: &[&Group<ItemMetadata>], mic_vocab: &[String]) -> Option<TemplateNode> {
    // Innermost: the multi-mic capture dimension from field_value_descriptors.
    let mut inner: Option<TemplateNode> =
        (!mic_vocab.is_empty()).then(|| TemplateNode::dimension("MultiMic", mic_vocab.to_vec()));

    for d in dims.iter().rev() {
        // A nested-group "MultiMic" already supplies its own vocabulary via
        // patterns; other dimensions likewise use their configured patterns.
        let mut dim = TemplateNode::dimension(&d.name, d.patterns.clone());
        if let Some(child) = inner.take() {
            dim = dim.with_child(child);
        }
        inner = Some(dim);
    }
    inner
}

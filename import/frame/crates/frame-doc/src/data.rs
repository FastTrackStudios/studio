//! Typed access to the Frame data on figcraft nodes (`frame:*` keys of `Node::plugin_data`).
//!
//! [`FrameData::of`] parses one node's keys; [`FrameCache`] keeps the parse per node revision
//! (figcraft replaces a node's `Arc` whenever it changes, so the `Arc` itself is the revision).
//! Instances inherit their main's data and can override it ([`effective`], [`set_data`]); a
//! control's slots are marks on the slot nodes ([`slots_of`], [`control_of`]), so copying or
//! instancing a control keeps its slots without rewriting any ids.

use std::collections::BTreeMap;
use std::sync::Arc;

use figcraft_doc::{Document, Node, NodeId, NodeKind};
use frame::document::proto::{Arg, Bind, Emission, ShaderFx};
use frame::motion::proto::Motion;
use frame::surface::proto::{Binding, Control, Slot, SlotRole};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::keys;

/// A role a node plays for a control: on the slot node, under [`keys::SLOT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotMark {
    pub role: SlotRole,
    /// Its place in the control's slot list.
    pub at: u32,
    /// The control's `frame:id`, when it isn't the nearest control at or above the slot node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<Uuid>,
}

/// A slot naming a node that isn't in the document: on the control, under [`keys::SLOT_EXTERN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternSlot {
    pub role: SlotRole,
    pub at: u32,
    pub node: Uuid,
}

/// A key whose value didn't parse (it reads as absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataError {
    pub key: &'static str,
    pub message: String,
}

/// One node's Frame data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameData {
    /// The control, without its primary binding and slots (see [`control_of`]).
    pub control: Option<Control>,
    pub binding: Option<Binding>,
    pub slots: Vec<SlotMark>,
    pub slots_extern: Vec<ExternSlot>,
    pub motion: Vec<Motion>,
    pub shader: Option<ShaderFx>,
    pub emission: Option<Emission>,
    pub bind: Vec<Bind>,
    pub args: Vec<Arg>,
    pub of: Option<String>,
    /// Keys present but not valid JSON for their type.
    pub errors: Vec<DataError>,
}

impl FrameData {
    /// Parse the Frame keys of `node`.
    #[must_use]
    pub fn of(node: &Node) -> Self {
        Self::parse(&node.plugin_data)
    }

    /// Parse the Frame keys of a `plugin_data` map.
    #[must_use]
    pub fn parse(map: &BTreeMap<String, String>) -> Self {
        fn get<T: serde::de::DeserializeOwned>(
            map: &BTreeMap<String, String>,
            key: &'static str,
            errors: &mut Vec<DataError>,
        ) -> Option<T> {
            let s = map.get(key)?;
            serde_json::from_str(s)
                .map_err(|e| {
                    errors.push(DataError {
                        key,
                        message: e.to_string(),
                    })
                })
                .ok()
        }
        let mut errors = Vec::new();
        let e = &mut errors;
        let mut d = Self {
            control: get(map, keys::CONTROL, e),
            binding: get(map, keys::BINDING, e),
            slots: get(map, keys::SLOT, e).unwrap_or_default(),
            slots_extern: get(map, keys::SLOT_EXTERN, e).unwrap_or_default(),
            motion: get(map, keys::MOTION, e).unwrap_or_default(),
            shader: get(map, keys::SHADER, e),
            emission: get(map, keys::EMISSION, e),
            bind: get(map, keys::BIND, e).unwrap_or_default(),
            args: get(map, keys::ARGS, e).unwrap_or_default(),
            of: get(map, keys::OF, e),
            errors: Vec::new(),
        };
        d.errors = errors;
        d
    }

    /// Whether there is no Frame data at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.control.is_none()
            && self.binding.is_none()
            && self.slots.is_empty()
            && self.slots_extern.is_empty()
            && self.motion.is_empty()
            && self.shader.is_none()
            && self.emission.is_none()
            && self.bind.is_empty()
            && self.args.is_empty()
            && self.of.is_none()
    }

    /// Write the data into a `plugin_data` map: present values under their keys, absent ones
    /// removed. Other keys stay.
    ///
    /// # Errors
    /// A value that doesn't serialize.
    pub fn write(&self, map: &mut BTreeMap<String, String>) -> serde_json::Result<()> {
        fn put<T: Serialize>(
            map: &mut BTreeMap<String, String>,
            key: &str,
            v: Option<&T>,
        ) -> serde_json::Result<()> {
            match v {
                Some(v) => {
                    map.insert(key.to_string(), serde_json::to_string(v)?);
                }
                None => {
                    map.remove(key);
                }
            }
            Ok(())
        }
        let control = self.control.as_ref().map(|c| Control {
            binding: None,
            slots: Vec::new(),
            ..c.clone()
        });
        put(map, keys::CONTROL, control.as_ref())?;
        put(map, keys::BINDING, self.binding.as_ref())?;
        put(map, keys::SLOT, Some(&self.slots).filter(|v| !v.is_empty()))?;
        put(
            map,
            keys::SLOT_EXTERN,
            Some(&self.slots_extern).filter(|v| !v.is_empty()),
        )?;
        put(
            map,
            keys::MOTION,
            Some(&self.motion).filter(|v| !v.is_empty()),
        )?;
        put(map, keys::SHADER, self.shader.as_ref())?;
        put(map, keys::EMISSION, self.emission.as_ref())?;
        put(map, keys::BIND, Some(&self.bind).filter(|v| !v.is_empty()))?;
        put(map, keys::ARGS, Some(&self.args).filter(|v| !v.is_empty()))?;
        put(map, keys::OF, self.of.as_ref())
    }
}

/// Parsed Frame data per node revision. A node's entry is reused while the document still holds
/// the same `Arc<Node>`; any edit (figcraft copies the node on write) parses it again.
#[derive(Debug, Default)]
pub struct FrameCache {
    entries: BTreeMap<NodeId, (Arc<Node>, Arc<FrameData>)>,
    parses: u64,
}

impl FrameCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The Frame data of node `id` (`None`: no such node).
    pub fn get(&mut self, doc: &Document, id: NodeId) -> Option<Arc<FrameData>> {
        let node = doc.node_arc(id)?;
        if let Some((rev, data)) = self.entries.get(&id)
            && Arc::ptr_eq(rev, node)
        {
            return Some(data.clone());
        }
        let data = Arc::new(FrameData::of(node));
        self.parses += 1;
        self.entries.insert(id, (node.clone(), data.clone()));
        Some(data)
    }

    /// Drop the entries of nodes the document no longer has.
    pub fn retain_live(&mut self, doc: &Document) {
        self.entries.retain(|id, _| doc.contains(*id));
    }

    /// How many parses the cache has done (misses).
    #[must_use]
    pub fn parses(&self) -> u64 {
        self.parses
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The overrides an instance root holds for its layers: main layer id → Frame keys.
type Overrides = BTreeMap<String, BTreeMap<String, String>>;

/// The Frame keys in effect on node `id`: its own, else those of the main layer it mirrors (an
/// instance or a layer of one follows `source` up to the main), with the overrides held by the
/// instances it sits in on top (the outermost instance wins).
#[must_use]
pub fn effective_map(doc: &Document, id: NodeId) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut cur = doc.node(id);
    for _ in 0..64 {
        let Some(n) = cur else { break };
        for k in keys::DATA {
            if !out.contains_key(k)
                && let Some(v) = n.plugin_data.get(k)
            {
                out.insert(k.to_string(), v.clone());
            }
        }
        cur = n.source.and_then(|s| doc.node(s));
    }
    let Some(source) = doc.node(id).and_then(|n| n.source) else {
        return out;
    };
    for a in doc.ancestors(id) {
        let Some(over) = doc
            .node(a)
            .filter(|n| matches!(n.kind, NodeKind::Instance { .. }))
            .and_then(|n| n.plugin_data.get(keys::OVERRIDE))
        else {
            continue;
        };
        let Ok(over) = serde_json::from_str::<Overrides>(over) else {
            continue;
        };
        if let Some(m) = over.get(&source.0.to_string()) {
            out.extend(
                m.iter()
                    .filter(|(k, _)| keys::DATA.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
    }
    out
}

/// [`effective_map`], parsed.
#[must_use]
pub fn effective(doc: &Document, id: NodeId) -> FrameData {
    FrameData::parse(&effective_map(doc, id))
}

/// Set (or with `None` remove) Frame key `key` on node `id`. On a layer inside an instance the
/// value is kept as an override on the instance root, since the layer itself is rebuilt from the
/// main whenever the instance syncs.
///
/// # Errors
/// No such node, or an override map on the instance that isn't valid JSON.
pub fn set_data(
    doc: &mut Document,
    id: NodeId,
    key: &str,
    value: Option<String>,
) -> Result<(), crate::Error> {
    let node = doc.node(id).ok_or(crate::Error::NoNode(id))?;
    let root = match (node.source, &node.kind) {
        (Some(_), NodeKind::Instance { .. }) | (None, _) => None,
        (Some(s), _) => doc
            .ancestors(id)
            .into_iter()
            .find(|a| {
                doc.node(*a)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Instance { .. }))
            })
            .map(|r| (r, s)),
    };
    let Some((root, source)) = root else {
        let n = doc.node_mut(id).ok_or(crate::Error::NoNode(id))?;
        match value {
            Some(v) => n.plugin_data.insert(key.to_string(), v),
            None => n.plugin_data.remove(key),
        };
        return Ok(());
    };
    let r = doc.node_mut(root).ok_or(crate::Error::NoNode(root))?;
    let mut over: Overrides = match r.plugin_data.get(keys::OVERRIDE) {
        Some(s) => serde_json::from_str(s).map_err(|e| crate::Error::Data(e.to_string()))?,
        None => Overrides::new(),
    };
    let entry = over.entry(source.0.to_string()).or_default();
    match value {
        Some(v) => {
            entry.insert(key.to_string(), v);
        }
        None => {
            entry.remove(key);
        }
    }
    over.retain(|_, m| !m.is_empty());
    if over.is_empty() {
        r.plugin_data.remove(keys::OVERRIDE);
    } else {
        r.plugin_data.insert(
            keys::OVERRIDE.to_string(),
            serde_json::to_string(&over).map_err(|e| crate::Error::Data(e.to_string()))?,
        );
    }
    Ok(())
}

/// What a slot names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotTarget {
    Node(NodeId),
    /// A node the document doesn't have.
    Extern(Uuid),
}

/// The `frame:id` of node `n`, when it has a valid one.
#[must_use]
pub fn frame_id(n: &Node) -> Option<Uuid> {
    n.plugin_data
        .get(keys::ID)
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Every control's slots, in slot order: the marks on slot nodes (their control is the nearest
/// one at or above them unless the mark names another) and the control's extern slots.
#[must_use]
pub fn slot_index(doc: &Document) -> BTreeMap<NodeId, Vec<(SlotRole, SlotTarget)>> {
    let mut by_uuid: BTreeMap<Uuid, NodeId> = BTreeMap::new();
    let order: Vec<NodeId> = doc.pages.iter().flat_map(|p| doc.subtree(*p)).collect();
    for id in &order {
        if let Some(u) = doc.node(*id).and_then(frame_id) {
            by_uuid.entry(u).or_insert(*id);
        }
    }
    let has_control = |id: NodeId| {
        doc.node(id)
            .is_some_and(|n| n.plugin_data.contains_key(keys::CONTROL))
    };
    let mut lists: BTreeMap<NodeId, Vec<(u32, usize, SlotRole, SlotTarget)>> = BTreeMap::new();
    for (i, id) in order.iter().enumerate() {
        let Some(n) = doc.node(*id) else { continue };
        if let Some(s) = n.plugin_data.get(keys::SLOT_EXTERN)
            && let Ok(ext) = serde_json::from_str::<Vec<ExternSlot>>(s)
        {
            let l = lists.entry(*id).or_default();
            l.extend(
                ext.into_iter()
                    .map(|e| (e.at, i, e.role, SlotTarget::Extern(e.node))),
            );
        }
        let Some(s) = n.plugin_data.get(keys::SLOT) else {
            continue;
        };
        let Ok(marks) = serde_json::from_str::<Vec<SlotMark>>(s) else {
            continue;
        };
        let up = || std::iter::once(*id).chain(doc.ancestors(*id));
        let nearest = up().find(|a| has_control(*a));
        for m in marks {
            let owner = match m.of {
                // The named control: one this node sits in first (a pasted copy's), else any.
                Some(u) => up()
                    .find(|a| doc.node(*a).and_then(frame_id) == Some(u))
                    .or_else(|| by_uuid.get(&u).copied()),
                None => nearest,
            };
            if let Some(o) = owner {
                lists
                    .entry(o)
                    .or_default()
                    .push((m.at, i, m.role, SlotTarget::Node(*id)));
            }
        }
    }
    lists
        .into_iter()
        .map(|(o, mut l)| {
            l.sort_by_key(|(at, i, _, _)| (*at, *i));
            (o, l.into_iter().map(|(_, _, r, t)| (r, t)).collect())
        })
        .collect()
}

/// The slots of control `owner`, in order.
#[must_use]
pub fn slots_of(doc: &Document, owner: NodeId) -> Vec<(SlotRole, SlotTarget)> {
    slot_index(doc).remove(&owner).unwrap_or_default()
}

/// The whole control of node `owner` (its control with the primary binding and slots put back),
/// slot nodes named by their `frame:id` (a node without one by a UUID derived from its figcraft
/// id, as [`doc_to_scene`](crate::doc_to_scene) does). Builds the slot index; for many controls
/// use [`controls`].
#[must_use]
pub fn control_of(doc: &Document, owner: NodeId) -> Option<Control> {
    assemble(doc, owner, slots_of(doc, owner))
}

/// Every control in the document, whole (see [`control_of`]), with one slot index.
#[must_use]
pub fn controls(doc: &Document) -> BTreeMap<NodeId, Control> {
    let mut index = slot_index(doc);
    doc.pages
        .iter()
        .flat_map(|p| doc.subtree(*p))
        .filter_map(|id| {
            Some((
                id,
                assemble(doc, id, index.remove(&id).unwrap_or_default())?,
            ))
        })
        .collect()
}

fn assemble(doc: &Document, owner: NodeId, slots: Vec<(SlotRole, SlotTarget)>) -> Option<Control> {
    let data = effective(doc, owner);
    let mut c = data.control?;
    c.binding = data.binding;
    c.slots = slots
        .into_iter()
        .map(|(role, t)| Slot {
            role,
            node: match t {
                SlotTarget::Node(id) => doc
                    .node(id)
                    .and_then(frame_id)
                    .unwrap_or_else(|| crate::convert::derived_uuid(id)),
                SlotTarget::Extern(u) => u,
            },
        })
        .collect();
    Some(c)
}

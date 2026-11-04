//! v1 `Scene` ⇄ figcraft `Document`.
//!
//! [`scene_to_doc`] maps each v1 node onto the nearest figcraft node, its Frame data onto the
//! `frame:*` keys ([`FrameData`]) and its v1 id onto [`keys::ID`]. Where figcraft can't hold a
//! group of v1 fields exactly (an f64 opacity, a text's runs, a vector's SVG outlines), the v1
//! values are kept beside it under [`keys::V1`] with a hash of what the figcraft side held at the
//! time: [`doc_to_scene`] puts them back while that side is unedited and reads the figcraft value
//! once it has been edited. What isn't a node (the scene root, variables, asset ids, unreachable
//! nodes) is a record on a page under [`keys::ROOT`]. So Scene → Document → Scene gives back the
//! same Scene, and a document edited in figcraft gives the Scene of the edit.

mod project;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use figcraft_color::Color;
use figcraft_doc::{self as fc, Document, ImageData};
use frame::document::proto as v1;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::data::{ExternSlot, FrameData, SlotMark, SlotTarget, effective, frame_id, slot_index};
use crate::{Error, keys};
use project::{Cx, GROUPS, get, native, put};

/// Image keys are `b3-` and the asset id (a blake3 hex).
pub(crate) const KEY_PREFIX: &str = "b3-";

/// The namespace of the UUIDs given to nodes that have no `frame:id` of their own.
const NAMESPACE: Uuid = Uuid::from_u128(0x5d2c_47a1_9b0e_4f63_8a17_c3e9_02b4_6f58);

/// The v1 id a figcraft node without a (usable) `frame:id` gets: stable for the node.
#[must_use]
pub fn derived_uuid(id: fc::NodeId) -> Uuid {
    Uuid::new_v5(&NAMESPACE, &id.0.to_be_bytes())
}

/// The figcraft image key of asset `id`.
pub(crate) fn image_key(id: &v1::AssetId) -> String {
    let key = format!("{KEY_PREFIX}{}", id.0);
    if ImageData::is_valid_key(&key) {
        key
    } else {
        format!("{KEY_PREFIX}{}", blake3::hash(id.0.as_bytes()).to_hex())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    /// The root is a document whose children are the pages.
    #[default]
    Pages,
    /// Any other root: it is the single child of a page made to carry it.
    Carrier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AssetRecord {
    key: String,
    mime: String,
}

/// What of a scene isn't a figcraft node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RootRecord {
    mode: Mode,
    /// Pages mode: the root node, without its children (the pages are).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root: Option<v1::Node>,
    #[serde(default, skip_serializing_if = "v1::Variables::is_empty")]
    variables: v1::Variables,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    assets: BTreeMap<v1::AssetId, AssetRecord>,
    /// Nodes the root doesn't reach.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    orphans: Vec<v1::Node>,
}

/// One group's v1 values and the hash of the figcraft side they go with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Shadow {
    v: Map<String, Value>,
    n: String,
}

type Shadows = BTreeMap<String, Shadow>;

fn hash16(v: &Value) -> String {
    blake3::hash(v.to_string().as_bytes()).to_hex()[..16].to_string()
}

fn object(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

fn parent_native(p: Option<fc::NodeId>) -> Value {
    json!(p.map(|p| p.0))
}

fn children_native(c: &[fc::NodeId]) -> Value {
    json!(c.iter().map(|c| c.0).collect::<Vec<_>>())
}

/// The v1 tree as the root reaches it: each node once, children that are missing or reached
/// before left out.
struct Tree {
    order: Vec<v1::NodeId>,
    parent: BTreeMap<v1::NodeId, Option<v1::NodeId>>,
    children: BTreeMap<v1::NodeId, Vec<v1::NodeId>>,
}

impl Tree {
    fn of(scene: &v1::Scene) -> Self {
        let mut t = Self {
            order: Vec::new(),
            parent: BTreeMap::new(),
            children: BTreeMap::new(),
        };
        let mut seen = BTreeSet::from([scene.root]);
        let mut stack = vec![(scene.root, None)];
        while let Some((id, parent)) = stack.pop() {
            t.order.push(id);
            t.parent.insert(id, parent);
            let kids: Vec<v1::NodeId> = scene.nodes[&id]
                .children
                .iter()
                .copied()
                .filter(|c| scene.nodes.contains_key(c) && seen.insert(*c))
                .collect();
            stack.extend(kids.iter().rev().map(|c| (*c, Some(id))));
            t.children.insert(id, kids);
        }
        t
    }
}

/// A v1 scene as a figcraft document. Every v1 node the root reaches is a figcraft node with its
/// Frame data on its `frame:*` keys; see the module docs for what keeps the rest.
///
/// # Errors
/// The scene has no root node, or a value doesn't serialize.
pub fn scene_to_doc(scene: &v1::Scene) -> Result<Document, Error> {
    let root = scene.nodes.get(&scene.root).ok_or(Error::NoRoot)?;
    let tree = Tree::of(scene);
    let pages_mode = root.kind == v1::NodeKind::Document
        && !root.children.is_empty()
        && tree.children[&scene.root] == root.children
        && root
            .children
            .iter()
            .all(|c| scene.nodes[c].kind == v1::NodeKind::Page);

    let mut doc = Document::without_pages(root.name.clone());
    let mut cx = Cx::default();
    let mut record = RootRecord {
        mode: if pages_mode {
            Mode::Pages
        } else {
            Mode::Carrier
        },
        variables: scene.variables.clone(),
        ..RootRecord::default()
    };
    for (id, a) in &scene.assets {
        let key = image_key(id);
        doc.images.insert(
            key.clone(),
            ImageData {
                bytes: Arc::new(a.bytes.clone()),
                width: 0,
                height: 0,
            },
        );
        record.assets.insert(
            id.clone(),
            AssetRecord {
                key: key.clone(),
                mime: a.mime.clone(),
            },
        );
        cx.key_of.insert(id.clone(), key);
    }

    // Ids first (instances name their component's), pages before the rest.
    let carrier = (!pages_mode).then(|| doc.alloc_id());
    let converted: Vec<v1::NodeId> = tree
        .order
        .iter()
        .copied()
        .filter(|id| !pages_mode || *id != scene.root)
        .collect();
    let mut fc_of: BTreeMap<v1::NodeId, fc::NodeId> = BTreeMap::new();
    for id in &converted {
        let f = doc.alloc_id();
        fc_of.insert(*id, f);
        cx.uuid_of.insert(f, id.0);
        if scene.nodes[id].kind == v1::NodeKind::Component {
            cx.components.insert(id.0, f);
        }
    }
    let is_page = |id: &v1::NodeId| pages_mode && tree.parent[id] == Some(scene.root);
    let fc_parent = |id: &v1::NodeId| match tree.parent[id] {
        Some(p) if fc_of.contains_key(&p) => Some(fc_of[&p]),
        _ => carrier,
    };

    // Slot marks: each slot on the node it names, saying whose it is when that isn't the
    // nearest control above it.
    let has_control = |id: &v1::NodeId| fc_of.contains_key(id) && scene.nodes[id].control.is_some();
    let default_owner = |id: v1::NodeId| {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if has_control(&c) {
                return Some(c);
            }
            cur = tree.parent.get(&c).copied().flatten();
        }
        None
    };
    let mut marks: BTreeMap<v1::NodeId, Vec<SlotMark>> = BTreeMap::new();
    let mut externs: BTreeMap<v1::NodeId, Vec<ExternSlot>> = BTreeMap::new();
    for owner in &converted {
        let Some(c) = &scene.nodes[owner].control else {
            continue;
        };
        for (at, s) in c.slots.iter().enumerate() {
            let at = u32::try_from(at).map_err(|e| Error::Data(e.to_string()))?;
            let target = v1::NodeId(s.node);
            if fc_of.contains_key(&target) {
                let of = (default_owner(target) != Some(*owner)).then_some(owner.0);
                marks.entry(target).or_default().push(SlotMark {
                    role: s.role,
                    at,
                    of,
                });
            } else {
                externs.entry(*owner).or_default().push(ExternSlot {
                    role: s.role,
                    at,
                    node: s.node,
                });
            }
        }
    }

    let mut nodes: Vec<(fc::Node, Option<fc::NodeId>)> = Vec::with_capacity(converted.len());
    for id in &converted {
        let v = &scene.nodes[id];
        let page = is_page(id);
        let mut f = fc::Node::new(fc_of[id], fc::NodeKind::Frame, "", 0.0, 0.0);
        for (g, _, _) in GROUPS {
            put(g, v, &mut f, &cx, page);
        }
        let parent = if page { None } else { fc_parent(id) };

        // What figcraft doesn't give back exactly.
        let vm = object(serde_json::to_value(v)?);
        let mut back = v1::Node::new(v1::NodeKind::Frame, "");
        for (g, _, _) in GROUPS {
            get(g, &f, &mut back, &cx);
        }
        let bm = object(serde_json::to_value(&back)?);
        let mut shadows = Shadows::new();
        for (g, name, fields) in GROUPS {
            if fields.iter().any(|k| vm.get(*k) != bm.get(*k)) {
                let vals = fields
                    .iter()
                    .filter_map(|k| Some((k.to_string(), vm.get(*k)?.clone())))
                    .collect();
                shadows.insert(
                    name.to_string(),
                    Shadow {
                        v: vals,
                        n: hash16(&native(g, &f)),
                    },
                );
            }
        }
        let tree_parent = if page {
            Some(scene.root)
        } else {
            tree.parent[id]
        };
        if v.parent != tree_parent {
            shadows.insert(
                "parent".into(),
                Shadow {
                    v: Map::from_iter([("parent".to_string(), vm["parent"].clone())]),
                    n: hash16(&parent_native(parent)),
                },
            );
        }
        let kids = &tree.children[id];
        if &v.children != kids {
            let native_kids: Vec<fc::NodeId> = kids.iter().map(|k| fc_of[k]).collect();
            shadows.insert(
                "children".into(),
                Shadow {
                    v: Map::from_iter([("children".to_string(), vm["children"].clone())]),
                    n: hash16(&children_native(&native_kids)),
                },
            );
        }

        f.plugin_data.insert(keys::ID.to_string(), id.0.to_string());
        if !shadows.is_empty() {
            f.plugin_data
                .insert(keys::V1.to_string(), serde_json::to_string(&shadows)?);
        }
        let data = FrameData {
            control: v.control.clone(),
            binding: v.control.as_ref().and_then(|c| c.binding.clone()),
            slots: marks.remove(id).unwrap_or_default(),
            slots_extern: externs.remove(id).unwrap_or_default(),
            motion: v.motion.clone(),
            shader: v.shader.clone(),
            emission: v.emission.clone(),
            bind: v.bind.clone(),
            args: v.args.clone(),
            of: v.of.clone(),
            errors: Vec::new(),
        };
        data.write(&mut f.plugin_data)?;
        nodes.push((f, parent));
    }

    if pages_mode {
        record.root = Some(v1::Node {
            children: Vec::new(),
            ..root.clone()
        });
    }
    record.orphans = scene
        .nodes
        .iter()
        .filter(|(id, _)| !tree.parent.contains_key(id))
        .map(|(_, n)| n.clone())
        .collect();
    let record = serde_json::to_string(&record)?;
    if let Some(c) = carrier {
        let mut page = fc::Node::new(c, page_kind(), root.name.clone(), 0.0, 0.0);
        page.plugin_data
            .insert(keys::ROOT.to_string(), record.clone());
        doc.insert_page(page).map_err(doc_error)?;
    }
    let mut record = carrier.is_none().then_some(record);
    for (mut f, parent) in nodes {
        match parent {
            None => {
                if let Some(r) = record.take() {
                    f.plugin_data.insert(keys::ROOT.to_string(), r);
                }
                doc.insert_page(f).map_err(doc_error)?;
            }
            Some(p) => {
                doc.insert(f, p, None).map_err(doc_error)?;
            }
        }
    }
    Ok(doc)
}

fn page_kind() -> fc::NodeKind {
    fc::NodeKind::Page {
        background: Color::DEFAULT_PAGE,
        flows: Vec::new(),
        prototype: Default::default(),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn doc_error(e: fc::DocError) -> Error {
    Error::Data(e.to_string())
}

/// A figcraft document as a v1 scene: [`scene_to_doc`] reversed, with whatever was edited in
/// figcraft read from figcraft.
///
/// # Errors
/// A `frame:root` record or `frame:v1` value that isn't valid, or a node that doesn't make a v1
/// node.
pub fn doc_to_scene(doc: &Document) -> Result<v1::Scene, Error> {
    let record: RootRecord = match doc
        .pages
        .iter()
        .find_map(|p| doc.node(*p)?.plugin_data.get(keys::ROOT))
    {
        Some(s) => serde_json::from_str(s)?,
        None => RootRecord::default(),
    };
    let lone = (doc.pages.len() == 1)
        .then(|| doc.node(doc.pages[0]))
        .flatten()
        .filter(|p| p.children.len() == 1)
        .map(|p| p.children[0]);
    let carried = lone.filter(|_| record.mode == Mode::Carrier);

    let tops: Vec<fc::NodeId> = match carried {
        Some(c) => vec![c],
        None => doc.pages.clone(),
    };
    let converted: Vec<fc::NodeId> = tops.iter().flat_map(|t| doc.subtree(*t)).collect();

    // Ids: a node keeps its `frame:id` unless one before it (by figcraft id: the original, not
    // the pasted copy) has it.
    let root_node = match (&record.root, carried) {
        (_, Some(_)) => None,
        (Some(r), None) => Some(r.clone()),
        (None, None) => Some(v1::Node {
            id: v1::NodeId(derived_uuid(fc::NodeId(0))),
            ..v1::Node::new(v1::NodeKind::Document, doc.name.clone())
        }),
    };
    let mut used: BTreeSet<Uuid> = record
        .orphans
        .iter()
        .map(|n| n.id.0)
        .chain(root_node.as_ref().map(|r| r.id.0))
        .collect();
    let mut sorted = converted.clone();
    sorted.sort_unstable();
    let mut cx = Cx::default();
    for id in sorted {
        let own = doc
            .node(id)
            .and_then(frame_id)
            .filter(|u| !used.contains(u));
        let u = own.unwrap_or_else(|| derived_uuid(id));
        used.insert(u);
        cx.uuid_of.insert(id, u);
    }
    let uuid = |id: fc::NodeId| v1::NodeId(cx.uuid_of[&id]);

    let mut assets = BTreeMap::new();
    let by_key: BTreeMap<&str, (&v1::AssetId, &AssetRecord)> = record
        .assets
        .iter()
        .map(|(id, r)| (r.key.as_str(), (id, r)))
        .collect();
    for (key, img) in &doc.images {
        let (id, mime) = match by_key.get(key.as_str()) {
            Some((id, r)) => ((*id).clone(), r.mime.clone()),
            None => (v1::AssetId::of(&img.bytes), sniff(&img.bytes).to_string()),
        };
        cx.asset_of.insert(key.clone(), id.clone());
        assets.insert(
            id.clone(),
            v1::Asset {
                id,
                mime,
                bytes: img.bytes.as_ref().clone(),
            },
        );
    }

    let slots = slot_index(doc);
    let root_id = carried
        .map(uuid)
        .or(root_node.as_ref().map(|r| r.id))
        .ok_or(Error::NoRoot)?;
    let mut nodes = BTreeMap::new();
    for id in &converted {
        let f = doc.node(*id).ok_or(Error::NoNode(*id))?;
        let tree_parent = if tops.contains(id) {
            root_node.as_ref().map(|r| r.id)
        } else {
            f.parent.map(uuid)
        };
        let mut back = v1::Node::new(v1::NodeKind::Frame, "");
        for (g, _, _) in GROUPS {
            get(g, f, &mut back, &cx);
        }
        back.id = uuid(*id);
        back.parent = tree_parent;
        back.children = f.children.iter().map(|c| uuid(*c)).collect();
        let mut m = object(serde_json::to_value(&back)?);
        if let Some(s) = f.plugin_data.get(keys::V1) {
            let shadows: Shadows = serde_json::from_str(s)?;
            let mut apply = |name: &str, now: &Value| {
                if let Some(e) = shadows.get(name)
                    && e.n == hash16(now)
                {
                    m.extend(e.v.clone());
                }
            };
            for (g, name, _) in GROUPS {
                apply(name, &native(g, f));
            }
            apply("parent", &parent_native(f.parent));
            apply("children", &children_native(&f.children));
        }
        let mut node: v1::Node = serde_json::from_value(Value::Object(m))?;

        let d = effective(doc, *id);
        node.control = d.control.map(|mut c| {
            c.binding = d.binding;
            c.slots = slots
                .get(id)
                .into_iter()
                .flatten()
                .map(|(role, t)| frame::surface::proto::Slot {
                    role: *role,
                    node: match t {
                        SlotTarget::Node(n) => cx
                            .uuid_of
                            .get(n)
                            .copied()
                            .or_else(|| doc.node(*n).and_then(frame_id))
                            .unwrap_or_else(|| derived_uuid(*n)),
                        SlotTarget::Extern(u) => *u,
                    },
                })
                .collect();
            c
        });
        node.motion = d.motion;
        node.shader = d.shader;
        node.emission = d.emission;
        node.bind = d.bind;
        node.args = d.args;
        node.of = d.of;
        nodes.insert(node.id, node);
    }
    if let Some(mut r) = root_node {
        r.children = tops.iter().map(|t| uuid(*t)).collect();
        nodes.insert(r.id, r);
    }
    for o in record.orphans {
        nodes.entry(o.id).or_insert(o);
    }
    Ok(v1::Scene {
        root: root_id,
        nodes,
        assets,
        variables: record.variables,
    })
}

/// The mime type of an image added in figcraft, from its first bytes.
fn sniff(b: &[u8]) -> &'static str {
    if b.starts_with(b"\x89PNG") {
        "image/png"
    } else if b.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if b.starts_with(b"GIF8") {
        "image/gif"
    } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else if b.starts_with(b"<svg") || b.starts_with(b"<?xml") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

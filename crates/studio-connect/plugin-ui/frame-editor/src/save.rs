//! Persist one edited role into an explicitly chosen theme layer.
//!
//! These operations perform conversion, compilation and file I/O. Call them
//! on a worker, and serialize saves with the editor's preview requests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use frame::document::proto::{NodeId, NodeKind, Scene};
use frame::theme::host::{ThemeError, ThemeHost};
use frame_doc::doc::Document;

/// The source layers and displayed scene when editing began.
pub struct SaveSnapshot {
    scene: Arc<Scene>,
    files: BTreeMap<PathBuf, String>,
    order: Vec<PathBuf>,
    version: u64,
}

/// A save was refused or failed. A write error after staging retains the
/// staged draft, so the caller may report it and let the user resolve it.
#[derive(Debug)]
pub enum SaveError {
    NoTheme,
    HostChanged,
    Busy,
    NoSnapshot,
    WorkerStopped,
    UnknownFile,
    MissingRole,
    LaterOverride,
    OtherChanges,
    EmbeddedComponents,
    IdCollision,
    Document(frame_doc::Error),
    Parse(String),
    Stage(Vec<ThemeError>),
    Write {
        errors: Vec<ThemeError>,
        staged_version: u64,
    },
}

impl SaveSnapshot {
    /// Capture the displayed merged theme and the exact file texts that
    /// produced it. Run on a worker; file-layer inspection can block.
    ///
    /// # Errors
    /// No theme is displayed, or it changed while the snapshot was read.
    pub fn capture(host: &ThemeHost) -> Result<Self, SaveError> {
        let (theme, version) = host.snapshot().ok_or(SaveError::NoTheme)?;
        let files = host.compiled_files();
        let order = host
            .inspect()
            .files
            .into_iter()
            .map(|f| PathBuf::from(f.path))
            .collect();
        if host.version() != version {
            return Err(SaveError::HostChanged);
        }
        // Compare like representations: document conversion can normalize scene
        // data even before a user makes an edit.
        let document = frame_doc::scene_to_doc(&theme.source()).map_err(SaveError::Document)?;
        let scene = frame_doc::doc_to_scene(&document).map_err(SaveError::Document)?;
        Ok(Self {
            scene: Arc::new(scene),
            files,
            order,
            version,
        })
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    /// Source files in layer order.
    #[must_use]
    pub fn files(&self) -> &[PathBuf] {
        &self.order
    }

    pub(crate) fn refresh(host: &ThemeHost, document: &Document) -> Result<Self, SaveError> {
        let mut snapshot = Self::capture(host)?;
        snapshot.scene = Arc::new(frame_doc::doc_to_scene(document).map_err(SaveError::Document)?);
        Ok(snapshot)
    }

    /// Stage and save one changed role to its existing target file. Changes to
    /// other roles, components, variables or assets are refused. The target
    /// must be the last layer defining the role. Other drafts remain drafts.
    ///
    /// # Errors
    /// The role or file cannot be safely mapped, the host or disk changed,
    /// compilation failed, or the staged file could not be written.
    pub fn save_role(
        &self,
        host: &ThemeHost,
        document: &Document,
        role: &str,
        target: &Path,
        version: u64,
    ) -> Result<(), SaveError> {
        let current = frame_doc::doc_to_scene(document).map_err(SaveError::Document)?;
        if without_role(&self.scene, role)? != without_role(&current, role)? {
            return Err(SaveError::OtherChanges);
        }
        let expected = self.files.get(target).ok_or(SaveError::UnknownFile)?;
        ensure_last_layer(&self.files, &self.order, target, role)?;
        let mut layer = parse(expected)?;
        replace_role(&mut layer, &current, role)?;
        let markup = frame::document::text::print(&layer);
        host.edit_if_unchanged(target, expected, markup, version)
            .map_err(SaveError::Stage)?;
        let staged_version = version.wrapping_add(1);
        host.save_file_if_version(target, staged_version)
            .map_err(|errors| SaveError::Write {
                errors,
                staged_version,
            })
    }
}

fn parse(text: &str) -> Result<Scene, SaveError> {
    frame::document::text::parse(text, BTreeMap::new()).map_err(|e| SaveError::Parse(e.to_string()))
}

fn page(scene: &Scene, role: &str) -> Result<NodeId, SaveError> {
    scene
        .pages()
        .iter()
        .copied()
        .find(|id| scene.node(*id).is_some_and(|n| n.name == role))
        .ok_or(SaveError::MissingRole)
}

fn subtree(scene: &Scene, root: NodeId) -> BTreeSet<NodeId> {
    let mut all = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if all.insert(id)
            && let Some(node) = scene.node(id)
        {
            stack.extend(node.children.iter().copied());
        }
    }
    all
}

fn without_role(scene: &Scene, role: &str) -> Result<Scene, SaveError> {
    let id = page(scene, role)?;
    let mut rest = scene.clone();
    for id in subtree(scene, id) {
        rest.nodes.remove(&id);
    }
    if let Some(root) = rest.nodes.get_mut(&rest.root) {
        root.children.retain(|child| *child != id);
    }
    Ok(rest)
}

fn replace_role(layer: &mut Scene, edited: &Scene, role: &str) -> Result<(), SaveError> {
    let old = page(layer, role)?;
    let new = page(edited, role)?;
    let old_ids = subtree(layer, old);
    if old_ids.iter().any(|id| {
        layer
            .node(*id)
            .is_some_and(|n| n.kind == NodeKind::Component)
    }) {
        return Err(SaveError::EmbeddedComponents);
    }
    let new_ids = subtree(edited, new);
    if new_ids
        .iter()
        .any(|id| layer.nodes.contains_key(id) && !old_ids.contains(id))
    {
        return Err(SaveError::IdCollision);
    }
    for id in old_ids {
        layer.nodes.remove(&id);
    }
    let root = layer
        .nodes
        .get_mut(&layer.root)
        .ok_or(SaveError::MissingRole)?;
    let slot = root
        .children
        .iter_mut()
        .find(|id| **id == old)
        .ok_or(SaveError::MissingRole)?;
    *slot = new;
    for id in new_ids {
        let mut node = edited.nodes[&id].clone();
        if id == new {
            node.parent = Some(layer.root);
        }
        layer.nodes.insert(id, node);
    }
    Ok(())
}

fn ensure_last_layer(
    files: &BTreeMap<PathBuf, String>,
    order: &[PathBuf],
    target: &Path,
    role: &str,
) -> Result<(), SaveError> {
    let at = order
        .iter()
        .position(|p| p == target)
        .ok_or(SaveError::UnknownFile)?;
    for path in &order[at + 1..] {
        let text = files.get(path).ok_or(SaveError::UnknownFile)?;
        if page(&parse(text)?, role).is_ok() {
            return Err(SaveError::LaterOverride);
        }
    }
    Ok(())
}

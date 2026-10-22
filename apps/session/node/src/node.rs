//! The embedded node: a thin owner of Task's `LocalNode`.

use std::path::{Path, PathBuf};

use eyre::WrapErr;
use task_server::local_node::LocalNode;

pub use task_server::local_node::{NodeOptions, PERSONAL_SLUG};

/// A running Task device node inside a studio app.
///
/// Dropping it stops every endpoint it bound.
#[derive(Debug)]
pub struct Node {
    root: PathBuf,
    inner: LocalNode,
}

impl Node {
    /// Provision (first run) and boot a node over `data_root`, with its
    /// per-org iroh endpoints bound and the device pull running.
    ///
    /// # Errors
    ///
    /// The data root cannot be created or provisioned, or the backend
    /// cannot boot over it.
    pub async fn start(data_root: impl AsRef<Path>, owner_name: &str) -> eyre::Result<Self> {
        let root = data_root.as_ref().to_path_buf();
        let opts = NodeOptions {
            owner_name: owner_name.to_owned(),
            ..NodeOptions::at(&root)
        };
        Self::open(opts).await
    }

    /// Boot with explicit options (peers off for in-process-only tests,
    /// the example studio planted in debug builds).
    ///
    /// # Errors
    ///
    /// As [`Node::start`].
    pub async fn open(opts: NodeOptions) -> eyre::Result<Self> {
        let root = opts.data_root.clone();
        let inner = LocalNode::open(opts)
            .await
            .wrap_err_with(|| format!("boot the Task node at {}", root.display()))?;
        Ok(Self { root, inner })
    }

    /// The data root the node runs over.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// This device's endpoint id for `slug`, or `None` when that org is
    /// not held here or its endpoint is not bound.
    #[must_use]
    pub fn endpoint_id(&self, slug: &str) -> Option<String> {
        self.inner.endpoint_id(slug)
    }

    /// This device's endpoint id for its personal org: what a peer
    /// enrolls to admit this install.
    #[must_use]
    pub fn personal_endpoint_id(&self) -> Option<String> {
        self.endpoint_id(PERSONAL_SLUG)
    }

    /// The orgs this node holds, sorted.
    #[must_use]
    pub fn org_slugs(&self) -> Vec<String> {
        self.inner.org_slugs()
    }

    /// Task's node itself: in-process lanes (`caller`), `join`,
    /// `admit_peer`, `admit_account`.
    #[must_use]
    pub const fn local(&self) -> &LocalNode {
        &self.inner
    }
}

//! The `document` contract: frame's own document model ([`model`]), the
//! typed edits over it ([`ops`]), and the service that serves documents.
//!
//! The entity/service declarations below are architect's: everything else
//! — the vox service, the HTTP routes, both clients, the storage — is
//! generated.

mod hug;
pub mod model;
pub mod ops;

pub use model::*;
pub use ops::{History, Op, OpError, Prop};

use uuid::Uuid;

/// A document's index row (what a file browser lists). The content — the
/// [`Scene`] — lives in the document's own store (a Loro replica / `.frame`
/// bundle), not in this row.
#[architect::entity(table_name = "documents", repo, events)]
pub struct Document {
    #[architect(primary_key, auto_increment = false, on_create = Uuid::new_v4())]
    pub id: Uuid,
    #[architect(filterable, sortable)]
    pub name: String,
}

/// How this feature fails. The status, the code and the transport
/// variant (what lets the generated clients implement the service trait)
/// are written by the attribute.
#[architect::error]
#[derive(Eq)]
pub enum DocumentError {
    #[error("not found")]
    NotFound,
}

/// The service: `POST /document/<method-kebab>` over HTTP, the same
/// methods over vox, and clients for both that implement this trait.
#[architect::service]
pub trait DocumentService {
    /// Look one up by name.
    async fn find(&self, name: String) -> Result<Document, DocumentError>;
}

// The trait-prefixed mount names (`DocumentServiceService`, `document_service_layer`).
pub use prelude::*;

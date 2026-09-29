//! The `document` contract. Three declarations; everything else — the vox
//! service, the HTTP routes, both clients, the storage, the migration —
//! is generated.

use uuid::Uuid;

/// The entity. `NoteCreate`/`Update`/`List`, the `DocumentRepo` trait,
/// its clients and its storage all come from this.
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

//! An in-memory `document` backend: one struct implementing the repo and
//! the service, and one `Services` impl naming its bundle.

use std::sync::{Arc, Mutex};

use architect::{Filter, Page, RepoError, Sort};
use document_proto::*;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct Memory {
    rows: Arc<Mutex<Vec<Document>>>,
}

impl DocumentService for Memory {
    async fn find(&self, name: String) -> Result<Document, DocumentError> {
        architect::lock(&self.rows)
            .iter()
            .find(|r| r.name == name)
            .cloned()
            .ok_or(DocumentError::NotFound)
    }
}

impl DocumentRepo for Memory {
    async fn get(&self, id: Uuid) -> Result<Document, RepoError> {
        architect::lock(&self.rows)
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or(RepoError::NotFound)
    }
    async fn list(&self, page: Page, _sort: Option<Sort>, _filter: Option<Filter>) -> Result<DocumentList, RepoError> {
        let items = architect::lock(&self.rows).clone();
        let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
        Ok(DocumentList { items, total, page })
    }
    async fn create(&self, input: DocumentCreate) -> Result<Document, RepoError> {
        let row = Document { id: Uuid::new_v4(), name: input.name };
        architect::lock(&self.rows).push(row.clone());
        Ok(row)
    }
    async fn update(&self, id: Uuid, input: DocumentUpdate) -> Result<Document, RepoError> {
        let mut rows = architect::lock(&self.rows);
        let row = rows.iter_mut().find(|r| r.id == id).ok_or(RepoError::NotFound)?;
        if let Some(name) = input.name {
            row.name = name;
        }
        Ok(row.clone())
    }
    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        let mut rows = architect::lock(&self.rows);
        let before = rows.len();
        rows.retain(|r| r.id != id);
        if rows.len() == before {
            return Err(RepoError::NotFound);
        }
        Ok(())
    }
}

impl architect::Services for Memory {
    fn layers() -> impl architect::Layer<Self> {
        architect::layers![DocumentServiceService, DocumentRepoLayer]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use architect::{LocalServer, Scope, Services as _};

    /// The scenario is generic over the trait: the in-process backend and
    /// the vox client both satisfy it.
    async fn finds_what_it_created(s: &(impl DocumentService + DocumentRepo)) {
        let made = s.create(DocumentCreate { name: "one".into() }).await.unwrap();
        assert_eq!(s.find("one".into()).await.unwrap().id, made.id);
        assert_eq!(s.find("two".into()).await.unwrap_err(), DocumentError::NotFound);
    }

    #[tokio::test]
    async fn in_process() {
        finds_what_it_created(&Memory::default()).await;
    }

    #[tokio::test]
    async fn over_vox() {
        let scope = Scope::new();
        let local = LocalServer::serve(Memory::default().into_router(), scope.clone());
        // Two typed clients over one backend; each implements its trait.
        let service: DocumentServiceClient = local.establish().await.unwrap();
        let repo: DocumentRepoClient = local.establish().await.unwrap();
        let made = repo.create(DocumentCreate { name: "one".into() }).await.unwrap();
        assert_eq!(service.find("one".into()).await.unwrap().id, made.id);
        scope.close().await;
    }
}

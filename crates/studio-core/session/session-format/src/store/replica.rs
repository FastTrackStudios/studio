//! A song file's parts as what the session's file writes through
//! (`session::file::PartStore`, implemented by `song-file`): each part of
//! each branch a document of its own, named by its doc id
//! ([`Store::part_id`]; the song's session on `main` is the song's id).
//! The file stays the one place the song lives: the session's writer is
//! one more writer of it, never a second copy.

use uuid::Uuid;

use super::{PartRef, Store, StoreError, StoreResult};

/// The parts of the song file a [`Store`] holds open, by doc id.
pub struct PartReplica {
    store: std::sync::Arc<std::sync::Mutex<Store>>,
}

impl PartReplica {
    /// The parts of the song file `store` holds open; the store stays
    /// shared with the rest of what the app does with the file.
    #[must_use]
    pub const fn new(store: std::sync::Arc<std::sync::Mutex<Store>>) -> Self {
        Self { store }
    }

    /// The store, held only for synchronous calls: the store drives its
    /// database to completion on the calling thread, so nothing awaits
    /// under it.
    fn with<T>(&self, f: impl FnOnce(&Store) -> T) -> T {
        let store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&store)
    }

    fn part(store: &Store, id: Uuid) -> StoreResult<PartRef> {
        store.part_of(id)?.ok_or(StoreError::NotAPart(id))
    }

    /// The part `id` as the file holds it, as one Loro snapshot; `None`
    /// before it holds any (or when `id` is not one of this song's).
    ///
    /// # Errors
    ///
    /// The database refused the read, or the part would not load.
    pub fn load(&self, id: Uuid) -> StoreResult<Option<Vec<u8>>> {
        self.with(|store| {
            let Some(p) = store.part_of(id)? else {
                return Ok(None);
            };
            if !store.has_part(&p.part, &p.branch)? {
                return Ok(None);
            }
            let doc = store.load_part(&p.part, &p.branch)?;
            doc.export(loro::ExportMode::Snapshot)
                .map(Some)
                .map_err(|e| StoreError::Loro(e.to_string()))
        })
    }

    /// Appends one update to part `id`.
    ///
    /// # Errors
    ///
    /// `id` is not a part of this song, or the database refused.
    pub fn append(&self, id: Uuid, update: &[u8]) -> StoreResult<()> {
        self.with(|store| {
            let p = Self::part(store, id)?;
            store.append_part_update(&p.part, &p.branch, update)
        })
    }

    /// Folds part `id` into `snapshot` and what the file holds, both: a
    /// compaction never drops an update appended since the caller's
    /// export.
    ///
    /// # Errors
    ///
    /// `id` is not a part of this song, `snapshot` is not a Loro
    /// document, or the database refused.
    pub fn compact(&self, id: Uuid, snapshot: &[u8]) -> StoreResult<()> {
        self.with(|store| {
            let p = Self::part(store, id)?;
            let doc = store.load_part(&p.part, &p.branch)?;
            doc.import(snapshot)
                .map_err(|e| StoreError::Loro(e.to_string()))?;
            store.compact_part(&p.part, &p.branch, &doc)
        })
    }
}

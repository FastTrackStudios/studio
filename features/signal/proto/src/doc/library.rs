//! A Signal library and the three ways of cutting it into docs.
//!
//! - [`Layout::PerEntity`] — one doc per rig, patch, song and setlist,
//!   named by the entity's key (`patch:worship-lead`).
//! - [`Layout::WholeLibrary`] — one doc, `library`, holding everything.
//! - [`Layout::PerSetlist`] — one doc per setlist, `set:<id>`, holding the
//!   setlist and everything it plays (its songs, their patches, the rigs
//!   those patches target), plus `library:rest` for what no setlist plays.
//!   An entity a second setlist also plays is copied into both.
//!
//! In every layout a song points at its patches the way the model does —
//! by id inside the doc — and across docs by key: [`Library::patch_refs`]
//! gives a setlist's patches as `patch:<slug>`, the reference Task's
//! setlist keeps (live S8).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::profile::{Patch, PatchTarget, Profile};
use crate::rig::Rig;
use crate::setlist::Setlist;
use crate::song::{SceneSource, Song};

use super::{DocEntity, DocError, EntityKind, SignalDoc};

/// The entities of a library (or of one doc's share of it).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    pub rigs: Vec<Rig>,
    pub patches: Vec<Patch>,
    pub songs: Vec<Song>,
    pub setlists: Vec<Setlist>,
}

impl Library {
    /// A library from profiles: their patches, flattened.
    #[must_use]
    pub fn from_profiles(
        rigs: Vec<Rig>,
        profiles: &[Profile],
        songs: Vec<Song>,
        setlists: Vec<Setlist>,
    ) -> Self {
        Self {
            rigs,
            patches: profiles
                .iter()
                .flat_map(|p| p.patches.iter().cloned())
                .collect(),
            songs,
            setlists,
        }
    }

    /// The keys of every entity, sorted.
    #[must_use]
    pub fn keys(&self) -> BTreeSet<String> {
        self.rigs
            .iter()
            .map(DocEntity::doc_key)
            .chain(self.patches.iter().map(DocEntity::doc_key))
            .chain(self.songs.iter().map(DocEntity::doc_key))
            .chain(self.setlists.iter().map(DocEntity::doc_key))
            .collect()
    }

    /// How many entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rigs
            .len()
            .saturating_add(self.patches.len())
            .saturating_add(self.songs.len())
            .saturating_add(self.setlists.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The patch with id `id`.
    #[must_use]
    pub fn patch_by_id(&self, id: &crate::profile::PatchId) -> Option<&Patch> {
        self.patches.iter().find(|p| &p.id == id)
    }

    /// The patch with key `key` (`patch:<slug>`).
    #[must_use]
    pub fn patch(&self, key: &str) -> Option<&Patch> {
        self.patches.iter().find(|p| p.doc_key() == key)
    }

    pub fn patch_mut(&mut self, key: &str) -> Option<&mut Patch> {
        self.patches.iter_mut().find(|p| p.doc_key() == key)
    }

    #[must_use]
    pub fn song(&self, key: &str) -> Option<&Song> {
        self.songs.iter().find(|s| s.doc_key() == key)
    }

    pub fn song_mut(&mut self, key: &str) -> Option<&mut Song> {
        self.songs.iter_mut().find(|s| s.doc_key() == key)
    }

    #[must_use]
    pub fn setlist(&self, key: &str) -> Option<&Setlist> {
        self.setlists.iter().find(|s| s.doc_key() == key)
    }

    pub fn setlist_mut(&mut self, key: &str) -> Option<&mut Setlist> {
        self.setlists.iter_mut().find(|s| s.doc_key() == key)
    }

    /// The patch keys a setlist plays, in set order, each once — what a
    /// setlist references across docs.
    #[must_use]
    pub fn patch_refs(&self, setlist: &Setlist) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for song in self.songs_of(setlist) {
            for scene in &song.scenes {
                if let SceneSource::Patch { patch_id } = &scene.source
                    && let Some(patch) = self.patch_by_id(patch_id)
                {
                    let key = patch.doc_key();
                    if seen.insert(key.clone()) {
                        out.push(key);
                    }
                }
            }
        }
        out
    }

    fn songs_of<'a>(&'a self, setlist: &'a Setlist) -> impl Iterator<Item = &'a Song> + 'a {
        setlist
            .entries
            .iter()
            .filter_map(|entry| self.songs.iter().find(|s| s.id == entry.song_id))
    }

    /// Everything `setlist` plays: the setlist, its songs, their patches
    /// (and the patches those point at), and the rigs all of them target.
    #[must_use]
    pub fn closure(&self, setlist: &Setlist) -> Self {
        let mut songs: Vec<Song> = Vec::new();
        for song in self.songs_of(setlist) {
            if !songs.iter().any(|s| s.id == song.id) {
                songs.push(song.clone());
            }
        }
        let mut patch_ids = Vec::new();
        let mut rig_ids = HashSet::new();
        for song in &songs {
            for scene in &song.scenes {
                match &scene.source {
                    SceneSource::Patch { patch_id } => patch_ids.push(patch_id.clone()),
                    SceneSource::RigScene { rig_id, .. } => {
                        rig_ids.insert(rig_id.clone());
                    }
                    SceneSource::Node { .. } => {}
                }
            }
        }
        let mut patches: Vec<Patch> = Vec::new();
        while let Some(id) = patch_ids.pop() {
            if patches.iter().any(|p| p.id == id) {
                continue;
            }
            let Some(patch) = self.patch_by_id(&id) else {
                continue;
            };
            match &patch.target {
                PatchTarget::RigScene { rig_id, .. } => {
                    rig_ids.insert(rig_id.clone());
                }
                PatchTarget::Patch { patch_id } => patch_ids.push(patch_id.clone()),
                _ => {}
            }
            patches.push(patch.clone());
        }
        patches.sort_by_key(DocEntity::doc_key);
        Self {
            rigs: self
                .rigs
                .iter()
                .filter(|r| rig_ids.contains(&r.id))
                .cloned()
                .collect(),
            patches,
            songs,
            setlists: vec![setlist.clone()],
        }
    }

    /// The entities whose keys are not in `keys`.
    #[must_use]
    pub fn without(&self, keys: &BTreeSet<String>) -> Self {
        Self {
            rigs: keep(&self.rigs, |e| !keys.contains(&e.doc_key())),
            patches: keep(&self.patches, |e| !keys.contains(&e.doc_key())),
            songs: keep(&self.songs, |e| !keys.contains(&e.doc_key())),
            setlists: keep(&self.setlists, |e| !keys.contains(&e.doc_key())),
        }
    }

    fn single(key: &str, from: &Self) -> Self {
        Self {
            rigs: keep(&from.rigs, |e| e.doc_key() == key),
            patches: keep(&from.patches, |e| e.doc_key() == key),
            songs: keep(&from.songs, |e| e.doc_key() == key),
            setlists: keep(&from.setlists, |e| e.doc_key() == key),
        }
    }

    /// Fold `other` in; an entity already here (by key) is kept as is.
    pub fn merge(&mut self, other: Self) {
        let have = self.keys();
        let fresh = other.without(&have);
        self.rigs.extend(fresh.rigs);
        self.patches.extend(fresh.patches);
        self.songs.extend(fresh.songs);
        self.setlists.extend(fresh.setlists);
    }
}

fn keep<T: Clone>(items: &[T], f: impl Fn(&T) -> bool) -> Vec<T> {
    items.iter().filter(|i| f(i)).cloned().collect()
}

/// Which entities go in which doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    PerEntity,
    WholeLibrary,
    PerSetlist,
}

impl Layout {
    pub const ALL: [Self; 3] = [Self::PerEntity, Self::WholeLibrary, Self::PerSetlist];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerEntity => "per-entity",
            Self::WholeLibrary => "whole-library",
            Self::PerSetlist => "per-setlist",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_str() == s)
    }

    /// Each doc's name and the share of `library` it holds.
    #[must_use]
    pub fn contents(self, library: &Library) -> BTreeMap<String, Library> {
        match self {
            Self::PerEntity => library
                .keys()
                .into_iter()
                .map(|key| {
                    let share = Library::single(&key, library);
                    (key, share)
                })
                .collect(),
            Self::WholeLibrary => BTreeMap::from([("library".to_owned(), library.clone())]),
            Self::PerSetlist => {
                let mut out = BTreeMap::new();
                let mut played = BTreeSet::new();
                for setlist in &library.setlists {
                    let share = library.closure(setlist);
                    played.extend(share.keys());
                    out.insert(format!("set:{}", setlist.id), share);
                }
                let rest = library.without(&played);
                if !rest.is_empty() {
                    out.insert("library:rest".to_owned(), rest);
                }
                out
            }
        }
    }
}

impl SignalDoc {
    /// Reconcile the doc with `share`: put every entity, drop every entity
    /// the share no longer holds. Not committed.
    ///
    /// # Errors
    /// When an entity does not serialize or a Loro operation fails.
    pub fn write_library(&self, share: &Library) -> Result<(), DocError> {
        for rig in &share.rigs {
            self.put(rig)?;
        }
        for patch in &share.patches {
            self.put(patch)?;
        }
        for song in &share.songs {
            self.put(song)?;
        }
        for setlist in &share.setlists {
            self.put(setlist)?;
        }
        let wanted = share.keys();
        for kind in EntityKind::ALL {
            for key in self.keys(kind) {
                if !wanted.contains(&key) {
                    self.remove(&key)?;
                }
            }
        }
        Ok(())
    }

    /// Everything the doc holds, as a library.
    ///
    /// # Errors
    /// When an entity does not read back.
    pub fn read_library(&self) -> Result<Library, DocError> {
        Ok(Library {
            rigs: self.all()?,
            patches: self.all()?,
            songs: self.all()?,
            setlists: self.all()?,
        })
    }
}

/// The library as docs in `layout`, each written and committed under
/// `origin`.
///
/// # Errors
/// When an entity does not serialize or a Loro operation fails.
pub fn build(
    layout: Layout,
    library: &Library,
    origin: &str,
) -> Result<BTreeMap<String, SignalDoc>, DocError> {
    layout
        .contents(library)
        .into_iter()
        .map(|(name, share)| {
            let doc = SignalDoc::new();
            doc.write_library(&share)?;
            doc.commit(origin);
            Ok((name, doc))
        })
        .collect()
}

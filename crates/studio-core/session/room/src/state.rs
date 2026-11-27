//! What a live room shares.
//!
//! **The doc** ([`RoomDoc`]): one Loro doc per room, its root map `room`
//! holding what the leader has loaded ([`Loaded`]) and a count of loads
//! (`seq`), so a member opens each load once, the same setlist loaded
//! again included. Only the leader writes it.
//!
//! **The presence**, under the room's id too: each person writes who they
//! are under [`here_key`], and the leader writes the transport everyone
//! follows under session-sync's `transport::KEY`. It is ephemeral, as a
//! playhead and who is here should be: gone with the person.

use std::path::{Path, PathBuf};

use loro::{LoroDoc, LoroValue};

/// The doc's root map.
const ROOM: &str = "room";

/// What is loaded: a setlist, or a lone session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Setlist,
    Session,
}

impl Kind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Setlist => "setlist",
            Self::Session => "session",
        }
    }
}

/// What the leader has loaded into the room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub kind: Kind,
    /// The file, relative to the Studio folder when it is inside it (it
    /// means the same file on every device), else as it is.
    pub path: String,
    /// What the room shows it as.
    pub title: String,
}

impl Loaded {
    /// `path` loaded, kept relative to `studio_root` when it is inside it.
    /// A `.setlist` is a setlist; anything else opens as a session.
    #[must_use]
    pub fn of_path(path: &Path, studio_root: Option<&Path>) -> Self {
        let kind = if path.extension().is_some_and(|e| e == "setlist") {
            Kind::Setlist
        } else {
            Kind::Session
        };
        let title = path.file_stem().map_or_else(
            || path.display().to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        let kept = studio_root
            .and_then(|root| path.strip_prefix(root).ok())
            .filter(|rel| !rel.as_os_str().is_empty())
            .unwrap_or(path);
        Self {
            kind,
            path: kept.to_string_lossy().into_owned(),
            title,
        }
    }

    /// The file on this device: a relative path under `studio_root`.
    #[must_use]
    pub fn resolve(&self, studio_root: Option<&Path>) -> PathBuf {
        let path = Path::new(&self.path);
        match studio_root {
            Some(root) if path.is_relative() => root.join(path),
            _ => path.to_path_buf(),
        }
    }
}

/// A replica of a room's doc. Cloning shares it.
#[derive(Clone, Default)]
pub struct RoomDoc {
    doc: LoroDoc,
}

impl RoomDoc {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The Loro doc, for its sync.
    #[must_use]
    pub const fn loro(&self) -> &LoroDoc {
        &self.doc
    }

    /// Load `loaded` into the room: the leader's.
    ///
    /// # Errors
    ///
    /// Loro refused the write.
    pub fn load(&self, loaded: &Loaded) -> Result<(), loro::LoroError> {
        let map = self.doc.get_map(ROOM);
        let seq = self.loaded().map_or(0, |(_, seq)| seq).saturating_add(1);
        map.insert("kind", loaded.kind.as_str())?;
        map.insert("path", loaded.path.as_str())?;
        map.insert("title", loaded.title.as_str())?;
        map.insert("seq", i64::try_from(seq).unwrap_or(i64::MAX))?;
        self.doc.commit();
        Ok(())
    }

    /// What is loaded, and which load it is (each load counts up).
    #[must_use]
    pub fn loaded(&self) -> Option<(Loaded, u64)> {
        let LoroValue::Map(map) = self.doc.get_map(ROOM).get_value() else {
            return None;
        };
        let text = |k: &str| match map.get(k)? {
            LoroValue::String(s) => Some(s.to_string()),
            _ => None,
        };
        let kind = match text("kind")?.as_str() {
            "setlist" => Kind::Setlist,
            _ => Kind::Session,
        };
        let seq = match map.get("seq")? {
            LoroValue::I64(n) => u64::try_from(*n).ok()?,
            _ => return None,
        };
        Some((
            Loaded {
                kind,
                path: text("path")?,
                title: text("title").unwrap_or_default(),
            },
            seq,
        ))
    }
}

/// The presence key person `peer` says who they are under.
#[must_use]
pub fn here_key(peer: &str) -> String {
    format!("{peer}/here")
}

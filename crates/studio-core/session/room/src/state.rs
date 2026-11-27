//! What a live room shares.
//!
//! **The doc** ([`RoomDoc`]): one Loro doc per room, its root map `room`
//! holding what the leader has loaded ([`Loaded`]) and a count of loads
//! (`seq`), so a member opens each load once, the same setlist loaded
//! again included. Only the leader writes it.
//!
//! **The share** ([`Share`]), in the same doc: where the leader shares the
//! set it loaded so everyone edits it together (engine-collab's live set,
//! served over iroh), and which load it is for. A member joins it once it
//! has opened that load, and the edits of everyone in the room merge
//! through it. Only the leader writes it.
//!
//! **The presence**, under the room's id too: each person writes who they
//! are under [`here_key`], and the leader writes the transport everyone
//! follows under session-sync's `transport::KEY`. It is ephemeral, as a
//! playhead and who is here should be: gone with the person.

use std::path::{Path, PathBuf};

use loro::{LoroDoc, LoroValue};
use serde::{Deserialize, Serialize};

/// The doc's root map.
const ROOM: &str = "room";

/// The share's root map.
const SHARE: &str = "share";

/// What is loaded: a setlist, or a lone session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// What the leader has loaded into the room. The room's file keeps the
/// last one (`Room::setlist`), for when nobody leads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loaded {
    pub kind: Kind,
    /// The file, relative to the Studio folder when it is inside it (it
    /// means the same file on every device), else as it is.
    pub path: String,
    /// What the room shows it as.
    #[serde(default)]
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

    /// Say where load `share.load` is shared: the leader's.
    ///
    /// # Errors
    ///
    /// Loro refused the write.
    pub fn share(&self, share: &Share) -> Result<(), loro::LoroError> {
        let map = self.doc.get_map(SHARE);
        map.insert("load", i64::try_from(share.load).unwrap_or(i64::MAX))?;
        map.insert("ticket", share.ticket.as_str())?;
        map.insert("addrs", share.addrs.join("\n").as_str())?;
        self.doc.commit();
        Ok(())
    }

    /// Where the leader shares what it loaded, once it does. A share for
    /// an earlier load stays until the leader shares the next.
    #[must_use]
    pub fn shared(&self) -> Option<Share> {
        let LoroValue::Map(map) = self.doc.get_map(SHARE).get_value() else {
            return None;
        };
        let text = |k: &str| match map.get(k)? {
            LoroValue::String(s) => Some(s.to_string()),
            _ => None,
        };
        let load = match map.get("load")? {
            LoroValue::I64(n) => u64::try_from(*n).ok()?,
            _ => return None,
        };
        Some(Share {
            load,
            ticket: text("ticket")?,
            addrs: text("addrs")
                .unwrap_or_default()
                .split('\n')
                .filter(|a| !a.is_empty())
                .map(str::to_owned)
                .collect(),
        })
    }
}

/// Where the leader shares what it loaded, for everyone in the room to
/// edit it together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    /// The load it shares ([`RoomDoc::loaded`]'s count): a member joins
    /// it only once it has opened that load.
    pub load: u64,
    /// The live set's ticket (`fts-session:<endpoint>/<set>`).
    pub ticket: String,
    /// The addresses the leader's live set is served at, so a member on
    /// the same network dials it directly.
    pub addrs: Vec<String>,
}

/// The presence key person `peer` says who they are under.
#[must_use]
pub fn here_key(peer: &str) -> String {
    format!("{peer}/here")
}

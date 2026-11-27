//! The room as its organization keeps it: one JSON file per room in the
//! org's Rooms folder (`<Studio>/Rooms/<room>.room`, beside its Songs and
//! Setlists), so every device the folder reaches knows the room.
//!
//! The room's id is made once, from its name and a few random letters
//! (`adult-jam-3f9a1c`), and is its link's last part: the link never
//! changes, whoever leads and whatever is loaded.
//!
//! **Finding the leader.** While someone leads the room, its file says
//! where to reach them ([`Live`]: their endpoint and its addresses), and
//! nothing when it is not live. That is the seam a directory kept by the
//! organization's server takes over later; a member only ever asks
//! [`Rooms::find`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where a room's link points.
pub const JOIN_BASE: &str = "https://fasttrackstudio.app/join/";

/// The same link in the app's own scheme: what opens Studio on a phone or
/// tablet straight into the room (`fasttrackstudio://join/<room>`, the URL
/// type the iOS build declares), where no browser stands in between.
pub const APP_JOIN_BASE: &str = "fasttrackstudio://join/";

/// A room's id: lower-case letters, digits and dashes, made once.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomId(String);

impl RoomId {
    /// `id` as a room's id, when it is one.
    ///
    /// # Errors
    ///
    /// [`Error::NotALink`] when it has anything but `a-z`, `0-9` and `-`.
    pub fn parse(id: &str) -> Result<Self, Error> {
        let ok = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if ok {
            Ok(Self(id.to_owned()))
        } else {
            Err(Error::NotALink(id.to_owned()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The id the room's live doc and presence are shared under.
    #[must_use]
    pub fn doc_id(&self) -> uuid::Uuid {
        const NAMESPACE: uuid::Uuid =
            uuid::Uuid::from_u128(0x3b8e_51d2_7c04_4e19_a6f3_0d92_c5e7_18ab);
        uuid::Uuid::new_v5(&NAMESPACE, self.0.as_bytes())
    }
}

impl std::fmt::Display for RoomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a live room's leader is reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Live {
    /// The leader's name, as the room shows it.
    pub leader: String,
    /// The leader's endpoint (an iroh endpoint id).
    pub endpoint: String,
    /// Its addresses (`ip:port`), dialled directly: the room works on a
    /// local network with no internet.
    pub addrs: Vec<String>,
    /// Its home relays (`https://…`): what a browser dials it through (a
    /// page has no direct path to a device), and any member whose direct
    /// paths are all closed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relays: Vec<String>,
    /// What the leader admits its members by ([`Pass`]): made new each
    /// time it leads the room.
    #[serde(default, skip_serializing_if = "Pass::is_empty")]
    pub pass: Pass,
}

/// What a room's leader admits its members by.
///
/// The leader makes one each time it leads the room and writes it in the
/// room's file, in its organization's Rooms folder. Only the
/// organization's people can read that folder: on a device through Drive,
/// in a browser through the node with their sign-in. A member presents
/// the pass on every call to the leader, and the leader's gate refuses a
/// call without it (`gate`). Knowing the link, or the leader's endpoint,
/// is not enough to join.
///
/// The leader leading again (another device, or after a restart) makes a
/// new one, so someone who left the organization holds a pass that opens
/// nothing after that. A pass is a credential: it is never logged (its
/// `Debug` hides it).
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pass(String);

impl Pass {
    /// A new pass: 244 random bits, as hex.
    #[must_use]
    pub fn mint() -> Self {
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        Self(format!("{}{}", a.simple(), b.simple()))
    }

    /// No pass: a room's file from before passes, or not live.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The pass as it is presented, for the call that presents it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether `presented` is this pass. No pass matches nothing, and the
    /// comparison takes as long whichever byte differs.
    #[must_use]
    pub fn matches(&self, presented: &str) -> bool {
        let (ours, theirs) = (self.0.as_bytes(), presented.as_bytes());
        if ours.is_empty() || ours.len() != theirs.len() {
            return false;
        }
        ours.iter()
            .zip(theirs)
            .fold(0u8, |differ, (a, b)| differ | (a ^ b))
            == 0
    }
}

impl std::fmt::Debug for Pass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            f.write_str("Pass(none)")
        } else {
            f.write_str("Pass(…)")
        }
    }
}

/// One room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub id: RoomId,
    pub name: String,
    /// The organization that owns it (its slug).
    pub org: String,
    /// Where its leader is, while it is live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<Live>,
}

impl Room {
    /// The room's one link.
    #[must_use]
    pub fn link(&self) -> String {
        link_of(&self.id)
    }
}

/// The link of room `id`.
#[must_use]
pub fn link_of(id: &RoomId) -> String {
    format!("{JOIN_BASE}{id}")
}

/// The room a link names: the whole link, the link without its scheme, the
/// link in the app's scheme ([`APP_JOIN_BASE`]), or the bare id.
///
/// # Errors
///
/// [`Error::NotALink`] when it names no room.
pub fn parse_link(link: &str) -> Result<RoomId, Error> {
    let link = link.trim();
    let rest = link
        .strip_prefix("https://")
        .or_else(|| link.strip_prefix("http://"))
        .unwrap_or(link);
    let id = match rest
        .strip_prefix("fasttrackstudio.app/join/")
        .or_else(|| link.strip_prefix(APP_JOIN_BASE))
    {
        Some(id) => id.trim_end_matches('/'),
        None if rest.contains('/') => return Err(Error::NotALink(link.to_owned())),
        None => rest,
    };
    RoomId::parse(id).map_err(|_| Error::NotALink(link.to_owned()))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0:?} is not a room's link")]
    NotALink(String),
    #[error("no room {0}")]
    NoRoom(RoomId),
    #[error("a room needs {0}")]
    Missing(&'static str),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Bad {
        path: PathBuf,
        source: serde_json::Error,
    },
}

/// An organization's Rooms folder.
#[derive(Debug, Clone)]
pub struct Rooms {
    dir: PathBuf,
}

/// The variable that names the Rooms folder instead of the default.
pub const ROOMS_DIR_ENV: &str = "FTS_ROOMS_DIR";

impl Rooms {
    /// The rooms in `dir`.
    #[must_use]
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// This device's Rooms folder: [`ROOMS_DIR_ENV`], else
    /// `~/Drive/Studio/Rooms` (Studio's folder, beside Songs and
    /// Setlists); `None` with neither.
    #[must_use]
    pub fn here() -> Option<Self> {
        if let Some(dir) = std::env::var_os(ROOMS_DIR_ENV).filter(|d| !d.is_empty()) {
            return Some(Self::at(PathBuf::from(dir)));
        }
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
        Some(Self::at(
            PathBuf::from(home)
                .join("Drive")
                .join("Studio")
                .join("Rooms"),
        ))
    }

    /// The folder.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The Studio folder the Rooms folder is in: what a loaded path is
    /// kept relative to, so it means the same file on every device.
    #[must_use]
    pub fn studio_root(&self) -> Option<&Path> {
        self.dir.parent()
    }

    fn file(&self, id: &RoomId) -> PathBuf {
        self.dir.join(format!("{id}.room"))
    }

    /// Make a room named `name` for organization `org`.
    ///
    /// # Errors
    ///
    /// No name or no organization, or the folder cannot be written.
    pub fn create(&self, name: &str, org: &str) -> Result<Room, Error> {
        let name = name.trim();
        let org = org.trim();
        if name.is_empty() {
            return Err(Error::Missing("a name"));
        }
        if org.is_empty() {
            return Err(Error::Missing("an organization"));
        }
        let room = loop {
            let id = RoomId(format!("{}-{}", slug(name), suffix()));
            if !self.file(&id).exists() {
                break Room {
                    id,
                    name: name.to_owned(),
                    org: org.to_owned(),
                    live: None,
                };
            }
        };
        self.write(&room)?;
        Ok(room)
    }

    /// The room `link` names.
    ///
    /// # Errors
    ///
    /// Not a room's link, no such room here, or its file is unreadable.
    pub fn find(&self, link: &str) -> Result<Room, Error> {
        let id = parse_link(link)?;
        let path = self.file(&id);
        match std::fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|source| Error::Bad { path, source })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::NoRoom(id)),
            Err(source) => Err(Error::Io { path, source }),
        }
    }

    /// Every room in the folder, by id. A file that is not a room's (a
    /// name no room has, or unreadable) is passed over, and said.
    ///
    /// # Errors
    ///
    /// The folder cannot be read.
    pub fn list(&self) -> Result<Vec<Room>, Error> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    path: self.dir.clone(),
                    source,
                });
            }
        };
        let mut rooms = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "room")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                match self.find(stem) {
                    Ok(room) => rooms.push(room),
                    Err(e) => {
                        tracing::warn!(room.error = %e, "room: not a room's file; passed over")
                    }
                }
            }
        }
        rooms.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(rooms)
    }

    /// Say where the room's leader is (`Some`), or that it is not live.
    ///
    /// # Errors
    ///
    /// No such room, or its file cannot be written.
    pub fn set_live(&self, id: &RoomId, live: Option<Live>) -> Result<Room, Error> {
        let mut room = self.find(id.as_str())?;
        room.live = live;
        self.write(&room)?;
        Ok(room)
    }

    /// Write `room` whole: to a file beside it first, then moved over, so
    /// a reader never sees half of one.
    fn write(&self, room: &Room) -> Result<(), Error> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| Error::Io { path, source }
        };
        std::fs::create_dir_all(&self.dir).map_err(io(&self.dir))?;
        let path = self.file(&room.id);
        let next = self.dir.join(format!(".{}.room.{}", room.id, suffix()));
        let text = serde_json::to_string_pretty(room).map_err(|source| Error::Bad {
            path: path.clone(),
            source,
        })?;
        std::fs::write(&next, format!("{text}\n")).map_err(io(&next))?;
        std::fs::rename(&next, &path).map_err(io(&path))
    }
}

/// `name` as the readable part of an id: lower case, words joined by dashes.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "room".to_owned()
    } else {
        out.chars()
            .take(40)
            .collect::<String>()
            .trim_end_matches('-')
            .to_owned()
    }
}

/// Six random hex letters.
fn suffix() -> String {
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(6)
        .collect()
}

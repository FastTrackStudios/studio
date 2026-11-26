//! One file per project, on Turso.
//!
//! The project is a Loro document — what everyone editing it, on any
//! device, merges — and this is where it lives on disk: one SQLite-format
//! file, written through [Turso](https://github.com/tursodatabase/turso),
//! holding
//!
//! ```text
//! meta       key → value              the schema, the project's id
//! blobs      hash → bytes (zstd)      plugin states, MIDI sources, an
//!                                     imported project's verbatim text —
//!                                     once each, however many branches
//!                                     or joined projects refer to them
//! branches   name → parent, base      the song (`main`) and its
//!                                     alternatives and checkpoints, each
//!                                     forked from another at a point
//! updates    seq → branch, update     each undo step's edits, appended
//!                                     to its branch (zstd)
//! snapshots  branch → upto, snapshot  a branch's doc as of its update
//!                                     `upto`, folded by [`Store::compact`]
//! history    seq → branch, kind,      each step and mark of a branch: its
//!            label, action, at,       name, the action that made it, when,
//!            frontiers                and the doc's frontier after it —
//!                                     never folded, so a step stays
//!                                     reachable after any compaction
//! part_updates,                       the song's other documents (the
//! part_snapshots                      session's timing, sections, chart,
//!                                     cues), one per branch ([`parts`])
//! ```
//!
//! `meta.id` is the song's id: what a setlist names it by, kept however
//! the file is moved or copied. The file is the whole song; closed
//! ([`Store::close`]) its write-ahead log is folded in, and the one file
//! opens anywhere.
//!
//! A branch is Logic's alternative: `Organized` forked from the song when
//! the organizing was done, edited on its own after. It shares everything
//! before the fork with the song — the history up to there, every blob —
//! and keeps only what it did since.
//!
//! Media is not here: audio stays files beside the project, as it is now.
//!
//! What makes it small is what goes in, not the engine: a project as a
//! Loro doc rather than text, every blob stored once by content, and an
//! alternative a fork of one history rather than a copy of the project. Turso itself does not compress; the blobs and the history are
//! zstd'd here.
//!
//! Turso's API is async; the store drives it to completion on the
//! calling thread, so callers (a save, an open) stay synchronous.

use sha2::{Digest, Sha256};

mod parts;
pub use parts::{ID_KEY, PartRef, SESSION};
#[cfg(feature = "sync")]
mod replica;
#[cfg(feature = "sync")]
pub use replica::{PartReplica, Replica, SYNC_LABEL};

/// How long a write waits for another connection's lock on the file.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The schema this build writes, kept in `meta.schema`.
pub const SCHEMA: i64 = 1;

/// The zstd level blobs and history are compressed at: well past the
/// default's ratio, still fast to write for a save.
const LEVEL: i32 = 9;

/// The zstd level a branch's snapshot is packed at. A long-edited song's
/// snapshot is hundreds of MB: at [`LEVEL`] Unbreakable's 598 MB took
/// 10.6 s to pack, every time the song was left alone; at 3 a fraction
/// of that, for a somewhat larger file.
const SNAPSHOT_LEVEL: i32 = 3;

/// A branch's updates past its snapshot are imported one by one, not as a
/// batch, when there are at most this many and they hold at most
/// [`ONE_BY_ONE_MAX_BYTES`] ([`Store::load`] says why). A first guess:
/// `just test-unbreakable` times it.
const ONE_BY_ONE_MAX_UPDATES: usize = 16;
/// See [`ONE_BY_ONE_MAX_UPDATES`].
const ONE_BY_ONE_MAX_BYTES: usize = 1 << 20;

/// `meta` key: the version a branch's archive holds (hex of its encoded
/// version vector), what the next [`Store::archive`] adds from.
const ARCHIVE_VV: &str = "archive.vv";
/// `meta` key: the last history entry of a branch its archive covers.
const ARCHIVE_HISTORY: &str = "archive.history";

fn archive_key(key: &str, branch: &str) -> String {
    format!("{key}.{branch}")
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    (text.len() % 2 == 0)
        .then(|| {
            (0..text.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
                .collect()
        })
        .flatten()
}

/// A branch's snapshot taken ([`Store::take_snapshot`]), not packed yet.
#[derive(Debug)]
pub struct Snapshot {
    branch: String,
    upto: i64,
    data: Vec<u8>,
}

impl Snapshot {
    /// Packed for the store: the slow part, which needs neither the doc
    /// nor the store.
    ///
    /// # Errors
    ///
    /// zstd refused it.
    pub fn pack(self) -> StoreResult<PackedSnapshot> {
        let span = tracing::info_span!(
            "store.pack",
            store.snapshot_bytes = self.data.len(),
            store.packed_bytes = tracing::field::Empty,
        );
        let _in = span.enter();
        let data = zstd::encode_all(self.data.as_slice(), SNAPSHOT_LEVEL)?;
        span.record("store.packed_bytes", data.len());
        Ok(PackedSnapshot {
            branch: self.branch,
            upto: self.upto,
            data,
        })
    }
}

/// A snapshot packed, to be written ([`Store::write_snapshot`]).
#[derive(Debug)]
pub struct PackedSnapshot {
    branch: String,
    upto: i64,
    data: Vec<u8>,
}

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Db(#[from] turso::Error),
    #[error("compression: {0}")]
    Codec(#[from] std::io::Error),
    #[error("loro: {0}")]
    Loro(String),
    #[error("blob {0} is not in the store")]
    Missing(String),
    #[error("blob {0} does not hash to its name")]
    Corrupt(String),
    #[error("the store is schema {found}, this build reads {SCHEMA}")]
    Schema { found: i64 },
    #[error("no branch {0}")]
    NoBranch(String),
    #[error("the song is {had}, not {asked}: an id names one song for good")]
    IdTaken { had: String, asked: String },
}

/// The song's own branch: what it is when nothing else is named.
pub const MAIN: &str = "main";

/// The store's results.
pub type StoreResult<T> = Result<T, StoreError>;

/// `doc` as it was at `at`, to read: Loro's `fork_at`, which it does not do
/// on a shallow doc ([`Store::compact_shallow`]); there, a copy checked out
/// at `at`, which can be any version since its shallow root. The copy is
/// detached: read it, never edit it.
///
/// # Errors
///
/// `at` is not a version of `doc` (before a shallow doc's root: its
/// archive has it).
pub fn fork_at(doc: &loro::LoroDoc, at: &loro::Frontiers) -> StoreResult<loro::LoroDoc> {
    if !doc.is_shallow() {
        return doc.fork_at(at).map_err(|e| StoreError::Loro(e.to_string()));
    }
    let copy = doc.fork();
    copy.checkout(at)
        .map_err(|e| StoreError::Loro(e.to_string()))?;
    Ok(copy)
}

/// A blob's name: its SHA-256, lowercase hex — the same names the
/// `.session` folder's `objects/` use, so a project moves between them
/// without renaming anything.
#[must_use]
pub fn hash_of(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// A branch: the song, or an alternative or checkpoint of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// The branch it was forked from; `None` for the song itself.
    pub parent: Option<String>,
    /// Seconds since the epoch it was made at.
    pub at: i64,
}

/// One step of a branch's history, as [`Store::steps`] lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The undo step's name.
    pub label: String,
    /// When it was written, in seconds since the epoch.
    pub at: i64,
    /// What it keeps, packed.
    pub bytes: i64,
}

/// What a [`Moment`] of a branch's history is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MomentKind {
    /// A step of edits, as undo names it.
    Step,
    /// A mark set on the song as it was, before a rewrite: a name to go
    /// back to, no branch.
    Mark,
}

impl MomentKind {
    const fn key(self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::Mark => "mark",
        }
    }
}

/// A step or a mark of a branch's history, as [`Store::history`] lists
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moment {
    /// Its number in the file: what a step is gone back to by.
    pub seq: i64,
    pub kind: MomentKind,
    /// The step's name, or the mark's.
    pub label: String,
    /// The action that made it, when one did.
    pub action: Option<String>,
    /// When, in seconds since the epoch.
    pub at: i64,
    /// The doc's frontier after it, encoded (`loro::Frontiers::encode`).
    pub frontiers: Vec<u8>,
}

/// One project's file.
pub struct Store {
    conn: turso::Connection,
    /// Kept so the file stays open as long as the store.
    _db: turso::Database,
}

/// Which moment of a branch's history: a step's (or mark's) number, or a
/// mark's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MomentRef {
    Seq(i64),
    Mark(String),
}

fn moment(row: &turso::Row) -> StoreResult<Moment> {
    let kind: String = row.get(1)?;
    Ok(Moment {
        seq: row.get(0)?,
        kind: if kind == "mark" {
            MomentKind::Mark
        } else {
            MomentKind::Step
        },
        label: row.get(2)?,
        action: row.get::<Option<String>>(3)?,
        at: row.get(4)?,
        frontiers: row.get(5)?,
    })
}

/// Run `future` to completion here.
fn run<T>(future: impl std::future::Future<Output = T>) -> T {
    pollster::block_on(future)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn import(doc: &loro::LoroDoc, packed: &[u8]) -> StoreResult<()> {
    let bytes = tracing::info_span!("store.unpack", store.packed_bytes = packed.len())
        .in_scope(|| zstd::decode_all(packed))?;
    tracing::info_span!("store.import", store.snapshot_bytes = bytes.len()).in_scope(|| {
        doc.import(&bytes)
            .map_err(|e| StoreError::Loro(e.to_string()))
    })?;
    Ok(())
}

impl Store {
    /// Open the project file at `path`, made if it is not there.
    ///
    /// # Errors
    ///
    /// The file would not open, or it is a schema this build does not
    /// read.
    pub fn open(path: &std::path::Path) -> StoreResult<Self> {
        let _span = tracing::info_span!("store.open").entered();
        run(async {
            // Several processes at once — the app saving it, a script or
            // another tool reading it — through a shared write-ahead log.
            let db = turso::Builder::new_local(&path.to_string_lossy())
                .experimental_multiprocess_wal(true)
                .build()
                .await?;
            let conn = db.connect()?;
            // Another connection to the file (another process, or a sync
            // replica's writer and a reader of the same project) may hold
            // its lock for a moment: wait for it rather than fail, or a
            // change written in that moment is lost (the replica's writer
            // logs a refused append and drops it).
            conn.busy_timeout(BUSY_TIMEOUT)?;
            for sql in [
                "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE IF NOT EXISTS blobs (hash TEXT PRIMARY KEY, kind TEXT NOT NULL, \
                 size INTEGER NOT NULL, data BLOB NOT NULL)",
                "CREATE TABLE IF NOT EXISTS branches (name TEXT PRIMARY KEY, parent TEXT, \
                 base BLOB, at INTEGER NOT NULL)",
                "CREATE TABLE IF NOT EXISTS updates (seq INTEGER PRIMARY KEY AUTOINCREMENT, \
                 branch TEXT NOT NULL, label TEXT NOT NULL, at INTEGER NOT NULL, \
                 data BLOB NOT NULL)",
                "CREATE TABLE IF NOT EXISTS snapshots (branch TEXT PRIMARY KEY, \
                 upto INTEGER NOT NULL, at INTEGER NOT NULL, data BLOB NOT NULL)",
                "CREATE TABLE IF NOT EXISTS history (seq INTEGER PRIMARY KEY AUTOINCREMENT, \
                 branch TEXT NOT NULL, kind TEXT NOT NULL, label TEXT NOT NULL, \
                 action TEXT, at INTEGER NOT NULL, frontiers BLOB NOT NULL)",
            ]
            .into_iter()
            .chain(parts::TABLES)
            {
                conn.execute(sql, ()).await?;
            }
            conn.execute(
                "INSERT INTO branches (name, parent, base, at) VALUES (?1, NULL, NULL, ?2) \
                 ON CONFLICT(name) DO NOTHING",
                (MAIN, now()),
            )
            .await?;
            let store = Self { conn, _db: db };
            match store.meta_async("schema").await? {
                None => {
                    store.set_meta_async("schema", &SCHEMA.to_string()).await?;
                }
                Some(found) => {
                    let found = found.parse().unwrap_or(-1);
                    if found != SCHEMA {
                        return Err(StoreError::Schema { found });
                    }
                }
            }
            Ok(store)
        })
    }

    async fn meta_async(&self, key: &str) -> StoreResult<Option<String>> {
        let mut rows = self
            .conn
            .query("SELECT value FROM meta WHERE key = ?1", (key,))
            .await?;
        Ok(match rows.next().await? {
            Some(row) => row.get::<String>(0).ok(),
            None => None,
        })
    }

    async fn set_meta_async(&self, key: &str, value: &str) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (key, value),
            )
            .await?;
        Ok(())
    }

    /// A `meta` value.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn meta(&self, key: &str) -> StoreResult<Option<String>> {
        run(self.meta_async(key))
    }

    /// Set a `meta` value.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn set_meta(&self, key: &str, value: &str) -> StoreResult<()> {
        run(self.set_meta_async(key, value))
    }

    /// Fold what is written into the file itself: the engine writes ahead
    /// into a log beside it (`Song.session-wal`), and a project is one
    /// file to copy, sync or send only once that log is emptied. Done after
    /// a save.
    ///
    /// # Errors
    ///
    /// The database refused.
    pub fn checkpoint(&self) -> StoreResult<()> {
        run(async {
            let mut rows = self
                .conn
                .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
                .await?;
            while rows.next().await?.is_some() {}
            StoreResult::Ok(())
        })
    }

    /// Keep `bytes` (of `kind`: `fx`, `midi`, `rpp`, …) and say its name.
    /// Bytes already here are not written again: a blob is stored once
    /// however many branches or joined projects use it.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn put_blob(&self, kind: &str, bytes: &[u8]) -> StoreResult<String> {
        let hash = hash_of(bytes);
        let packed = zstd::encode_all(bytes, LEVEL)?;
        let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        run(self.conn.execute(
            "INSERT INTO blobs (hash, kind, size, data) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(hash) DO NOTHING",
            (hash.as_str(), kind, size, packed),
        ))?;
        Ok(hash)
    }

    /// The blob named `hash`, checked against its name.
    ///
    /// # Errors
    ///
    /// It is not here, or what is here is not what the name says.
    pub fn blob(&self, hash: &str) -> StoreResult<Vec<u8>> {
        let packed: Vec<u8> = run(async {
            let mut rows = self
                .conn
                .query("SELECT data FROM blobs WHERE hash = ?1", (hash,))
                .await?;
            match rows.next().await? {
                Some(row) => Ok(row.get::<Vec<u8>>(0)?),
                None => Err(StoreError::Missing(hash.to_owned())),
            }
        })?;
        let bytes = zstd::decode_all(packed.as_slice())?;
        if hash_of(&bytes) != hash {
            return Err(StoreError::Corrupt(hash.to_owned()));
        }
        Ok(bytes)
    }

    /// How many blobs, and their bytes before and after compression.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn blob_totals(&self) -> StoreResult<(i64, i64, i64)> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT COUNT(*), COALESCE(SUM(size), 0), COALESCE(SUM(LENGTH(data)), 0) \
                     FROM blobs",
                    (),
                )
                .await?;
            Ok(match rows.next().await? {
                Some(row) => (row.get(0)?, row.get(1)?, row.get(2)?),
                None => (0, 0, 0),
            })
        })
    }

    /// Every branch: the song first, then its alternatives oldest first.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn branches(&self) -> StoreResult<Vec<Branch>> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT name, parent, at FROM branches \
                     ORDER BY (name = ?1) DESC, at, name",
                    (MAIN,),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(Branch {
                    name: row.get(0)?,
                    parent: row.get::<Option<String>>(1)?,
                    at: row.get(2)?,
                });
            }
            Ok(out)
        })
    }

    /// Fork `name` from `parent` at `doc`'s current state (`doc` being
    /// `parent` as loaded, with whatever it holds unsaved appended first by
    /// the caller). A branch of that name already here is replaced.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn fork(&self, name: &str, parent: &str, doc: &loro::LoroDoc) -> StoreResult<()> {
        let base = doc.state_frontiers().encode();
        // The parent's parts (the session's timing, the chart) as they are
        // now: small, so copied whole, history and all.
        let parts = self.part_snapshots_of(parent)?;
        run(async {
            self.conn.execute("BEGIN", ()).await?;
            for sql in [
                "DELETE FROM updates WHERE branch = ?1",
                "DELETE FROM snapshots WHERE branch = ?1",
                "DELETE FROM history WHERE branch = ?1",
                "DELETE FROM part_updates WHERE branch = ?1",
                "DELETE FROM part_snapshots WHERE branch = ?1",
                "DELETE FROM branches WHERE name = ?1",
            ] {
                self.conn.execute(sql, (name,)).await?;
            }
            self.conn
                .execute(
                    "INSERT INTO branches (name, parent, base, at) VALUES (?1, ?2, ?3, ?4)",
                    (name, parent, base, now()),
                )
                .await?;
            for (part, data) in parts {
                self.conn
                    .execute(
                        "INSERT INTO part_snapshots (part, branch, upto, at, data) \
                         VALUES (?1, ?2, 0, ?3, ?4)",
                        (part.as_str(), name, now(), data),
                    )
                    .await?;
            }
            self.conn.execute("COMMIT", ()).await?;
            StoreResult::Ok(())
        })?;
        // A branch loads by forking its parent where it forked, which Loro
        // cannot do on a shallow doc: from a shallow song, the branch keeps
        // a snapshot of its own (the song as it is now, where it forks).
        if doc.is_shallow() {
            self.compact(name, doc)?;
        }
        Ok(())
    }

    /// Forget branch `name` and what it did; the song itself cannot go.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn delete_branch(&self, name: &str) -> StoreResult<()> {
        if name == MAIN {
            return Err(StoreError::NoBranch(name.to_owned()));
        }
        run(async {
            for sql in [
                "DELETE FROM updates WHERE branch = ?1",
                "DELETE FROM snapshots WHERE branch = ?1",
                "DELETE FROM history WHERE branch = ?1",
                "DELETE FROM part_updates WHERE branch = ?1",
                "DELETE FROM part_snapshots WHERE branch = ?1",
                "DELETE FROM branches WHERE name = ?1",
            ] {
                self.conn.execute(sql, (name,)).await?;
            }
            StoreResult::Ok(())
        })
    }

    /// Append one step of edits — a Loro update, `label` the undo step's
    /// name — to `branch`.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn append_update(&self, branch: &str, label: &str, update: &[u8]) -> StoreResult<()> {
        let packed = zstd::encode_all(update, LEVEL)?;
        run(self.conn.execute(
            "INSERT INTO updates (branch, label, at, data) VALUES (?1, ?2, ?3, ?4)",
            (branch, label, now(), packed),
        ))?;
        Ok(())
    }

    /// `branch`'s own steps since its last compaction, oldest first: each
    /// one's name, when, and how many bytes it keeps.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn steps(&self, branch: &str) -> StoreResult<Vec<Step>> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT label, at, length(data) FROM updates WHERE branch = ?1 ORDER BY seq",
                    (branch,),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(Step {
                    label: row.get(0)?,
                    at: row.get(1)?,
                    bytes: row.get(2)?,
                });
            }
            Ok(out)
        })
    }

    /// Note a step or a mark of `branch`: `label`, the action that made it,
    /// and `frontiers`, the doc's after it. Kept through every compaction.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn note(
        &self,
        branch: &str,
        (kind, label, action): (MomentKind, &str, Option<&str>),
        frontiers: &loro::Frontiers,
    ) -> StoreResult<i64> {
        run(async {
            self.conn
                .execute(
                    "INSERT INTO history (branch, kind, label, action, at, frontiers) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    (branch, kind.key(), label, action, now(), frontiers.encode()),
                )
                .await?;
            let mut rows = self.conn.query("SELECT last_insert_rowid()", ()).await?;
            Ok(match rows.next().await? {
                Some(row) => row.get(0)?,
                None => 0,
            })
        })
    }

    /// `branch`'s history, newest first: `limit` moments from `offset`,
    /// and how many it has in all.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn history(
        &self,
        branch: &str,
        (offset, limit): (u32, u32),
    ) -> StoreResult<(Vec<Moment>, u32)> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT seq, kind, label, action, at, frontiers FROM history \
                     WHERE branch = ?1 ORDER BY seq DESC LIMIT ?2 OFFSET ?3",
                    (branch, i64::from(limit), i64::from(offset)),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(moment(&row)?);
            }
            let mut count = self
                .conn
                .query("SELECT COUNT(*) FROM history WHERE branch = ?1", (branch,))
                .await?;
            let total: i64 = match count.next().await? {
                Some(row) => row.get(0)?,
                None => 0,
            };
            Ok((out, u32::try_from(total).unwrap_or(u32::MAX)))
        })
    }

    /// The moment of `branch` numbered `seq`, or its newest mark called
    /// `name`.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn moment(&self, branch: &str, which: &MomentRef) -> StoreResult<Option<Moment>> {
        run(async {
            let mut rows = match which {
                MomentRef::Seq(seq) => {
                    self.conn
                        .query(
                            "SELECT seq, kind, label, action, at, frontiers FROM history \
                             WHERE branch = ?1 AND seq = ?2",
                            (branch, *seq),
                        )
                        .await?
                }
                MomentRef::Mark(name) => {
                    self.conn
                        .query(
                            "SELECT seq, kind, label, action, at, frontiers FROM history \
                             WHERE branch = ?1 AND kind = 'mark' AND label = ?2 \
                             ORDER BY seq DESC LIMIT 1",
                            (branch, name.as_str()),
                        )
                        .await?
                }
            };
            Ok(match rows.next().await? {
                Some(row) => Some(moment(&row)?),
                None => None,
            })
        })
    }

    /// The names of `branch`'s marks.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn mark_names(&self, branch: &str) -> StoreResult<Vec<String>> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT label FROM history WHERE branch = ?1 AND kind = 'mark'",
                    (branch,),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(row.get(0)?);
            }
            Ok(out)
        })
    }

    /// Give `from`'s history to `to`: what a branch kept aside before it
    /// starts over from another lineage takes with it, the lineage its
    /// frontiers are of.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn move_history(&self, from: &str, to: &str) -> StoreResult<()> {
        run(async {
            self.conn
                .execute(
                    "UPDATE history SET branch = ?2 WHERE branch = ?1",
                    (from, to),
                )
                .await?;
            StoreResult::Ok(())
        })
    }

    /// `branch`'s doc: its snapshot if it has one, else its parent forked
    /// where it forked; then every update of its own after that.
    ///
    /// # Errors
    ///
    /// No such branch, the database refused the read, or the history
    /// would not import.
    pub fn load(&self, branch: &str) -> StoreResult<loro::LoroDoc> {
        // Where the doc starts: its own snapshot, its parent forked, or
        // nothing (every update replayed).
        let span = tracing::info_span!("store.load", store.from = tracing::field::Empty);
        let _in = span.enter();
        let (parent, base): (Option<String>, Option<Vec<u8>>) = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT parent, base FROM branches WHERE name = ?1",
                    (branch,),
                )
                .await?;
            match rows.next().await? {
                Some(row) => Ok((row.get(0)?, row.get(1)?)),
                None => Err(StoreError::NoBranch(branch.to_owned())),
            }
        })?;
        let read = tracing::info_span!(
            "store.read_snapshot",
            store.packed_bytes = tracing::field::Empty
        )
        .entered();
        let snapshot: Option<(i64, Vec<u8>)> = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT upto, data FROM snapshots WHERE branch = ?1",
                    (branch,),
                )
                .await?;
            StoreResult::Ok(match rows.next().await? {
                Some(row) => Some((row.get(0)?, row.get(1)?)),
                None => None,
            })
        })?;
        read.record(
            "store.packed_bytes",
            snapshot.as_ref().map_or(0, |(_, data)| data.len()),
        );
        drop(read);
        let (doc, upto) = match (snapshot, parent, base) {
            (Some((upto, packed)), _, _) => {
                span.record("store.from", "snapshot");
                let doc = loro::LoroDoc::new();
                let _span = tracing::info_span!("store.import_snapshot").entered();
                import(&doc, &packed)?;
                (doc, upto)
            }
            (None, Some(parent), Some(base)) => {
                span.record("store.from", "fork");
                let from = self.load(&parent)?;
                let at =
                    loro::Frontiers::decode(&base).map_err(|e| StoreError::Loro(e.to_string()))?;
                (
                    from.fork_at(&at)
                        .map_err(|e| StoreError::Loro(e.to_string()))?,
                    0,
                )
            }
            _ => {
                span.record("store.from", "empty");
                (loro::LoroDoc::new(), 0)
            }
        };
        let read = tracing::info_span!(
            "store.read_updates",
            store.upto = upto,
            store.updates = tracing::field::Empty,
            store.packed_bytes = tracing::field::Empty,
        )
        .entered();
        let updates: Vec<Vec<u8>> = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT data FROM updates WHERE branch = ?1 AND seq > ?2 ORDER BY seq",
                    (branch, upto),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(row.get::<Vec<u8>>(0)?);
            }
            StoreResult::Ok(out)
        })?;
        read.record("store.updates", updates.len());
        read.record(
            "store.packed_bytes",
            updates.iter().map(Vec::len).sum::<usize>(),
        );
        drop(read);
        // All at once: imported one by one, each update had the doc's
        // whole state worked out again — 377 of them, 17 s to open a song.
        // A batch decodes them all and works the state out once.
        let bytes = tracing::info_span!("store.unpack_updates", store.updates = updates.len())
            .in_scope(|| {
                updates
                    .iter()
                    .map(|packed| zstd::decode_all(packed.as_slice()))
                    .collect::<Result<Vec<Vec<u8>>, _>>()
            })?;
        if !bytes.is_empty() {
            let total: usize = bytes.iter().map(Vec::len).sum();
            // How: a batch imports the history alone and then works the
            // whole state out at once (Loro's closing checkout), which costs
            // what the state is, however little the updates hold: 72 s for
            // two updates of 206 bytes on Unbreakable's 1.47 GB snapshot. One
            // by one, each import applies its own ops to the state, which
            // costs what they are, but a long run pays that again and again
            // (377 of them, 17 s). So a few small ones go one by one.
            let one_by_one = bytes.len() <= ONE_BY_ONE_MAX_UPDATES && total <= ONE_BY_ONE_MAX_BYTES;
            let span = tracing::info_span!(
                "store.import_updates",
                store.updates = bytes.len(),
                store.bytes = total,
                store.how = if one_by_one { "one_by_one" } else { "batch" },
            );
            let _in = span.enter();
            if one_by_one {
                for update in &bytes {
                    doc.import(update)
                        .map_err(|e| StoreError::Loro(e.to_string()))?;
                }
            } else {
                doc.import_batch(&bytes)
                    .map_err(|e| StoreError::Loro(e.to_string()))?;
            }
        }
        Ok(doc)
    }

    /// Start `branch` again from `doc`, a doc of a new lineage holding what
    /// the branch holds now: its snapshot, its updates dropped, and no fork
    /// point (the parent's history it pointed into is not this doc's). What
    /// a song's live state accumulates — maps deleted and made again, every
    /// one kept — is left behind; Unbreakable's 157 MB of state was 26 MB
    /// written fresh. Rebase every branch of a project together: an
    /// alternative without a snapshot of its own loads from its parent's
    /// history, which this replaces.
    ///
    /// # Errors
    ///
    /// The doc would not export, or the database refused the write.
    pub fn rebase(&self, branch: &str, doc: &loro::LoroDoc) -> StoreResult<()> {
        self.compact(branch, doc)?;
        run(async {
            self.conn
                .execute("UPDATE branches SET base = NULL WHERE name = ?1", (branch,))
                .await?;
            StoreResult::Ok(())
        })
    }

    /// Fold `branch`'s history into one snapshot of `doc` (which must be
    /// the branch as loaded, with every update appended so far) and drop
    /// the updates it covers. History is kept — a Loro snapshot carries
    /// every operation — so a branch forked from this one still finds its
    /// fork point.
    ///
    /// # Errors
    ///
    /// The doc would not export, or the database refused the write.
    pub fn compact(&self, branch: &str, doc: &loro::LoroDoc) -> StoreResult<()> {
        let taken = self.take_snapshot(branch, doc)?;
        self.write_snapshot(taken.pack()?)
    }

    /// Where the archive of the project file at `file` is: `Archive/` beside
    /// it, `<its name>.history`. A store as the file is, holding each
    /// branch's whole history; never read to open the song.
    #[must_use]
    pub fn archive_path(file: &std::path::Path) -> std::path::PathBuf {
        let stem = file
            .file_stem()
            .map_or_else(|| "song".into(), |s| s.to_string_lossy().into_owned());
        file.parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("Archive")
            .join(format!("{stem}.history"))
    }

    /// Fold `branch` into a shallow snapshot at `root`: its state, and only
    /// the history since `root` (the version the song was opened at, so what
    /// was done since stays undoable and restorable here). Everything before
    /// goes to `archive` first ([`Self::archive`]). A branch forked from this
    /// one with no snapshot of its own gets one first, since the history it
    /// forked from leaves this file. Loading is then a small snapshot and no
    /// long history: Unbreakable's 1.47 GB snapshot was 173.5 MB, and opened
    /// in 0.57 s instead of 51.5 s.
    ///
    /// # Errors
    ///
    /// The doc would not export, or either file refused the write.
    pub fn compact_shallow(
        &self,
        branch: &str,
        doc: &loro::LoroDoc,
        root: &loro::Frontiers,
        archive: &Store,
    ) -> StoreResult<()> {
        let span = tracing::info_span!(
            "store.compact_shallow",
            store.forks_given_snapshots = tracing::field::Empty,
            store.snapshot_bytes = tracing::field::Empty,
        );
        let _in = span.enter();
        let forks = self.forks_without_snapshot(branch)?;
        span.record("store.forks_given_snapshots", forks.len());
        for fork in &forks {
            let forked = self.load(fork)?;
            self.compact(fork, &forked)?;
        }
        self.archive(branch, doc, archive)?;
        let taken = self.take_snapshot_as(branch, doc, loro::ExportMode::shallow_snapshot(root))?;
        span.record("store.snapshot_bytes", taken.data.len());
        self.write_snapshot(taken.pack()?)
    }

    /// Put into `archive` what `branch` holds that it does not yet: the
    /// whole doc the first time (its snapshot), then the updates since.
    /// Records what it holds, and the last history entry it covers
    /// ([`Self::archived_through`]).
    ///
    /// # Errors
    ///
    /// The doc would not export, or either file refused the write.
    pub fn archive(&self, branch: &str, doc: &loro::LoroDoc, archive: &Store) -> StoreResult<()> {
        let span = tracing::info_span!(
            "store.archive",
            store.how = tracing::field::Empty,
            store.bytes = tracing::field::Empty,
        );
        let _in = span.enter();
        archive.ensure_branch(branch)?;
        let held = self
            .meta(&archive_key(ARCHIVE_VV, branch))?
            .and_then(|hex| decode_hex(&hex))
            .and_then(|bytes| loro::VersionVector::decode(&bytes).ok());
        match held {
            None => {
                span.record("store.how", "whole");
                archive.compact(branch, doc)?;
            }
            Some(vv) if vv != doc.oplog_vv() => {
                span.record("store.how", "since");
                let update = doc
                    .export(loro::ExportMode::updates(&vv))
                    .map_err(|e| StoreError::Loro(e.to_string()))?;
                span.record("store.bytes", update.len());
                archive.append_update(branch, "Archived", &update)?;
            }
            Some(_) => {
                span.record("store.how", "nothing");
            }
        }
        self.set_meta(
            &archive_key(ARCHIVE_VV, branch),
            &encode_hex(&doc.oplog_vv().encode()),
        )?;
        let last = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT COALESCE(MAX(seq), 0) FROM history WHERE branch = ?1",
                    (branch,),
                )
                .await?;
            StoreResult::Ok(match rows.next().await? {
                Some(row) => row.get::<i64>(0)?,
                None => 0,
            })
        })?;
        self.set_meta(&archive_key(ARCHIVE_HISTORY, branch), &last.to_string())
    }

    /// The last history entry of `branch` its archive holds (every step and
    /// mark up to it can be restored from there), or `None` before it has
    /// been archived.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn archived_through(&self, branch: &str) -> StoreResult<Option<i64>> {
        Ok(self
            .meta(&archive_key(ARCHIVE_HISTORY, branch))?
            .and_then(|s| s.parse().ok()))
    }

    /// The branches forked from `branch` that have no snapshot of their
    /// own: they load from its history.
    fn forks_without_snapshot(&self, branch: &str) -> StoreResult<Vec<String>> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT name FROM branches WHERE parent = ?1 \
                     AND name NOT IN (SELECT branch FROM snapshots)",
                    (branch,),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(row.get::<String>(0)?);
            }
            StoreResult::Ok(out)
        })
    }

    /// `branch` made in this file if it is not (a branch archived from
    /// another file's), with no parent: its history is its own.
    fn ensure_branch(&self, branch: &str) -> StoreResult<()> {
        run(async {
            self.conn
                .execute(
                    "INSERT INTO branches (name, parent, base, at) VALUES (?1, NULL, NULL, ?2) \
                     ON CONFLICT(name) DO NOTHING",
                    (branch, now()),
                )
                .await?;
            StoreResult::Ok(())
        })
    }

    /// The first step of [`Self::compact`], the only one that needs the
    /// doc still: its snapshot, and how far the branch's updates go now —
    /// what the snapshot holds. The caller may let go of the doc after it
    /// and pack and write the rest elsewhere ([`Snapshot::pack`],
    /// [`Self::write_snapshot`]): updates appended meanwhile stay.
    ///
    /// # Errors
    ///
    /// The doc would not export, or the database would not say.
    pub fn take_snapshot(&self, branch: &str, doc: &loro::LoroDoc) -> StoreResult<Snapshot> {
        self.take_snapshot_as(branch, doc, loro::ExportMode::Snapshot)
    }

    /// [`Self::take_snapshot`], exported as `mode` (a shallow snapshot,
    /// [`Self::compact_shallow`]).
    fn take_snapshot_as(
        &self,
        branch: &str,
        doc: &loro::LoroDoc,
        mode: loro::ExportMode<'_>,
    ) -> StoreResult<Snapshot> {
        let _span = tracing::info_span!("store.take_snapshot").entered();
        let upto = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT COALESCE(MAX(seq), 0) FROM updates WHERE branch = ?1",
                    (branch,),
                )
                .await?;
            let upto: i64 = match rows.next().await? {
                Some(row) => row.get(0)?,
                None => 0,
            };
            StoreResult::Ok(upto)
        })?;
        let data = doc
            .export(mode)
            .map_err(|e| StoreError::Loro(e.to_string()))?;
        Ok(Snapshot {
            branch: branch.to_owned(),
            upto,
            data,
        })
    }

    /// The last step of [`Self::compact`]: a packed snapshot made the
    /// branch's base, and the updates it holds dropped.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn write_snapshot(&self, packed: PackedSnapshot) -> StoreResult<()> {
        let _span = tracing::info_span!("store.write_snapshot").entered();
        let PackedSnapshot { branch, upto, data } = packed;
        run(async {
            self.conn.execute("BEGIN", ()).await?;
            self.conn
                .execute(
                    // Never back: a snapshot packed after a later one
                    // (two folds at once) would put back the updates the
                    // later one dropped as folded.
                    "INSERT INTO snapshots (branch, upto, at, data) VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(branch) DO UPDATE SET upto = excluded.upto, \
                     at = excluded.at, data = excluded.data \
                     WHERE excluded.upto >= snapshots.upto",
                    (branch.as_str(), upto, now(), data),
                )
                .await?;
            self.conn
                .execute(
                    "DELETE FROM updates WHERE branch = ?1 AND seq <= ?2",
                    (branch.as_str(), upto),
                )
                .await?;
            self.conn.execute("COMMIT", ()).await?;
            StoreResult::Ok(())
        })
    }
}

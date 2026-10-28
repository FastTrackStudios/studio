//! What to upload of a song folder: the honest core of ingest.
//!
//! Given what is on this machine ([`Local`]), what the server catalogues
//! ([`RemoteFile`]) and what the last ingest from here recorded ([`Kept`]),
//! each file is skipped or sent, and why ([`decide`]). Identical content
//! is never sent twice: the server addresses a file it placed whole by
//! the hash of a one-chunk manifest over the file's BLAKE3 ([`whole_id`]),
//! which is computed here from the same bytes; a file it chunked instead
//! is recognised by the ledger's record of the bytes sent and the address
//! the server gave them.

use session::library::publish::RemoteFile;

/// Whether the WAVs travel too, or only what a set plays from elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    /// Everything: the originals beside their proxies.
    Originals,
    /// The project, its proxies, peaks and the rest — no uncompressed
    /// audio. A browser or a phone plays the proxies; the originals stay
    /// here.
    ProxiesOnly,
}

/// A file of the song folder on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    /// Relative to the folder, `/`-separated.
    pub rel: String,
    pub size: u64,
    /// Its modification time, nanoseconds since the epoch.
    pub modified: i128,
}

/// What the last ingest from this machine recorded of a file.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub size: u64,
    pub modified: i128,
    /// The file's BLAKE3, hex, when it was hashed.
    #[serde(default)]
    pub blake3: Option<String>,
    /// The address the server gave what was sent.
    #[serde(default)]
    pub remote: Option<String>,
    /// An upload begun and not landed: resumed next time.
    #[serde(default)]
    pub pending: Option<String>,
}

impl Kept {
    /// Whether `local` is the file as it was when this was recorded.
    #[must_use]
    pub fn describes(&self, local: &Local) -> bool {
        self.size == local.size && self.modified == local.modified
    }
}

/// What to do with one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Skip(Same),
    Send(Why),
}

/// How a skipped file is known to be on the server already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Same {
    /// Unchanged here since it was sent, and unchanged there.
    Unchanged,
    /// Its bytes hash to the address the server has for it.
    Content,
}

/// Why a file is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Not on the server.
    New,
    /// On the server, different.
    Changed,
}

/// Whether deciding about `local` needs its hash: only when the server
/// has a file of the same size there that the ledger does not vouch for.
#[must_use]
pub fn needs_hash(local: &Local, remote: Option<&RemoteFile>, kept: Option<&Kept>) -> bool {
    remote.is_some_and(|r| r.size == local.size && !vouched(local, r, kept))
}

/// Skip or send `local`. `hash` is its BLAKE3 (hex) when [`needs_hash`]
/// asked for it.
#[must_use]
pub fn decide(
    local: &Local,
    remote: Option<&RemoteFile>,
    kept: Option<&Kept>,
    hash: Option<&str>,
) -> Decision {
    let Some(remote) = remote else {
        return Decision::Send(Why::New);
    };
    if remote.size != local.size {
        return Decision::Send(Why::Changed);
    }
    if vouched(local, remote, kept) {
        return Decision::Skip(Same::Unchanged);
    }
    let Some(hash) = hash else {
        return Decision::Send(Why::Changed);
    };
    let placed_whole = remote.content.as_deref() == whole_id(hash, local.size).as_deref();
    let sent_before = kept.is_some_and(|k| {
        k.blake3.as_deref() == Some(hash) && k.remote.is_some() && k.remote == remote.content
    });
    if placed_whole || sent_before {
        Decision::Skip(Same::Content)
    } else {
        Decision::Send(Why::Changed)
    }
}

/// The ledger says `local` is what was sent, and the server still has
/// what it was given.
fn vouched(local: &Local, remote: &RemoteFile, kept: Option<&Kept>) -> bool {
    kept.is_some_and(|k| k.describes(local) && k.remote.is_some() && k.remote == remote.content)
}

/// The address the server gives a file it stores whole: the BLAKE3 of
/// its one-chunk manifest (`files_store::chunk::Manifest`: `FTSM`,
/// version 1, one entry of the file's BLAKE3 and length). `None` for a
/// hash that is not 64 hex digits.
#[must_use]
pub fn whole_id(blake3_hex: &str, len: u64) -> Option<String> {
    let hash = blake3::Hash::from_hex(blake3_hex).ok()?;
    let mut manifest = Vec::with_capacity(4 + 1 + 4 + 32 + 8);
    manifest.extend_from_slice(b"FTSM");
    manifest.push(1);
    manifest.extend_from_slice(&1u32.to_le_bytes());
    manifest.extend_from_slice(hash.as_bytes());
    manifest.extend_from_slice(&len.to_le_bytes());
    Some(blake3::hash(&manifest).to_hex().to_string())
}

/// Whether the file at `rel` (relative to a song folder) is part of the
/// song that travels: not the server's own bookkeeping, not a log or a
/// lock another program holds, not half a file still being written, and
/// — with [`Media::ProxiesOnly`] — not an original.
#[must_use]
pub fn travels(rel: &str, media: Media) -> bool {
    let lower = rel.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let in_kept_dir = lower
        .split('/')
        .rev()
        .skip(1)
        .any(|dir| dir.starts_with('.') || dir == "__macosx");
    let kept_here = name.starts_with('.')
        || name == "thumbs.db"
        || [
            ".partial",
            ".tmp",
            ".session-wal",
            ".session-shm",
            ".session-tshm",
            ".reapeaks",
            ".rpp-bak",
            ".rpp-undo",
        ]
        .iter()
        .any(|ext| name.ends_with(ext));
    if in_kept_dir || kept_here {
        return false;
    }
    media == Media::Originals || !is_original(&lower)
}

/// Whether `lower` (a lower-case relative path) is an original recording:
/// uncompressed or lossless audio, not under a `proxies` folder — what a
/// proxy stands in for. Wherever it sits: a REAPER song keeps them in
/// `Media/`, a Live set's in `Samples/`.
fn is_original(lower: &str) -> bool {
    const ORIGINALS: [&str; 9] = [
        ".wav", ".wave", ".bwf", ".rf64", ".w64", ".aif", ".aiff", ".flac", ".caf",
    ];
    !lower.split('/').any(|dir| dir == "proxies")
        && ORIGINALS.iter().any(|ext| lower.ends_with(ext))
}

#[cfg(test)]
mod tests;

//! Wiring between a [`Song`]'s binary media and Task's content-addressed
//! [`AttachmentService`](attachments_proto::AttachmentService).
//!
//! This module is gated behind the additive **`attachments`** cargo feature
//! so the default `song` build stays dependency-light (schema + folder I/O
//! only). Enabling it pulls in `attachments-proto` (default-features off — no
//! vox), giving the client-side upload/resolve flow described in
//! `plans/decentralized-foundation.md` §13 Phase 7:
//!
//! 1. [`upload_arrangement_attachment`] — `initiate_upload` → PUT the bytes →
//!    `complete_upload`, then record the resulting [`AttachmentRef`]
//!    (content-hash id + sha256 + kind) onto the arrangement.
//! 2. [`resolve_attachment_url`] / [`resolve_arrangement_urls`] — turn an
//!    arrangement's recorded refs into short-lived
//!    [`SignedDownloadUrl`]s for consumers (playback, bundles).
//!
//! Everything works against the [`AttachmentService`] **trait**, so no running
//! server is required — callers inject any client (a real vox client, or a
//! mock in tests). The raw byte-PUT step is likewise abstracted behind the
//! [`BlobPut`] trait: in production that's an HTTP client PUT-ing to the
//! signed `upload_url`; in tests a mock that captures + hashes the bytes.

use std::path::Path;

use attachments_proto::{
    AttachmentError, AttachmentService, CompleteUpload, ContentHashArg, InitiateUpload,
    SignedDownloadUrl, UploadTicket,
};
use thiserror::Error;

use crate::model::{ArrangementId, AttachmentRef, Song};

/// Transport for the raw byte-PUT step of an upload.
///
/// [`AttachmentService::initiate_upload`] returns a signed [`UploadTicket`]
/// with a PUT `upload_url`; something has to actually push the bytes there.
/// The server hashes the body as it reads and returns the computed sha256 —
/// which [`AttachmentService::complete_upload`] then requires. This trait
/// captures exactly that step: given the ticket + bytes, PUT them and return
/// the content hash (sha256 hex).
///
/// Kept out of `song`'s dependency graph on purpose: production wires an HTTP
/// client here, tests wire an in-memory mock, and neither pins a concrete
/// networking dep into this schema crate.
pub trait BlobPut {
    /// PUT `bytes` to the ticket's `upload_url`, returning the server-computed
    /// sha256 hex content hash.
    fn put_bytes(
        &self,
        ticket: &UploadTicket,
        bytes: &[u8],
    ) -> impl std::future::Future<Output = Result<String, AttachmentError>>;
}

/// Errors from the attachment wiring.
#[derive(Debug, Error)]
pub enum AttachError {
    /// Reading the local file to upload failed.
    #[error("io reading `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The referenced arrangement isn't part of the song.
    #[error("arrangement `{0}` not found in song")]
    NoArrangement(ArrangementId),

    /// An [`AttachmentRef`] had neither a `sha256` nor a usable `id` to
    /// address the blob by.
    #[error("attachment ref has no content hash to resolve")]
    NoContentHash,

    /// The underlying [`AttachmentService`] (or blob transport) failed.
    #[error(transparent)]
    Service(#[from] AttachmentError),
}

/// Upload a local binary file as an attachment for `arrangement`, recording
/// the resulting [`AttachmentRef`] onto that arrangement in `song`.
///
/// Runs the full three-step flow against the injected `service` +
/// `blob_put`: `initiate_upload` (scoped to the song's id as `doc_id`) → PUT
/// the bytes → `complete_upload`. The committed blob is content-addressed, so
/// the recorded ref's `id` **and** `sha256` are both the server-computed
/// content hash; `path` is left `None` (the bytes live in the blob store, not
/// in-folder). `kind` overrides the hint derived from the file's mime type.
///
/// Returns a clone of the recorded ref.
///
/// # Errors
/// - [`AttachError::Io`] if `local_file` can't be read.
/// - [`AttachError::NoArrangement`] if `arrangement` isn't in `song`.
/// - [`AttachError::Service`] if the service or blob PUT fails.
pub async fn upload_arrangement_attachment<S, P>(
    service: &S,
    blob_put: &P,
    song: &mut Song,
    arrangement: ArrangementId,
    local_file: &Path,
    kind: Option<String>,
) -> Result<AttachmentRef, AttachError>
where
    S: AttachmentService,
    P: BlobPut,
{
    // Fail fast if the target arrangement doesn't exist, before doing I/O or
    // touching the service.
    if song.arrangement(arrangement).is_none() {
        return Err(AttachError::NoArrangement(arrangement));
    }

    let bytes = std::fs::read(local_file).map_err(|source| AttachError::Io {
        path: local_file.display().to_string(),
        source,
    })?;
    let filename = local_file
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "attachment".to_string());
    let mime_type = guess_mime(&filename).to_string();

    let ticket = service
        .initiate_upload(InitiateUpload {
            doc_id: song.id.to_string(),
            filename,
            mime_type: mime_type.clone(),
            size_bytes: bytes.len() as u64,
        })
        .await?;

    let content_hash = blob_put.put_bytes(&ticket, &bytes).await?;

    let meta = service
        .complete_upload(CompleteUpload {
            upload_id: ticket.upload_id,
            content_hash,
        })
        .await?;

    let att = AttachmentRef {
        id: meta.content_hash.clone(),
        path: None,
        sha256: Some(meta.content_hash.clone()),
        kind: Some(kind.unwrap_or_else(|| kind_from_mime(&meta.mime_type).to_string())),
    };

    // Already checked present above; find again to record.
    let arr = song
        .arrangements
        .iter_mut()
        .find(|a| a.id == arrangement)
        .ok_or(AttachError::NoArrangement(arrangement))?;
    arr.attachment_refs.push(att.clone());
    Ok(att)
}

/// Resolve a single [`AttachmentRef`] to a short-lived signed download URL.
///
/// Addresses the blob by the ref's `sha256`, falling back to `id` (which for
/// content-addressed attachments is the same hash).
///
/// # Errors
/// - [`AttachError::NoContentHash`] if the ref carries no usable hash.
/// - [`AttachError::Service`] if the service fails (e.g. `NotFound`).
pub async fn resolve_attachment_url<S: AttachmentService>(
    service: &S,
    att: &AttachmentRef,
) -> Result<SignedDownloadUrl, AttachError> {
    let content_hash = att
        .sha256
        .clone()
        .or_else(|| (!att.id.is_empty()).then(|| att.id.clone()))
        .ok_or(AttachError::NoContentHash)?;
    Ok(service
        .get_download_url(ContentHashArg { content_hash })
        .await?)
}

/// Resolve every attachment ref on `arrangement` to a signed download URL.
///
/// Returns one entry per ref, pairing the ref with its resolution result so a
/// single failure doesn't sink the batch.
///
/// # Errors
/// [`AttachError::NoArrangement`] if `arrangement` isn't in `song`.
pub async fn resolve_arrangement_urls<'a, S: AttachmentService>(
    service: &S,
    song: &'a Song,
    arrangement: ArrangementId,
) -> Result<Vec<(&'a AttachmentRef, Result<SignedDownloadUrl, AttachError>)>, AttachError> {
    let arr = song
        .arrangement(arrangement)
        .ok_or(AttachError::NoArrangement(arrangement))?;
    let mut out = Vec::with_capacity(arr.attachment_refs.len());
    for att in &arr.attachment_refs {
        let res = resolve_attachment_url(service, att).await;
        out.push((att, res));
    }
    Ok(out)
}

/// Best-effort mime type from a filename extension. Deliberately tiny — a
/// dependency-free hint, not a full mime database.
fn guess_mime(filename: &str) -> &'static str {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "aif" | "aiff" => "audio/aiff",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Coarse `kind` hint (matching [`AttachmentRef::kind`]'s open set) from a
/// mime type: `"audio"`, `"pdf"`, `"image"`, else `"file"`.
fn kind_from_mime(mime: &str) -> &'static str {
    if mime.starts_with("audio/") {
        "audio"
    } else if mime == "application/pdf" {
        "pdf"
    } else if mime.starts_with("image/") {
        "image"
    } else {
        "file"
    }
}

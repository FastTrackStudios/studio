//! Sending song folders to their roots: each folder read ([`scan`]), each
//! file decided against the server's catalogue and this machine's ledger
//! (`super::plan`), and what must go sent by a few workers at once, each on
//! a connection of its own, streamed from disk a frame at a time.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use session::library::Library;
use session::library::publish::{self, FilesClient, ReadAt, RemoteFile, RootId, Upload, UploadId};

use super::ledger::Ledger;
use super::plan::{self, Decision, Kept, Local, Media};

/// One file to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Send {
    pub local: Local,
    pub hash: Option<String>,
    pub why: plan::Why,
}

/// One song's folder against its root: what goes, what is there already.
#[derive(Debug)]
pub struct Batch {
    pub root: RootId,
    pub folder: PathBuf,
    pub sends: Vec<Send>,
    pub skipped: usize,
    pub ledger: Ledger,
    pub ledger_path: PathBuf,
}

/// Every file under `folder` that travels (`plan::travels`), relative to
/// it and sorted. Symlinked files are followed; symlinked folders are not
/// (a song folder that links its show's folder in would send the show).
///
/// # Errors
///
/// The folder could not be read.
pub fn scan(folder: &Path, media: Media) -> std::io::Result<Vec<Local>> {
    let mut out = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(rel) = pending.pop() {
        for entry in std::fs::read_dir(folder.join(&rel))? {
            let entry = entry?;
            let child = rel.join(entry.file_name());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(child);
                continue;
            }
            let Ok(meta) = std::fs::metadata(entry.path()) else {
                continue; // a dangling link
            };
            let rel_text = child
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if meta.is_file() && plan::travels(&rel_text, media) {
                out.push(Local {
                    rel: rel_text,
                    size: meta.len(),
                    modified: nanos(&meta),
                });
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

fn nanos(meta: &std::fs::Metadata) -> i128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i128::try_from(d.as_nanos()).unwrap_or(i128::MAX))
}

/// BLAKE3 of the file at `path`, hex, read in bounded memory.
///
/// # Errors
///
/// The file could not be read.
pub fn blake3_of(path: &Path) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(std::fs::File::open(path)?)?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Decide every file of `folder` against `remote` and `ledger`: the batch
/// to send. Files the server has under another record are written into
/// the ledger as they are, so the next run need not hash them again.
///
/// # Errors
///
/// A file that needed hashing could not be read.
pub fn batch(
    locals: Vec<Local>,
    remote: &[RemoteFile],
    mut ledger: Ledger,
    folder: &Path,
) -> std::io::Result<(Vec<Send>, usize, Ledger)> {
    let remote: std::collections::HashMap<&str, &RemoteFile> =
        remote.iter().map(|r| (r.path.as_str(), r)).collect();
    let mut sends = Vec::new();
    let mut skipped = 0;
    for local in locals {
        let there = remote.get(local.rel.as_str()).copied();
        let kept = ledger.files.get(&local.rel);
        let hash = if plan::needs_hash(&local, there, kept) {
            Some(blake3_of(&folder.join(&local.rel))?)
        } else {
            None
        };
        match plan::decide(&local, there, kept, hash.as_deref()) {
            Decision::Skip(_) => {
                skipped += 1;
                let entry = ledger.files.entry(local.rel.clone()).or_default();
                *entry = Kept {
                    size: local.size,
                    modified: local.modified,
                    blake3: hash.or_else(|| entry.blake3.clone()),
                    remote: there.and_then(|r| r.content.clone()),
                    pending: None,
                };
            }
            Decision::Send(why) => sends.push(Send { local, hash, why }),
        }
    }
    Ok((sends, skipped, ledger))
}

/// A file read for an upload at offsets, by `pread`.
struct FileAt(std::fs::File);

impl ReadAt for FileAt {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::FileExt::read_exact_at(&self.0, buf, offset)
        }
        #[cfg(windows)]
        {
            let mut done = 0;
            while done < buf.len() {
                let n = std::os::windows::fs::FileExt::seek_read(
                    &self.0,
                    &mut buf[done..],
                    offset + done as u64,
                )?;
                if n == 0 {
                    return Err(std::io::ErrorKind::UnexpectedEof.into());
                }
                done += n;
            }
            Ok(())
        }
    }
}

/// How the sending is going, for the caller's progress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sending {
    pub files_done: usize,
    pub files: usize,
    pub bytes_sent: u64,
    pub bytes: u64,
}

/// What sending a set of batches came to, per batch (same order): files
/// landed, bytes that crossed the wire, and the failures.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sent {
    pub landed: usize,
    pub bytes: u64,
    pub failed: Vec<String>,
}

/// Send every batch's files, `workers` at a time, each worker on a
/// connection of its own. A file that fails is reported and the rest go
/// on; each batch's ledger is saved as its files land.
pub async fn send_all(
    library: &Library,
    batches: &mut [Batch],
    workers: usize,
    progress: &(dyn Fn(Sending) + Sync),
) -> Vec<Sent> {
    let queue: Vec<(usize, usize)> = batches
        .iter()
        .enumerate()
        .flat_map(|(b, batch)| (0..batch.sends.len()).map(move |f| (b, f)))
        .collect();
    let total = Sending {
        files: queue.len(),
        bytes: batches
            .iter()
            .flat_map(|b| b.sends.iter())
            .map(|s| s.local.size)
            .sum(),
        ..Sending::default()
    };
    let state = Mutex::new(total);
    let ledgers: Vec<Mutex<(Ledger, PathBuf)>> = batches
        .iter_mut()
        .map(|b| Mutex::new((std::mem::take(&mut b.ledger), b.ledger_path.clone())))
        .collect();
    let results: Vec<Mutex<Sent>> = batches.iter().map(|_| Mutex::default()).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    {
        let batches_ro: &[Batch] = batches;
        let (queue, state, ledgers, results, next) = (&queue, &state, &ledgers, &results, &next);
        let worker = move || async move {
            let files = match library.files_client().await {
                Ok(files) => files,
                Err(e) => {
                    // A worker that cannot connect takes nothing off the
                    // queue: the others carry it.
                    tracing::warn!(error = %e, "ingest: an upload worker could not connect");
                    return;
                }
            };
            loop {
                let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(b, f)) = queue.get(at) else { break };
                let batch = &batches_ro[b];
                let send = &batch.sends[f];
                let outcome = send_one(&files, batch, send, &ledgers[b], state, progress).await;
                if let Ok(mut sent) = results[b].lock() {
                    match outcome {
                        Ok(bytes) => {
                            sent.landed += 1;
                            sent.bytes += bytes;
                        }
                        Err(e) => sent.failed.push(format!("{}: {e}", send.local.rel)),
                    }
                }
                if let Ok(mut s) = state.lock() {
                    s.files_done += 1;
                    progress(*s);
                }
            }
        };
        futures_util::future::join_all((0..workers.max(1)).map(|_| worker())).await;
    }
    for (batch, kept) in batches.iter_mut().zip(ledgers) {
        if let Ok((ledger, _)) = kept.into_inner() {
            batch.ledger = ledger;
        }
    }
    results
        .into_iter()
        .map(|r| r.into_inner().unwrap_or_default())
        .collect()
}

/// Send one file, recording it in the batch's ledger: the upload's id as
/// soon as it has one (so a run cut short resumes it), and what landed.
/// Returns the bytes that crossed the wire.
async fn send_one(
    files: &FilesClient,
    batch: &Batch,
    send: &Send,
    ledger: &Mutex<(Ledger, PathBuf)>,
    state: &Mutex<Sending>,
    progress: &(dyn Fn(Sending) + Sync),
) -> eyre::Result<u64> {
    let path = batch.folder.join(&send.local.rel);
    let source = FileAt(std::fs::File::open(&path)?);
    let content = send
        .hash
        .as_deref()
        .and_then(|h| plan::whole_id(h, send.local.size));
    let resume = ledger
        .lock()
        .ok()
        .and_then(|l| l.0.files.get(&send.local.rel)?.pending.clone())
        .and_then(|id| id.parse::<uuid::Uuid>().ok())
        .map(UploadId::new);
    let keep = |entry: Kept| {
        if let Ok(mut l) = ledger.lock() {
            l.0.files.insert(send.local.rel.clone(), entry);
            if let Err(e) = l.0.save(&l.1) {
                tracing::warn!(error = %e, "ingest: the ledger was not saved");
            }
        }
    };
    let began = |id: UploadId| {
        keep(Kept {
            size: send.local.size,
            modified: send.local.modified,
            blake3: send.hash.clone(),
            remote: None,
            pending: Some(id.to_string()),
        });
    };
    let sent = |n: u64| {
        if let Ok(mut s) = state.lock() {
            s.bytes_sent += n;
            progress(*s);
        }
    };
    let landed = publish::upload(
        files,
        Upload {
            root: batch.root,
            path: &send.local.rel,
            size: send.local.size,
            content: content.as_deref(),
            resume,
        },
        &source,
        &began,
        &sent,
    )
    .await?;
    keep(Kept {
        size: send.local.size,
        modified: send.local.modified,
        blake3: send.hash.clone(),
        remote: landed.content,
        pending: None,
    });
    Ok(landed.sent)
}

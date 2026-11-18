//! Each source's peaks, computed once off every other thread, kept by the
//! hash of their bytes ([`daw_view::peaks`]), and cached on disk by the
//! source's path, size and modification time.
//!
//! A view asks `fts.daw peaks` for a hash it has not seen, once; the
//! clips that share a source share its peaks.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};

use daw_view::Peaks;
use session::Session;

#[derive(Default)]
struct Held {
    /// A source (as the session stores it) → its peaks' hash.
    by_source: HashMap<String, String>,
    /// A hash → the source's rate and the pairs.
    by_hash: HashMap<String, (u32, Vec<u8>)>,
    /// Sources being computed.
    pending: BTreeSet<String>,
}

static HELD: LazyLock<Mutex<Held>> = LazyLock::new(Mutex::default);

fn held() -> std::sync::MutexGuard<'static, Held> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The hash of `source`'s peaks, once computed.
#[must_use]
pub fn hash_of(source: &str) -> Option<String> {
    held().by_source.get(source).cloned()
}

/// Peaks by hash.
#[must_use]
pub fn get(hash: &str) -> Option<Peaks> {
    held().by_hash.get(hash).map(|(rate, bytes)| Peaks {
        hash: hash.to_owned(),
        sample_rate: *rate,
        data: daw_view::peaks::encode(bytes),
    })
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("fts-daw-peaks")
}

fn cache_key(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let key = format!("{}\n{}\n{modified}", path.display(), meta.len());
    Some(blake3::hash(key.as_bytes()).to_hex().to_string())
}

/// `path`'s rate and pairs: from the cache, else read and computed.
///
/// # Errors
///
/// Not a WAV this reads.
pub fn compute(path: &Path) -> Result<(u32, Vec<u8>), String> {
    let cached = cache_key(path).map(|k| cache_dir().join(format!("{k}.peaks")));
    if let Some(c) = &cached
        && let Ok(bytes) = std::fs::read(c)
        && let Some((head, pairs)) = bytes.split_first_chunk::<4>()
    {
        return Ok((u32::from_le_bytes(*head), pairs.to_vec()));
    }
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            #[expect(clippy::cast_precision_loss, reason = "a sample's full scale")]
            let scale = (1_i64 << (spec.bits_per_sample.max(1) - 1)) as f32;
            #[expect(clippy::cast_precision_loss, reason = "a sample")]
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 / scale)
                .collect()
        }
    };
    let pairs = daw_view::peaks::compute(&samples, usize::from(spec.channels));
    if let Some(c) = &cached {
        let _ = std::fs::create_dir_all(cache_dir());
        let mut bytes = spec.sample_rate.to_le_bytes().to_vec();
        bytes.extend_from_slice(&pairs);
        let _ = std::fs::write(c, bytes);
    }
    Ok((spec.sample_rate, pairs))
}

/// Computes the peaks of every source in `s` not held yet, on a few
/// threads of their own.
pub fn start(s: &Session) {
    let (tracks, root) = crate::import::engine_tracks(s);
    let mut todo: Vec<String> = tracks
        .iter()
        .flat_map(|t| t.clips.iter().filter_map(|(_, src)| src.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    {
        let mut h = held();
        todo.retain(|src| !h.by_source.contains_key(src) && !h.pending.contains(src));
        h.pending.extend(todo.iter().cloned());
    }
    let queue = std::sync::Arc::new(Mutex::new(todo));
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    for _ in 0..workers {
        let (queue, root) = (std::sync::Arc::clone(&queue), root.clone());
        let _ = std::thread::Builder::new()
            .name("daw-peaks".to_owned())
            .spawn(move || {
                loop {
                    let next = queue.lock().unwrap_or_else(PoisonError::into_inner).pop();
                    let Some(src) = next else { break };
                    let result = compute(&root.join(&src));
                    let mut h = held();
                    h.pending.remove(&src);
                    if let Ok((rate, pairs)) = result {
                        let hash = daw_view::peaks::hash(&pairs);
                        h.by_source.insert(src, hash.clone());
                        h.by_hash.insert(hash, (rate, pairs));
                        crate::bump();
                    }
                }
            });
    }
}

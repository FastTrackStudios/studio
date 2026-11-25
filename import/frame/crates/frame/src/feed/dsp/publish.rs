//! The control-thread side of an engine's feeds: a [`Publisher`] holds one
//! [`FeedSource`] (a tap's reader) per catalog feed, applies requests
//! (`SurfaceEngine::request_feeds`), and on each [`Publisher::poll`] emits a
//! [`FeedFrame`] for every feed that is due at its rate.

use crate::feed::proto::{EntryId, FeedFrame, FeedInfo, FeedOptions, FeedRequest};

/// Something that produces a feed's frame values on the control thread —
/// every tap's reader half implements it.
pub trait FeedSource: Send {
    /// Reshape for a request: `info` is the feed as resolved
    /// ([`FeedInfo::resolve`]), `options` what was asked.
    fn configure(&mut self, info: &FeedInfo, options: &FeedOptions) {
        let _ = (info, options);
    }

    /// The current frame's values; `dt` is the time since the last read,
    /// seconds.
    fn read(&mut self, dt: f64, out: &mut Vec<f64>);
}

/// Any closure `(dt, out)` is a source (derived feeds, test signals).
impl<F: FnMut(f64, &mut Vec<f64>) + Send> FeedSource for F {
    fn read(&mut self, dt: f64, out: &mut Vec<f64>) {
        self(dt, out);
    }
}

struct Entry {
    /// As the catalog lists it.
    base: FeedInfo,
    /// As last requested.
    current: FeedInfo,
    source: Box<dyn FeedSource>,
    next_due: f64,
    last_read: Option<f64>,
    buf: Vec<f64>,
}

/// An engine's feeds, published at their rates.
#[derive(Default)]
pub struct Publisher {
    entries: Vec<Entry>,
}

impl Publisher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Offer `info`, produced by `source`. A feed with the same id is
    /// replaced.
    pub fn add(&mut self, info: FeedInfo, source: impl FeedSource + 'static) -> &mut Self {
        let mut source: Box<dyn FeedSource> = Box::new(source);
        source.configure(&info, &FeedOptions::default());
        self.entries.retain(|e| e.base.id != info.id);
        self.entries.push(Entry {
            current: info.clone(),
            base: info,
            source,
            next_due: f64::NEG_INFINITY,
            last_read: None,
            buf: Vec::new(),
        });
        self
    }

    /// The feeds as a catalog lists them (`Catalog::streams`).
    #[must_use]
    pub fn infos(&self) -> Vec<FeedInfo> {
        self.entries.iter().map(|e| e.base.clone()).collect()
    }

    /// Feed `id` as currently shaped.
    #[must_use]
    pub fn current(&self, id: &EntryId) -> Option<&FeedInfo> {
        self.entries
            .iter()
            .find(|e| &e.base.id == id)
            .map(|e| &e.current)
    }

    /// Apply requests: each feed is resolved against its catalog entry (not
    /// a previous request) and its source reconfigured.
    ///
    /// # Errors
    /// The first unknown id; nothing is applied then.
    pub fn request(&mut self, requests: &[FeedRequest]) -> Result<Vec<FeedInfo>, EntryId> {
        if let Some(r) = requests
            .iter()
            .find(|r| !self.entries.iter().any(|e| e.base.id == r.id))
        {
            return Err(r.id.clone());
        }
        let mut out = Vec::with_capacity(requests.len());
        for r in requests {
            if let Some(e) = self.entries.iter_mut().find(|e| e.base.id == r.id) {
                e.current = e.base.resolve(&r.options);
                e.source.configure(&e.current, &r.options);
                e.next_due = f64::NEG_INFINITY;
                out.push(e.current.clone());
            }
        }
        Ok(out)
    }

    /// Emit a frame for every feed due at `now` (seconds, any monotonic
    /// clock). Call it from a control-thread timer at least as fast as the
    /// fastest feed.
    pub fn poll(&mut self, now: f64, mut emit: impl FnMut(FeedFrame)) {
        for e in &mut self.entries {
            if now + 1e-4 < e.next_due {
                continue;
            }
            let period = 1.0 / e.current.rate_hz.max(1e-3);
            // Keep the cadence, but never try to catch up a backlog.
            let next = e.next_due + period;
            e.next_due = if next > now { next } else { now + period };
            let dt = e.last_read.map_or(0.0, |t| (now - t).max(0.0));
            e.last_read = Some(now);
            e.source.read(dt, &mut e.buf);
            emit(FeedFrame {
                id: e.current.id.clone(),
                values: e.buf.clone(),
            });
        }
    }
}

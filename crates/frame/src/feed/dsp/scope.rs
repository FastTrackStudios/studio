//! [`ScopeTap`] → [`ScopeReader`]: a triggered waveform. The audio thread
//! pushes samples; the control thread finds the latest trigger whose
//! window is complete and copies `len` samples from it, so a periodic wave
//! stands still.

use crate::feed::proto::{FeedInfo, FeedKind, FeedOptions, Trigger};

use crate::feed::dsp::FeedSource;
use crate::feed::dsp::ring::{RingReader, RingWriter, ring};

/// The audio-thread end.
pub struct ScopeTap {
    ring: RingWriter,
}

/// The control-thread end.
pub struct ScopeReader {
    ring: RingReader,
    len: usize,
    trigger: Trigger,
    scratch: Vec<f32>,
}

/// A scope keeping `capacity` samples (show up to `capacity / 2` with a
/// trigger). Allocates here; the tap never does.
#[must_use]
pub fn scope(capacity: usize) -> (ScopeTap, ScopeReader) {
    let (w, r) = ring(capacity);
    let len = (r.capacity() / 4).max(1);
    (
        ScopeTap { ring: w },
        ScopeReader {
            ring: r,
            len,
            trigger: Trigger::Rising,
            scratch: Vec::new(),
        },
    )
}

impl ScopeTap {
    /// Push a block. Wait-free, no allocation.
    #[inline]
    pub fn push(&mut self, samples: &[f32]) {
        self.ring.push_slice(samples);
    }
}

impl ScopeReader {
    pub fn set_len(&mut self, len: usize) {
        self.len = len.clamp(1, self.ring.capacity());
    }

    pub fn set_trigger(&mut self, trigger: Trigger) {
        self.trigger = trigger;
    }

    /// `len` samples starting at the latest trigger with a full window
    /// after it (the latest `len` when none, or free-running).
    pub fn snapshot(&mut self, out: &mut Vec<f64>) {
        let len = self.len;
        let look = (len * 2).min(self.ring.capacity());
        self.scratch.clear();
        self.scratch.resize(look, 0.0);
        let got = self.ring.latest(&mut self.scratch);
        let data = &self.scratch[look - got..];
        let free = data.len().saturating_sub(len);
        let hit = |i: usize| {
            let (a, b) = (data[i - 1], data[i]);
            match self.trigger {
                Trigger::Rising => a < 0.0 && b >= 0.0,
                Trigger::Falling => a >= 0.0 && b < 0.0,
                Trigger::Free => false,
            }
        };
        let start = (1..=free).rev().find(|&i| hit(i)).unwrap_or(free);
        out.clear();
        out.extend(data[start..].iter().take(len).map(|&x| f64::from(x)));
        out.resize(len, 0.0);
    }
}

impl FeedSource for ScopeReader {
    fn configure(&mut self, info: &FeedInfo, options: &FeedOptions) {
        if let FeedKind::Waveform { len } = info.kind {
            self.set_len(len as usize);
        }
        if let Some(t) = options.trigger {
            self.trigger = t;
        }
    }

    fn read(&mut self, _dt: f64, out: &mut Vec<f64>) {
        self.snapshot(out);
    }
}

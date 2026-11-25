//! Circular buffer delay line with fractional-sample interpolation.

// r[impl dsp.delay.circular-buffer]
pub struct DelayLine {
    buffer: Box<[f64]>,
    write_pos: usize,
}

impl DelayLine {
    pub fn new(max_length: usize) -> Self {
        Self {
            buffer: vec![0.0; max_length].into_boxed_slice(),
            write_pos: 0,
        }
    }

    #[inline(always)]
    pub fn write(&mut self, sample: f64) {
        self.buffer[self.write_pos] = sample;
        self.write_pos += 1;
        if self.write_pos >= self.buffer.len() {
            self.write_pos = 0;
        }
    }

    // r[impl dsp.delay.integer-read]
    #[inline(always)]
    pub fn read(&self, delay_samples: usize) -> f64 {
        // The same index as `(write_pos + len - delay) % len`, without the
        // integer division: a reverb reads its lines dozens of times a
        // sample (four per cubic read), and the `%` was most of their cost.
        let len = self.buffer.len();
        let pos = if let Some(back) = self.write_pos.checked_sub(delay_samples) {
            back
        } else if delay_samples <= self.write_pos + len {
            self.write_pos + len - delay_samples
        } else {
            // Past the line's length: what the release build's wrapping
            // arithmetic always gave.
            self.write_pos.wrapping_add(len).wrapping_sub(delay_samples) % len
        };
        self.buffer[pos]
    }

    // r[impl dsp.delay.fractional-read]
    #[inline]
    pub fn read_linear(&self, delay_samples: f64) -> f64 {
        let int_delay = delay_samples as usize;
        let frac = delay_samples - int_delay as f64;
        let a = self.read(int_delay);
        let b = self.read(int_delay + 1);
        a + frac * (b - a)
    }

    // r[impl dsp.delay.cubic-read]
    /// Catmull-Rom cubic interpolation for higher quality fractional reads.
    #[inline(always)]
    pub fn read_cubic(&self, delay_samples: f64) -> f64 {
        let int_delay = delay_samples as usize;
        let frac = delay_samples - int_delay as f64;

        // Most reads find their four taps side by side behind the write
        // head (no wrap, no clamp): one slice, not four wrapped reads. The
        // same samples the general path reads, so the same result.
        let len = self.buffer.len();
        let contiguous = int_delay >= 2
            && int_delay.saturating_add(3) <= len
            && self.write_pos >= int_delay.saturating_add(2);
        let taps = if contiguous {
            let y1 = self.write_pos - int_delay;
            self.buffer.get(y1 - 2..y1 + 2)
        } else {
            None
        };
        let (y0, y1, y2, y3) = if let Some(&[y3, y2, y1, y0]) = taps {
            (y0, y1, y2, y3)
        } else {
            (
                self.read(int_delay.saturating_sub(1).max(1)),
                self.read(int_delay),
                self.read((int_delay + 1).min(len - 1)),
                self.read((int_delay + 2).min(len - 1)),
            )
        };

        // Catmull-Rom spline
        let a0 = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
        let a1 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let a2 = -0.5 * y0 + 0.5 * y2;
        let a3 = y1;

        ((a0 * frac + a1) * frac + a2) * frac + a3
    }

    /// The buffer and the position the next [`write`](Self::write) goes
    /// to, for a block reader that works out its own indices: `read(d)`
    /// is `buffer[(write_pos + len - d) % len]`.
    #[inline]
    pub fn raw(&self) -> (&[f64], usize) {
        (&self.buffer, self.write_pos)
    }

    /// What [`read`](Self::read)`(delay)` gives at each of the next
    /// `out.len()` samples, provided the line is not written meanwhile from
    /// any sample it would read: `out[k]` is the sample written `delay - k`
    /// writes before now. `delay` at most the line's length.
    #[inline]
    pub fn read_run(&self, delay: usize, out: &mut [f64]) {
        let len = self.buffer.len();
        if len == 0 || delay > len {
            out.fill(0.0);
            return;
        }
        let mut at = self.write_pos + len - delay;
        if at >= len {
            at -= len;
        }
        let mut rest = out;
        while !rest.is_empty() {
            let take = (len - at).min(rest.len());
            let (now, later) = rest.split_at_mut(take);
            now.copy_from_slice(&self.buffer[at..at + take]);
            at += take;
            if at >= len {
                at = 0;
            }
            rest = later;
        }
    }

    /// [`write`](Self::write) each sample of `src`, in order.
    #[inline]
    pub fn write_block(&mut self, src: &[f64]) {
        let len = self.buffer.len();
        if len == 0 {
            return;
        }
        let mut rest = src;
        while !rest.is_empty() {
            let take = (len - self.write_pos).min(rest.len());
            let (now, later) = rest.split_at(take);
            self.buffer[self.write_pos..self.write_pos + take].copy_from_slice(now);
            self.write_pos += take;
            if self.write_pos >= len {
                self.write_pos = 0;
            }
            rest = later;
        }
    }

    // r[impl dsp.delay.clear]
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests;

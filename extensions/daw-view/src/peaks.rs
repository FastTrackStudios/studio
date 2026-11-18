//! Peaks: a source's waveform at [`crate::PEAK_FRAMES`] frames a pair,
//! as signed bytes, named by the hash of those bytes.
//!
//! A pair is `(min, max)` over its frames, every channel together, each
//! scaled to -127..127. A 4-minute source at 48 kHz is ~45 KB.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use crate::PEAK_FRAMES;

/// The pairs of `samples`, interleaved in `channels`.
#[must_use]
pub fn compute(samples: &[f32], channels: usize) -> Vec<u8> {
    let channels = channels.max(1);
    let per = PEAK_FRAMES as usize * channels;
    let mut out = Vec::with_capacity(samples.len() / per * 2 + 2);
    for chunk in samples.chunks(per) {
        let (mut lo, mut hi) = (0.0_f32, 0.0_f32);
        for &s in chunk {
            lo = lo.min(s);
            hi = hi.max(s);
        }
        out.push(byte(lo));
        out.push(byte(hi));
    }
    out
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to -127..=127 first"
)]
fn byte(v: f32) -> u8 {
    ((v.clamp(-1.0, 1.0) * 127.0).round() as i8) as u8
}

/// The pairs as numbers: `(min, max)`, each -1..1.
#[must_use]
pub fn pairs(bytes: &[u8]) -> Vec<(f32, f32)> {
    bytes
        .chunks_exact(2)
        .map(|p| {
            let v = |b: u8| f32::from(b.cast_signed()) / 127.0;
            (v(p[0]), v(p[1]))
        })
        .collect()
}

/// The name of `bytes`: blake3, hex.
#[must_use]
pub fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// # Errors
///
/// Not base64.
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    STANDARD.decode(text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_is_the_min_and_max_of_its_frames_in_every_channel() {
        let mut s = vec![0.0_f32; 2 * 512 * 2];
        s[3] = -0.5;
        s[10] = 1.0;
        s[2 * 512 + 1] = 0.25;
        let p = pairs(&compute(&s, 2));
        assert_eq!(p.len(), 2);
        assert!((p[0].0 + 0.5).abs() < 0.01 && (p[0].1 - 1.0).abs() < 0.01);
        assert!(p[1].0.abs() < 0.01 && (p[1].1 - 0.25).abs() < 0.01);
        let bytes = compute(&s, 2);
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
        assert_eq!(hash(&bytes).len(), 64);
    }
}

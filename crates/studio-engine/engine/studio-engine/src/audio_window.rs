//! A stretch of a recording read straight from its WAV, mono: the header
//! and the frames asked for, never the whole file — what a seam is placed
//! from, a few hundred milliseconds of every mic of two takes.

use std::io::{Read as _, Seek as _, SeekFrom};

/// How much of a file's head is read to find its `data` chunk: room for
/// the broadcast-wave and iXML chunks a recorder writes before it.
const HEAD: usize = 256 * 1024;

/// Where a WAV's samples are and how to read them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The `data` chunk's first byte, and how many bytes it holds.
    data: (u64, u64),
    channels: usize,
    /// Bytes per sample.
    width: usize,
    pub rate: f64,
    format: Format,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Int16,
    Int24,
    Int32,
    Float32,
}

impl Format {
    fn sample(self, b: &[u8]) -> f32 {
        match (self, b) {
            (Self::Int16, [a, b, ..]) => f32::from(i16::from_le_bytes([*a, *b])) / 32_768.0,
            (Self::Int24, [a, b, c, ..]) => {
                num_traits::ToPrimitive::to_f32(&(i32::from_le_bytes([0, *a, *b, *c]) >> 8))
                    .unwrap_or(0.0)
                    / 8_388_608.0
            }
            (Self::Int32, [a, b, c, d, ..]) => {
                num_traits::ToPrimitive::to_f32(&i32::from_le_bytes([*a, *b, *c, *d]))
                    .unwrap_or(0.0)
                    / 2_147_483_648.0
            }
            (Self::Float32, [a, b, c, d, ..]) => f32::from_le_bytes([*a, *b, *c, *d]),
            _ => 0.0,
        }
    }
}

/// The layout of a PCM (16, 24 or 32-bit) or 32-bit float WAV from its
/// head; `None` for anything else, or a head without its `data` chunk.
#[must_use]
pub fn layout(head: &[u8]) -> Option<Layout> {
    if head.get(0..4)? != b"RIFF" || head.get(8..12)? != b"WAVE" {
        return None;
    }
    let u16_at = |at: usize| Some(u16::from_le_bytes(head.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(head.get(at..at + 4)?.try_into().ok()?));
    let mut fmt: Option<(u16, usize, u32, u16)> = None;
    let mut at = 12usize;
    while at + 8 <= head.len() {
        let id = head.get(at..at + 4)?;
        let len = u32_at(at + 4)?;
        let body = at + 8;
        if id == b"fmt " {
            let mut tag = u16_at(body)?;
            // WAVE_FORMAT_EXTENSIBLE: the real format in its sub-format.
            if tag == 0xFFFE && len >= 26 {
                tag = u16_at(body + 24)?;
            }
            fmt = Some((
                tag,
                usize::from(u16_at(body + 2)?),
                u32_at(body + 4)?,
                u16_at(body + 14)?,
            ));
        } else if id == b"data" {
            let (tag, channels, rate, bits) = fmt?;
            let format = match (tag, bits) {
                (1, 16) => Format::Int16,
                (1, 24) => Format::Int24,
                (1, 32) => Format::Int32,
                (3, 32) => Format::Float32,
                _ => return None,
            };
            if rate == 0 || channels == 0 {
                return None;
            }
            return Some(Layout {
                data: (u64::try_from(body).ok()?, u64::from(len)),
                channels,
                width: usize::from(bits / 8),
                rate: f64::from(rate),
                format,
            });
        }
        // Chunks are word-aligned.
        let len = usize::try_from(len).ok()?;
        at = body.saturating_add(len).saturating_add(len & 1);
    }
    None
}

/// `frames` frames of the recording at `path` from frame `from` (before
/// its start or past its end: silence), its channels averaged, and its
/// rate. `None` for a file that will not open or is not a plain WAV.
#[must_use]
pub fn read(path: &str, from: i64, frames: usize) -> Option<(Vec<f32>, f64)> {
    let (mut file, layout) = open(path)?;
    let audio = frames_of(&mut file, &layout, (from, frames))?;
    Some((audio, layout.rate))
}

/// `seconds` of the recording at `path` from `from` seconds into it, as
/// [`read`] reads frames.
#[must_use]
pub fn read_seconds(path: &str, from: f64, seconds: f64) -> Option<(Vec<f32>, f64)> {
    let (mut file, layout) = open(path)?;
    let start = num_traits::ToPrimitive::to_i64(&(from * layout.rate).round())?;
    let frames = num_traits::ToPrimitive::to_usize(&(seconds * layout.rate).round())?;
    let audio = frames_of(&mut file, &layout, (start, frames))?;
    Some((audio, layout.rate))
}

fn open(path: &str) -> Option<(std::fs::File, Layout)> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = vec![0_u8; HEAD];
    let n = file.read(&mut head).ok()?;
    head.truncate(n);
    let layout = layout(&head)?;
    Some((file, layout))
}

fn frames_of(
    file: &mut std::fs::File,
    layout: &Layout,
    (from, frames): (i64, usize),
) -> Option<Vec<f32>> {
    let frame = layout.width * layout.channels;
    let held = usize::try_from(layout.data.1).ok()? / frame;
    let mut out = vec![0.0_f32; frames];
    // The part of the stretch the file holds.
    let first = usize::try_from(from.max(0)).ok()?;
    let skip = usize::try_from(i64::try_from(first).ok()? - from).ok()?;
    let last = usize::try_from(from.saturating_add(i64::try_from(frames).ok()?).max(0))
        .ok()?
        .min(held);
    if last > first && skip < frames {
        let offset = layout.data.0 + u64::try_from(first * frame).ok()?;
        let mut bytes = vec![0_u8; (last - first) * frame];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut bytes).ok()?;
        let scale = 1.0 / num_traits::ToPrimitive::to_f32(&layout.channels).unwrap_or(1.0);
        for (slot, f) in out.iter_mut().skip(skip).zip(bytes.chunks_exact(frame)) {
            *slot = f
                .chunks_exact(layout.width)
                .map(|s| layout.format.sample(s))
                .sum::<f32>()
                * scale;
        }
    }
    Some(out)
}

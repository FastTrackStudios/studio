//! How long a recording runs, read from its header.
//!
//! An MP4 (`.mp4`, `.m4a`, `.mov`: the `moov`/`mvhd` box) or a WAV (its
//! `data` chunk over its byte rate). Nothing is decoded; anything else, or
//! a header that does not read, is None.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The recording at `path`'s length in seconds.
#[must_use]
pub fn length(path: &Path) -> Option<f64> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mut file = File::open(path).ok()?;
    let seconds = match ext.as_str() {
        "mp4" | "m4a" | "m4v" | "mov" => mp4(&mut file),
        "wav" | "wave" => wav(&mut file),
        _ => None,
    }?;
    (seconds.is_finite() && seconds > 0.0).then_some(seconds)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        b.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

/// The boxes from `start` to `end`: the first of type `kind`, as (where
/// its body starts, how long its body is).
fn find_box(file: &mut File, start: u64, end: u64, kind: [u8; 4]) -> Option<(u64, u64)> {
    let mut at = start;
    // A file has a handful of top-level boxes; a bound keeps a bad one
    // from spinning.
    for _ in 0..4096 {
        if at.checked_add(8)? > end {
            return None;
        }
        file.seek(SeekFrom::Start(at)).ok()?;
        let mut head = [0_u8; 16];
        file.read_exact(head.get_mut(..8)?).ok()?;
        let size = u64::from(u32_at(&head, 0)?);
        let (size, header) = match size {
            1 => {
                file.read_exact(head.get_mut(8..16)?).ok()?;
                (u64_at(&head, 8)?, 16)
            }
            0 => (end.checked_sub(at)?, 8),
            n => (n, 8),
        };
        if size < header {
            return None;
        }
        if head.get(4..8)? == kind.as_slice() {
            return Some((at.checked_add(header)?, size.checked_sub(header)?));
        }
        at = at.checked_add(size)?;
    }
    None
}

fn mp4(file: &mut File) -> Option<f64> {
    let end = file.metadata().ok()?.len();
    let (moov, moov_len) = find_box(file, 0, end, *b"moov")?;
    let (mvhd, _) = find_box(file, moov, moov.checked_add(moov_len)?, *b"mvhd")?;
    file.seek(SeekFrom::Start(mvhd)).ok()?;
    let mut b = [0_u8; 32];
    file.read_exact(&mut b).ok()?;
    let (scale, duration) = if b.first() == Some(&1) {
        (u32_at(&b, 20)?, u64_at(&b, 24)?)
    } else {
        (u32_at(&b, 12)?, u64::from(u32_at(&b, 16)?))
    };
    if scale == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    Some(duration as f64 / f64::from(scale))
}

fn wav(file: &mut File) -> Option<f64> {
    let mut riff = [0_u8; 12];
    file.read_exact(&mut riff).ok()?;
    if riff.get(..4)? != b"RIFF" || riff.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut rate: Option<u32> = None;
    for _ in 0..64 {
        let mut head = [0_u8; 8];
        file.read_exact(&mut head).ok()?;
        let size = u32::from_le_bytes(head.get(4..8)?.try_into().ok()?);
        match head.get(..4)? {
            b"fmt " => {
                let mut fmt = [0_u8; 12];
                file.read_exact(&mut fmt).ok()?;
                rate = Some(u32::from_le_bytes(fmt.get(8..12)?.try_into().ok()?));
                file.seek(SeekFrom::Current(i64::from(size).checked_sub(12)?))
                    .ok()?;
            }
            b"data" => {
                let rate = rate.filter(|r| *r > 0)?;
                return Some(f64::from(size) / f64::from(rate));
            }
            _ => {
                file.seek(SeekFrom::Current(i64::from(size))).ok()?;
            }
        }
        if size % 2 == 1 {
            file.seek(SeekFrom::Current(1)).ok()?;
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn put(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn an_mp4_and_a_wav_say_how_long_they_run() {
        let dir = tempfile::tempdir().unwrap();
        // ftyp, then moov with an mvhd (version 0): 600 units a second,
        // 111_600 of them: 186 s.
        let mut mp4 = Vec::new();
        mp4.extend_from_slice(&16_u32.to_be_bytes());
        mp4.extend_from_slice(b"ftypisom\0\0\0\0");
        let mut mvhd = vec![0_u8; 100];
        mvhd[12..16].copy_from_slice(&600_u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&111_600_u32.to_be_bytes());
        let mvhd_len = u32::try_from(8 + mvhd.len()).unwrap();
        mp4.extend_from_slice(&(mvhd_len + 8).to_be_bytes());
        mp4.extend_from_slice(b"moov");
        mp4.extend_from_slice(&mvhd_len.to_be_bytes());
        mp4.extend_from_slice(b"mvhd");
        mp4.extend_from_slice(&mvhd);
        assert_eq!(length(&put(dir.path(), "a.mp4", &mp4)), Some(186.0));
        // 8 kHz 16-bit mono: 16 000 bytes a second, 3 s of it.
        let data = 48_000_u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        wav.resize(wav.len() + 48_000, 0);
        assert_eq!(length(&put(dir.path(), "a.wav", &wav)), Some(3.0));
        assert_eq!(length(&put(dir.path(), "a.txt", b"hello")), None);
        assert_eq!(
            length(&put(dir.path(), "b.mp4", b"not an mp4 at all")),
            None
        );
    }
}

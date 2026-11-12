//! How long a recording runs, read from its header.
//!
//! Nothing is decoded: an MP4 (`.mp4`, `.m4a`, `.mov`: the `moov`/`mvhd`
//! box), a WAV (its `data` chunk over its byte rate), an AIFF (`COMM`'s
//! frames over its rate), a FLAC (`STREAMINFO`'s samples over its rate),
//! an Ogg Vorbis or Opus (its last page's granule over the rate its first
//! names) or an MP3 (a Xing/Info header's frame count, else its bit rate
//! over its size). Anything else, or a header that does not read, is None.

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
        "aif" | "aiff" | "aifc" => aiff(&mut file),
        "flac" => flac(&mut file),
        "ogg" | "oga" | "opus" => ogg(&mut file),
        "mp3" => mp3(&mut file),
        _ => None,
    }?;
    (seconds.is_finite() && seconds > 0.0).then_some(seconds)
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        b.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// `count` over `rate`, as seconds.
#[allow(clippy::cast_precision_loss, clippy::as_conversions)]
fn over(count: u64, rate: u64) -> Option<f64> {
    (rate > 0).then(|| count as f64 / rate as f64)
}

/// Up to `len` bytes at `at`.
fn read_at(file: &mut File, at: u64, len: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = Vec::with_capacity(len);
    file.take(u64::try_from(len).ok()?)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
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
        let head = read_at(file, at, 16)?;
        let size = u64::from(be32(&head, 0)?);
        let (size, header) = match size {
            1 => (be64(&head, 8)?, 16),
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
    let b = read_at(file, mvhd, 32)?;
    let (scale, duration) = if b.first() == Some(&1) {
        (be32(&b, 20)?, be64(&b, 24)?)
    } else {
        (be32(&b, 12)?, u64::from(be32(&b, 16)?))
    };
    over(duration, u64::from(scale))
}

/// What a chunk reader says of a chunk: go on, or the length.
type Seen = Option<Option<f64>>;

/// RIFF (WAV, little-endian) or IFF (AIFF, big-endian) chunks after a
/// 12-byte header: `f` sees each (its id, where its body starts, its
/// size) until it answers. A chunk of odd size is followed by a pad byte.
fn chunks(
    file: &mut File,
    big: bool,
    mut f: impl FnMut(&mut File, &[u8], u64, u32) -> Seen,
) -> Option<f64> {
    let mut at: u64 = 12;
    for _ in 0..256 {
        let head = read_at(file, at, 8)?;
        let id = head.get(..4)?.to_vec();
        let raw: [u8; 4] = head.get(4..8)?.try_into().ok()?;
        let size = if big {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        };
        let body = at.checked_add(8)?;
        if let Some(answer) = f(file, &id, body, size)? {
            return Some(answer);
        }
        at = body
            .checked_add(u64::from(size))?
            .checked_add(u64::from(size % 2))?;
    }
    None
}

fn wav(file: &mut File) -> Option<f64> {
    let riff = read_at(file, 0, 12)?;
    if riff.get(..4)? != b"RIFF" || riff.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut rate: Option<u32> = None;
    chunks(file, false, |file, id, body, size| match id {
        b"fmt " => {
            rate = le32(&read_at(file, body, 12)?, 8);
            Some(None)
        }
        b"data" => Some(Some(over(u64::from(size), u64::from(rate?))?)),
        _ => Some(None),
    })
}

fn aiff(file: &mut File) -> Option<f64> {
    let form = read_at(file, 0, 12)?;
    if form.get(..4)? != b"FORM" || !matches!(form.get(8..12)?, b"AIFF" | b"AIFC") {
        return None;
    }
    chunks(file, true, |file, id, body, _| {
        if id != b"COMM" {
            return Some(None);
        }
        let comm = read_at(file, body, 18)?;
        let frames = be32(&comm, 2)?;
        let rate = extended(comm.get(8..18)?)?;
        let seconds = (rate > 0.0).then(|| f64::from(frames) / rate);
        Some(Some(seconds?))
    })
}

/// An IEEE 754 80-bit extended float (AIFF's sample rate).
fn extended(b: &[u8]) -> Option<f64> {
    let head = u16::from_be_bytes(b.get(..2)?.try_into().ok()?);
    let mantissa = be64(b, 2)?;
    let exponent = i32::from(head & 0x7fff)
        .checked_sub(16383)?
        .checked_sub(63)?;
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    let value = mantissa as f64 * 2f64.powi(exponent);
    Some(if head & 0x8000 == 0 { value } else { -value })
}

fn flac(file: &mut File) -> Option<f64> {
    let b = read_at(file, 0, 42)?;
    if b.get(..4)? != b"fLaC" || b.get(4).map(|t| t & 0x7f) != Some(0) {
        return None;
    }
    // STREAMINFO's body starts at 8: its bytes 10..18 are the sample rate
    // (20 bits), channels (3), bits a sample (5) and samples (36).
    let packed = be64(&b, 18)?;
    let rate = (packed >> 44) & 0xf_ffff;
    let samples = packed & 0xf_ffff_ffff;
    over(samples, rate)
}

fn ogg(file: &mut File) -> Option<f64> {
    let first = read_at(file, 0, 128)?;
    if first.get(..4)? != b"OggS" {
        return None;
    }
    let segments = usize::from(*first.get(26)?);
    let packet = first.get(27_usize.checked_add(segments)?..)?;
    let (rate, skip) = if packet.get(..7)? == b"\x01vorbis" {
        (u64::from(le32(packet, 12)?), 0)
    } else if packet.get(..8)? == b"OpusHead" {
        let pre_skip = u16::from_le_bytes(packet.get(10..12)?.try_into().ok()?);
        (48_000, u64::from(pre_skip))
    } else {
        return None;
    };
    // The last page's granule: the samples played to its end.
    let len = file.metadata().ok()?.len();
    let tail = read_at(file, len.saturating_sub(65_536), 65_536)?;
    let at = tail.windows(4).rposition(|w| w == b"OggS")?;
    let granule = tail.get(at.checked_add(6)?..at.checked_add(14)?)?;
    let granule = u64::from_le_bytes(granule.try_into().ok()?);
    over(granule.checked_sub(skip)?, rate)
}

/// An MPEG Layer III frame header: (bit rate in kbit/s, sample rate,
/// samples a frame, its side information's length).
fn mp3_frame(h: u32) -> Option<(u64, u64, u64, usize)> {
    if h >> 21 != 0x7ff || (h >> 17) & 3 != 1 {
        return None;
    }
    // 3: MPEG-1, 2: MPEG-2, 0: MPEG-2.5.
    let version = (h >> 19) & 3;
    let mono = (h >> 6) & 3 == 3;
    let bitrates: [u64; 15] = if version == 3 {
        [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ]
    } else {
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
    };
    let kbps = *bitrates.get(usize::try_from((h >> 12) & 0xf).ok()?)?;
    let base: u64 = *[44_100, 48_000, 32_000].get(usize::try_from((h >> 10) & 3).ok()?)?;
    let (rate, samples, side) = match version {
        3 => (base, 1152, if mono { 17 } else { 32 }),
        2 => (base / 2, 576, if mono { 9 } else { 17 }),
        0 => (base / 4, 576, if mono { 9 } else { 17 }),
        _ => return None,
    };
    (kbps > 0).then_some((kbps, rate, samples, side))
}

fn mp3(file: &mut File) -> Option<f64> {
    let len = file.metadata().ok()?.len();
    let id3 = read_at(file, 0, 10)?;
    // An ID3v2 tag first: its size is four 7-bit bytes, after a 10-byte
    // header (and a 10-byte footer when its flag says so).
    let start: u64 = if id3.get(..3)? == b"ID3" {
        let size = id3
            .get(6..10)?
            .iter()
            .fold(0_u64, |n, b| (n << 7) | u64::from(b & 0x7f));
        let footer = if id3.get(5)? & 0x10 == 0 { 0 } else { 10 };
        size.checked_add(10)?.checked_add(footer)?
    } else {
        0
    };
    let head = read_at(file, start, 4096)?;
    let at = head
        .windows(4)
        .position(|w| be32(w, 0).and_then(mp3_frame).is_some())?;
    let (kbps, rate, samples, side) = mp3_frame(be32(&head, at)?)?;
    // A Xing/Info header (an encoder's first frame) counts the frames.
    let xing = at.checked_add(4)?.checked_add(side)?;
    if matches!(
        head.get(xing..xing.checked_add(4)?),
        Some(b"Xing" | b"Info")
    ) && be32(&head, xing.checked_add(4)?)? & 1 == 1
    {
        let frames = u64::from(be32(&head, xing.checked_add(8)?)?);
        return over(frames.checked_mul(samples)?, rate);
    }
    // Otherwise a constant bit rate over what follows the tag.
    let audio = len
        .checked_sub(start)?
        .checked_sub(u64::try_from(at).ok()?)?;
    over(audio.checked_mul(8)?, kbps.checked_mul(1000)?)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    fn put(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// 8 kHz 16-bit mono WAV, 3 s of it, `extra` chunks before its data.
    fn wav_with(extra: &[u8]) -> Vec<u8> {
        let data = 48_000_u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&0_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(extra);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        wav.resize(wav.len() + 48_000, 0);
        wav
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
        assert_eq!(length(&put(dir.path(), "a.wav", &wav_with(&[]))), Some(3.0));
        // A chunk of odd size, then its pad byte, before the data.
        let odd = [b'L', b'I', b'S', b'T', 3, 0, 0, 0, b'a', b'b', b'c', 0];
        assert_eq!(
            length(&put(dir.path(), "b.wav", &wav_with(&odd))),
            Some(3.0)
        );
        assert_eq!(length(&put(dir.path(), "a.txt", b"hello")), None);
        assert_eq!(
            length(&put(dir.path(), "b.mp4", b"not an mp4 at all")),
            None
        );
    }

    #[test]
    fn an_aiff_and_a_flac_say_how_long_they_run() {
        let dir = tempfile::tempdir().unwrap();
        // COMM: 1 channel, 132 300 frames, 16 bits, 44 100 Hz: 3 s.
        let mut aiff = Vec::new();
        aiff.extend_from_slice(b"FORM\0\0\0\0AIFF");
        aiff.extend_from_slice(b"COMM");
        aiff.extend_from_slice(&18_u32.to_be_bytes());
        aiff.extend_from_slice(&1_u16.to_be_bytes());
        aiff.extend_from_slice(&132_300_u32.to_be_bytes());
        aiff.extend_from_slice(&16_u16.to_be_bytes());
        // 44 100 as an 80-bit extended float.
        aiff.extend_from_slice(&[0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
        assert_eq!(length(&put(dir.path(), "a.aiff", &aiff)), Some(3.0));
        // STREAMINFO: 48 kHz, 2 channels, 16 bits, 240 000 samples: 5 s.
        let mut flac = b"fLaC".to_vec();
        flac.extend_from_slice(&[0, 0, 0, 34]);
        flac.extend_from_slice(&[0; 10]);
        let packed: u64 = (0xbb80 << 44) | (1 << 41) | (0xf << 36) | 0x3_a980;
        flac.extend_from_slice(&packed.to_be_bytes());
        flac.extend_from_slice(&[0; 16]);
        assert_eq!(length(&put(dir.path(), "a.flac", &flac)), Some(5.0));
    }

    /// An Ogg page: its header (`granule`), one segment of `body`.
    fn ogg_page(granule: u64, body: &[u8]) -> Vec<u8> {
        let mut page = b"OggS\0\0".to_vec();
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&[0; 12]);
        page.push(1);
        page.push(u8::try_from(body.len()).unwrap());
        page.extend_from_slice(body);
        page
    }

    #[test]
    fn an_ogg_vorbis_and_an_opus_say_how_long_they_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut vorbis_id = b"\x01vorbis".to_vec();
        vorbis_id.extend_from_slice(&0_u32.to_le_bytes());
        vorbis_id.push(2);
        vorbis_id.extend_from_slice(&44_100_u32.to_le_bytes());
        vorbis_id.extend_from_slice(&[0; 15]);
        let mut ogg = ogg_page(0, &vorbis_id);
        ogg.extend(vec![0; 1000]);
        ogg.extend(ogg_page(441_000, b"audio"));
        assert_eq!(length(&put(dir.path(), "a.ogg", &ogg)), Some(10.0));
        // Opus: 48 kHz, its pre-skip taken off.
        let mut head = b"OpusHead\x01\x02".to_vec();
        head.extend_from_slice(&312_u16.to_le_bytes());
        head.extend_from_slice(&[0; 7]);
        let mut opus = ogg_page(0, &head);
        opus.extend(ogg_page(96_312, b"audio"));
        assert_eq!(length(&put(dir.path(), "a.opus", &opus)), Some(2.0));
    }

    #[test]
    fn an_mp3_says_how_long_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        // MPEG-1 Layer III, 128 kbit/s, 44.1 kHz, stereo.
        let frame: u32 = 0xfffb_9000;
        // Constant bit rate after an ID3v2 tag of 100 bytes: 16 000
        // bytes a second, 2 s of them.
        let mut cbr = b"ID3\x04\0\0\0\0\0\x64".to_vec();
        cbr.extend(vec![0; 100]);
        cbr.extend_from_slice(&frame.to_be_bytes());
        cbr.extend(vec![0; 32_000 - 4]);
        assert_eq!(length(&put(dir.path(), "a.mp3", &cbr)), Some(2.0));
        // A Xing header counting 1000 frames of 1152 samples at 44.1 kHz.
        let mut vbr = frame.to_be_bytes().to_vec();
        vbr.extend(vec![0; 32]);
        vbr.extend_from_slice(b"Xing");
        vbr.extend_from_slice(&1_u32.to_be_bytes());
        vbr.extend_from_slice(&1000_u32.to_be_bytes());
        vbr.extend(vec![0; 400]);
        let want = 1000.0 * 1152.0 / 44_100.0;
        assert!((length(&put(dir.path(), "b.mp3", &vbr)).unwrap() - want).abs() < 1e-9);
    }
}

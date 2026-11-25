//! The ways Daslight packs bytes into an XML attribute.
//!
//! - **Prefixed**: `uint32be(length) + zlib stream`, the stream finished
//!   (adler32 trailer). Base64 for `PATCHS DATA`; hex for
//!   `TOUCH_DOCK_MANAGER`.
//! - **Raw**: a zlib stream with no length, ended by a sync flush
//!   (`00 00 ff ff`) and no trailer. Hex for a fixture's `OUTMODE` and
//!   `FLAG`; base64 for a scene's `FIXTUREDATA DATA`.
//!
//! Writing copies the desk's own form, so the desk reads what it expects.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

use crate::Error;

/// Which text encoding an attribute uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Base64,
    Hex,
}

/// Decodes a prefixed payload, in either text encoding.
///
/// # Errors
/// The text is not one of the encodings, or the length does not match.
pub fn unpack_prefixed(text: &str) -> Result<Vec<u8>, Error> {
    let raw = decode_text(text)?;
    let (head, body) = raw
        .split_at_checked(4)
        .ok_or(Error::Container("shorter than its length prefix"))?;
    let declared = u32::from_be_bytes(
        head.try_into()
            .map_err(|_| Error::Container("bad length prefix"))?,
    );
    let out = inflate(body)?;
    if u32::try_from(out.len()).ok() != Some(declared) {
        return Err(Error::Container("length prefix does not match the payload"));
    }
    Ok(out)
}

/// Encodes a prefixed payload.
///
/// # Errors
/// The payload is larger than 4 GiB.
pub fn pack_prefixed(body: &[u8], encoding: Encoding) -> Result<String, Error> {
    let len = u32::try_from(body.len()).map_err(|_| Error::Container("payload over 4 GiB"))?;
    let mut raw = len.to_be_bytes().to_vec();
    raw.extend(deflate(body, true)?);
    Ok(encode_text(&raw, encoding))
}

/// Decodes a raw (sync-flushed) payload.
///
/// # Errors
/// The text is not one of the encodings, or the stream is corrupt.
pub fn unpack_raw(text: &str) -> Result<Vec<u8>, Error> {
    inflate(&decode_text(text)?)
}

/// Encodes a raw payload the way the desk does: sync flush, no trailer.
///
/// # Errors
/// Compression failed.
pub fn pack_raw(body: &[u8], encoding: Encoding) -> Result<String, Error> {
    Ok(encode_text(&deflate(body, false)?, encoding))
}

fn decode_text(text: &str) -> Result<Vec<u8>, Error> {
    let s: String = text.split_whitespace().collect();
    // Hex first: the stricter grammar, so a hex payload is never read as
    // base64 by accident.
    if !s.is_empty() && s.len().is_multiple_of(2) && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return hex_decode(&s);
    }
    STANDARD
        .decode(s.as_bytes())
        .map_err(|_| Error::Container("neither hex nor base64"))
}

fn encode_text(raw: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Base64 => STANDARD.encode(raw),
        Encoding::Hex => {
            use std::fmt::Write as _;
            raw.iter().fold(
                String::with_capacity(raw.len().saturating_mul(2)),
                |mut s, b| {
                    let _ = write!(s, "{b:02x}");
                    s
                },
            )
        }
    }
}

fn hex_decode(s: &str) -> Result<Vec<u8>, Error> {
    s.as_bytes()
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|p| u8::from_str_radix(p, 16).ok())
                .ok_or(Error::Container("bad hex"))
        })
        .collect()
}

/// Inflates a zlib stream whether or not it was finished.
fn inflate(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut d = Decompress::new(true);
    let mut out = Vec::with_capacity(input.len().saturating_mul(4).max(64));
    loop {
        let before_in = d.total_in();
        let before_out = d.total_out();
        let consumed =
            usize::try_from(before_in).map_err(|_| Error::Container("stream too large"))?;
        let rest = input.get(consumed..).unwrap_or_default();
        if out.capacity() == out.len() {
            out.reserve(out.len().max(64));
        }
        let status = d
            .decompress_vec(rest, &mut out, FlushDecompress::Sync)
            .map_err(|_| Error::Container("corrupt zlib stream"))?;
        if status == Status::StreamEnd {
            return Ok(out);
        }
        let progressed = d.total_in() != before_in || d.total_out() != before_out;
        if !progressed && out.len() < out.capacity() {
            // Input exhausted: a sync-flushed stream ends here.
            return Ok(out);
        }
    }
}

fn deflate(input: &[u8], finish: bool) -> Result<Vec<u8>, Error> {
    let mut c = Compress::new(Compression::default(), true);
    let mut out = Vec::with_capacity(input.len().saturating_add(64));
    let flush = if finish {
        FlushCompress::Finish
    } else {
        FlushCompress::Sync
    };
    loop {
        let consumed =
            usize::try_from(c.total_in()).map_err(|_| Error::Container("stream too large"))?;
        let rest = input.get(consumed..).unwrap_or_default();
        if out.capacity() == out.len() {
            out.reserve(out.len().max(64));
        }
        let status = c
            .compress_vec(rest, &mut out, flush)
            .map_err(|_| Error::Container("compression failed"))?;
        let all_in = usize::try_from(c.total_in()).ok() == Some(input.len());
        let room_left = out.len() < out.capacity();
        match status {
            Status::StreamEnd => return Ok(out),
            Status::Ok | Status::BufError if !finish && all_in && room_left => return Ok(out),
            Status::Ok | Status::BufError => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_matches_the_desks_bytes() {
        // A RockStrip's FLAG from a real showfile: R G B fade, Dimmer HTP.
        let flag = unpack_raw("789c6a686838c0c0c000000000ffff").unwrap();
        assert_eq!(flag, [0x80, 0x80, 0x80, 0xc0, 0, 0, 0]);
        let again = unpack_raw(&pack_raw(&flag, Encoding::Hex).unwrap()).unwrap();
        assert_eq!(again, flag);
    }

    #[test]
    fn prefixed_round_trips() {
        let body = b"<PATCH NBFIXTURE=\"0\"/>".repeat(40);
        for enc in [Encoding::Base64, Encoding::Hex] {
            assert_eq!(
                unpack_prefixed(&pack_prefixed(&body, enc).unwrap()).unwrap(),
                body
            );
        }
    }
}

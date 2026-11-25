//! Safe helpers for REAPER APIs that write into caller-provided byte buffers.

/// Call an FFI function that writes a NUL-terminated string into a fixed-size
/// buffer. Returns `None` if the callback returns `false` (indicating failure).
///
/// The callback receives `(buf_ptr: *mut i8, buf_len: i32)` and should return
/// `true` on success.
pub fn with_string_buffer<F>(size: usize, f: F) -> Option<String>
where
    F: FnOnce(*mut i8, i32) -> bool,
{
    let mut buf = vec![0u8; size];
    if !f(buf.as_mut_ptr() as *mut i8, size as i32) {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..len]).into_owned())
}

/// Like [`with_string_buffer`] but uses the integer return value as the
/// success indicator (> 0 means success).
pub fn with_string_buffer_i32<F>(size: usize, f: F) -> Option<String>
where
    F: FnOnce(*mut i8, i32) -> i32,
{
    let mut buf = vec![0u8; size];
    let result = f(buf.as_mut_ptr() as *mut i8, size as i32);
    if result <= 0 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..len]).into_owned())
}

/// Like [`with_string_buffer_i32`], but grows the buffer and retries when the
/// value may not have fitted.
///
/// REAPER's `...OutNeedBig` APIs give the caller no "would not fit" signal: a
/// value longer than the buffer is written truncated and reports success like
/// any other. The only observable symptom is that the result fills the buffer
/// exactly, so that is the retry condition. A value that happens to be exactly
/// `size - 1` bytes costs one redundant call and returns the same answer.
///
/// Returns `None` at `max` rather than a truncated string: for a caller parsing
/// the result, silently-short data is worse than no data.
pub fn with_growing_string_buffer_i32<F>(initial: usize, max: usize, mut f: F) -> Option<String>
where
    F: FnMut(*mut i8, i32) -> i32,
{
    let mut size = initial.clamp(1, max);
    loop {
        let got = with_string_buffer_i32(size, &mut f)?;
        // A result shorter than the buffer cannot have been truncated.
        if got.len() + 1 < size {
            return Some(got);
        }
        if size >= max {
            return None;
        }
        size = (size * 2).min(max);
    }
}

/// Read a NUL-terminated string from a raw byte buffer (e.g. stack-allocated).
pub fn string_from_buffer(buf: &[u8]) -> String {
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

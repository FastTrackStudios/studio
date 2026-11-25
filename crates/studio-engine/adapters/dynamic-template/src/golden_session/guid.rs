//! GUIDs derived from a node's path, so a re-run of the builder is
//! byte-identical to the last one.
//!
//! REAPER wants `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`. Two FNV-1a
//! passes over the path with different seeds give the 128 bits; FNV is
//! spelled out here rather than taken from `std`'s hasher because the
//! standard hasher's algorithm is not a stable promise across toolchains
//! and the committed fixture must not change when the compiler does.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

/// The 64-bit hash of a path — what [`guid`] is made of, and what the
/// deterministic item patterns are seeded with.
#[must_use]
pub fn hash(path: &str) -> u64 {
    fnv1a(FNV_OFFSET, path.as_bytes())
}

/// The GUID for a path such as `track:Drum Kit/Kick/Sum/In` or
/// `item:Drum Kit/Kick/Sum/In/3`. Distinct paths give distinct GUIDs;
/// the same path always gives the same one.
#[must_use]
pub fn guid(path: &str) -> String {
    let hi = hash(path);
    // A second pass seeded from the first, so the two halves are not
    // the same 64 bits twice.
    let lo = fnv1a(hi ^ FNV_OFFSET.rotate_left(17), path.as_bytes());
    let hex = format!("{hi:016X}{lo:016X}");
    let mut chars = hex.chars();
    let mut take = |n: usize| -> String { chars.by_ref().take(n).collect() };
    format!(
        "{{{}-{}-{}-{}-{}}}",
        take(8),
        take(4),
        take(4),
        take(4),
        take(12)
    )
}

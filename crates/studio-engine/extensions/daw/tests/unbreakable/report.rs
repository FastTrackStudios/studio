//! What a run leaves behind: its whole record as JSON in `target/`
//! (`<profile dir>/unbreakable/<song>-<unix>.json` and `latest-<song>.json`),
//! and for the real song one line appended to `docs/perf/unbreakable.jsonl`,
//! so the trend is in git.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

/// Where this test binary's build put things: `target/<triple>/<profile>/`
/// (the binary is in its `deps/`).
#[must_use]
pub fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("the test's own path");
    exe.parent()
        .and_then(Path::parent)
        .map_or_else(std::env::temp_dir, Path::to_path_buf)
}

/// The checkout this test was built from.
#[must_use]
pub fn checkout() -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The commit, the branch, and whether the tree had changes.
#[must_use]
pub fn commit() -> Value {
    json!({
        "sha": git(&["rev-parse", "--short=12", "HEAD"]),
        "branch": git(&["rev-parse", "--abbrev-ref", "HEAD"]),
        "dirty": git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty()),
    })
}

/// The machine, and how busy it was as the run started (the handoffs'
/// numbers moved with the load average).
#[must_use]
pub fn machine() -> Value {
    let host = std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|h| h.trim().to_owned())
        .filter(|h| !h.is_empty())
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        });
    let load = std::fs::read_to_string("/proc/loadavg").ok().and_then(|l| {
        l.split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
    });
    json!({
        "host": host,
        "cpus": std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        "load_1m": load,
    })
}

/// Now, as `2026-11-25T18:20:47Z`, and as Unix seconds.
#[must_use]
pub fn now() -> (String, u64) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (utc(secs), secs)
}

/// Unix seconds as an ISO 8601 UTC time (days from the civil calendar
/// algorithm, no time zone library).
#[must_use]
pub fn utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Write `record` for `song` into `target/`: the file written.
///
/// # Errors
///
/// The folder or file could not be written.
pub fn write_record(song: &str, unix: u64, record: &Value) -> Result<PathBuf, String> {
    let dir = profile_dir().join("unbreakable");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{song}-{unix}.json"));
    std::fs::write(&file, &text).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("latest-{song}.json")), &text).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Append `line` (one JSON object) to `docs/perf/unbreakable.jsonl` of
/// the checkout: the file.
///
/// # Errors
///
/// No checkout, or the file could not be written.
pub fn append_trend(line: &Value) -> Result<PathBuf, String> {
    let file = checkout()
        .ok_or("not in a git checkout")?
        .join("docs")
        .join("perf")
        .join("unbreakable.jsonl");
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .map_err(|e| e.to_string())?;
    writeln!(out, "{line}").map_err(|e| e.to_string())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    #[test]
    fn unix_seconds_read_as_a_utc_date() {
        assert_eq!(super::utc(0), "1970-01-01T00:00:00Z");
        // 2026-11-25 18:20:47 UTC.
        assert_eq!(super::utc(1_795_630_847), "2026-11-25T18:20:47Z");
        assert_eq!(super::utc(951_782_400), "2000-02-29T00:00:00Z");
    }
}

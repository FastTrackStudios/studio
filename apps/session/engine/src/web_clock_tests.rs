//! The page's clock: Session's engine and window run in a browser (`session-daw-web`),
//! where `std::time::Instant::now()` and `SystemTime::now()` panic
//! ("time not implemented on this platform") — and a panic in the open
//! path leaves the page on "Bringing the song in…" for good (#235). The
//! library reads `web_time`'s clock, which is std's natively and the
//! browser's on wasm32. The command-line tools under `src/bin` never run
//! in a browser, so they keep std's.

use std::path::Path;

/// `std::time` items that read the clock (or are the clock's own type).
const CLOCKS: [&str; 3] = ["Instant", "SystemTime::now", "UNIX_EPOCH"];

/// Every `std::time` clock a line of code names: the `std::time::Instant`
/// paths, and the `use std::time::{…, Instant}` groups. A file's mtime
/// (`std::time::SystemTime` as a type) is std's and fine.
fn std_clocks(line: &str) -> Vec<&'static str> {
    let code = line.split("//").next().unwrap_or("");
    let mut found = Vec::new();
    let mut rest = code;
    while let Some(at) = rest.find("std::time::") {
        let after = &rest[at + "std::time::".len()..];
        let named = if let Some(group) = after.strip_prefix('{') {
            let group = group.split('}').next().unwrap_or("");
            group.split(',').map(str::trim).collect::<Vec<_>>()
        } else {
            vec![after]
        };
        for clock in CLOCKS {
            if named.iter().any(|n| {
                n.strip_prefix(clock).is_some_and(|tail| {
                    !tail.starts_with(|c: char| c.is_alphanumeric() || c == '_')
                })
            }) && !found.contains(&clock)
            {
                found.push(clock);
            }
        }
        rest = after;
    }
    found
}

fn walk(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "bin") {
                continue;
            }
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            // This file names the clocks it looks for.
            && !path.ends_with("web_clock_tests.rs")
        {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (n, line) in text.lines().enumerate() {
                let clocks = std_clocks(line);
                if !clocks.is_empty() {
                    out.push(format!("{}:{}: {clocks:?}", path.display(), n + 1));
                }
            }
        }
    }
}

#[test]
fn the_library_reads_a_clock_the_browser_has() {
    // The engine and the window both run in the page (`session-daw-web`).
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    for src in [here.join("src"), here.join("../ui/src")] {
        walk(&src, &mut found);
    }
    assert!(
        found.is_empty(),
        "std's clock panics in a browser; use web_time::Instant / SystemTime:\n{}",
        found.join("\n"),
    );
}

#[test]
fn the_scan_sees_each_way_of_naming_the_clock() {
    assert_eq!(
        std_clocks("let t = std::time::Instant::now();"),
        ["Instant"]
    );
    assert_eq!(
        std_clocks("use std::time::{Duration, Instant, SystemTime};"),
        ["Instant"]
    );
    assert_eq!(
        std_clocks("std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)"),
        ["SystemTime::now", "UNIX_EPOCH"]
    );
    // A file's mtime, a duration, a comment, and web_time's clock are fine.
    assert!(std_clocks("use std::time::{Duration, SystemTime};").is_empty());
    assert!(std_clocks("let d = std::time::Duration::from_millis(5);").is_empty());
    assert!(std_clocks("// no std::time::Instant here").is_empty());
    assert!(std_clocks("let t = web_time::Instant::now();").is_empty());
    assert!(std_clocks("x: std::time::InstantLike").is_empty());
}

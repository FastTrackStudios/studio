//! `omni_zonemap` — write a zone-mode styx for every soundsource an Omnisphere
//! library presents, beside audio that is already extracted.
//!
//! Walks the `.db`s under a STEAM soundsources dir (e.g.
//! `…/STEAM/Omnisphere/Soundsources/Factory`), reads each one's `.zmap`s
//! ([`synth::omni_import::steam`]) and writes one styx per soundsource into
//! the extraction root at the place [`SoundsourceIndex`] finds it by name:
//!
//! - `<rel>/<stem>/library.styx` — one soundsource, named after its `.db`;
//! - `<rel>/<stem>/<name>.styx` — several, or a name that is not the `.db`'s;
//! - `<rel>/<name>.styx` — a one-sound `.db` (its audio is `<rel>/<stem>.flac`).
//!
//! Dry run unless `--write`. `--prune` removes a `<stem>/library.styx` the
//! layout no longer wants (it would hide the named styx beside it from the
//! index). Every zone's file is checked against the extraction.
//!
//! ```text
//! cargo run -p synth --release --example omni_zonemap -- <steam_soundsources> <extraction_root> [--write] [--prune]
//! ```
//!
//! [`SoundsourceIndex`]: synth::omni_import::SoundsourceIndex

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use synth::omni_import::steam::{SteamDb, ZmapSoundsource, soundsources, write_styx};

const VENDOR: &str = "Spectrasonics (Omnisphere)";

#[derive(Default)]
struct Report {
    dbs: usize,
    failed: Vec<(PathBuf, String)>,
    soundsources: usize,
    empty: Vec<String>,
    zones: usize,
    unresolved_zones: usize,
    missing_files: Vec<(String, String)>,
    renamed: usize,
    multi_layer: usize,
    written: usize,
    pruned: usize,
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "db") {
            out.push(p);
        }
    }
}

/// Where `ss` goes: beside the audio its zones name.
fn placement(
    rel_dir: &Path,
    stem: &str,
    ss: &ZmapSoundsource,
    siblings: usize,
    root: &Path,
) -> PathBuf {
    let one_sound = root.join(rel_dir).join(format!("{stem}.flac")).exists()
        || root.join(rel_dir).join(format!("{stem}.wav")).exists();
    if one_sound && !root.join(rel_dir).join(stem).is_dir() {
        return root.join(rel_dir).join(format!("{}.styx", ss.name));
    }
    let dir = root.join(rel_dir).join(stem);
    if siblings == 1 && ss.name == stem {
        dir.join("library.styx")
    } else {
        dir.join(format!("{}.styx", ss.name))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flags: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| a.starts_with("--"))
        .collect();
    let paths: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [src, root] = paths.as_slice() else {
        return Err(
            "usage: omni_zonemap <steam_soundsources> <extraction_root> [--write] [--prune]".into(),
        );
    };
    let (src, root) = (Path::new(src.as_str()), Path::new(root.as_str()));
    let write = flags.contains(&"--write");
    let prune = flags.contains(&"--prune");

    let mut dbs = Vec::new();
    walk(src, &mut dbs);
    dbs.sort();

    let mut r = Report::default();
    let mut layers: BTreeMap<usize, usize> = BTreeMap::new();
    for path in &dbs {
        r.dbs += 1;
        let db = match SteamDb::open(path) {
            Ok(db) => db,
            Err(e) => {
                r.failed.push((path.clone(), e));
                continue;
            }
        };
        let rel = path.strip_prefix(src).unwrap_or(path);
        let rel_dir = rel.parent().unwrap_or_else(|| Path::new(""));
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let all = soundsources(&db, &stem);
        let mut wants_library = false;
        for ss in &all {
            r.soundsources += 1;
            r.zones += ss.zones.len();
            r.unresolved_zones += ss.unresolved;
            *layers.entry(ss.layers().len()).or_default() += 1;
            if ss.layers().len() > 1 {
                r.multi_layer += 1;
            }
            if ss.name != stem {
                r.renamed += 1;
            }
            if ss.zones.is_empty() {
                r.empty.push(format!("{}/{}", rel_dir.display(), ss.name));
                continue;
            }
            let out = placement(rel_dir, &stem, ss, all.len(), root);
            wants_library |= out.file_name().is_some_and(|f| f == "library.styx");
            let dir = out.parent().unwrap_or(root);
            for z in &ss.zones {
                if !dir.join(&z.file).exists() {
                    r.missing_files.push((ss.name.clone(), z.file.clone()));
                }
            }
            if write {
                std::fs::create_dir_all(dir)?;
                std::fs::write(&out, write_styx(ss, VENDOR, ""))?;
                r.written += 1;
            }
        }
        let stale = root.join(rel_dir).join(&stem).join("library.styx");
        if prune && write && !wants_library && stale.exists() {
            std::fs::remove_file(&stale)?;
            r.pruned += 1;
        }
    }

    println!("dbs            {} (failed {})", r.dbs, r.failed.len());
    println!(
        "soundsources   {} ({} not named after their .db, {} multi-layer)",
        r.soundsources, r.renamed, r.multi_layer
    );
    println!(
        "zones          {} (unresolved {})",
        r.zones, r.unresolved_zones
    );
    println!("empty          {}", r.empty.len());
    println!("missing files  {}", r.missing_files.len());
    println!("layers/source  {layers:?}");
    if write {
        println!("written        {} styx, pruned {}", r.written, r.pruned);
    }
    for (p, e) in r.failed.iter().take(10) {
        println!("  failed  {}: {e}", p.display());
    }
    for e in r.empty.iter().take(10) {
        println!("  empty   {e}");
    }
    for (ss, f) in r.missing_files.iter().take(10) {
        println!("  missing {ss}: {f}");
    }
    Ok(())
}

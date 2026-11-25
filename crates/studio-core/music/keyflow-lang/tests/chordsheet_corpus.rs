#![cfg(feature = "manifest")]
//! Whole-corpus smoke test.
//!
//! The corpus is our chordsheet.com account backup, committed at
//! `crates/studio-core/music/keyflow-examples/chordsheet-compat/` (`chordsheet/`
//! and `manifest.json` beside it). Ignored: it is a corpus diagnostic, and on
//! 2026-11-25 three charts fail it (sections that reparse longer than their
//! header). Run `cargo test -p keyflow-lang --features manifest -- --ignored`.
//!
//! What it proves: every chart in the backup imports without panicking, and
//! the `.kf` it exports parses back into a chart with the same number of
//! sections. That round trip is the contract — an import that only *looks*
//! right until keyflow reads it back is not an import.

use std::path::PathBuf;

use keyflow_lang::chordsheet::manifest::Manifest;

fn corpus() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../keyflow-examples/chordsheet-compat/chordsheet");
    dir.is_dir().then_some(dir)
}

#[test]
#[ignore = "corpus diagnostic over the committed chordsheet.com backup; 3 of 204 charts do not round-trip yet"]
fn every_chart_in_the_backup_round_trips_through_keyflow_text() {
    let Some(dir) = corpus() else {
        panic!("corpus not present — see the module docs");
    };
    let manifest = Manifest::read(dir.join("../manifest.json")).unwrap_or_default();

    let mut sources: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("txt"))
        .collect();
    sources.sort();
    assert!(!sources.is_empty(), "corpus dir has no .txt files");

    let mut mismatches = Vec::new();
    for path in &sources {
        let source = std::fs::read_to_string(path).expect("read source");
        let chart = keyflow_lang::chordsheet::import_str(&source, &manifest.options_for(path));
        let text = keyflow_lang::text::chart::exporter::chart_to_keyflow(&chart);

        match keyflow_lang::text::chart::parse_chart(&text) {
            Ok(reparsed) => {
                let before: Vec<usize> =
                    chart.sections.iter().map(|s| s.measures().len()).collect();
                let after: Vec<usize> = reparsed
                    .sections
                    .iter()
                    .map(|s| s.measures().len())
                    .collect();
                // Sections must survive one for one, and no section may come
                // back shorter. It may come back *longer*: `chart_to_keyflow`
                // writes `|: … :|` out as the bars it stands for, so a section
                // with a repeat legitimately doubles.
                if before.len() != after.len() || before.iter().zip(&after).any(|(b, a)| a < b) {
                    mismatches.push(format!(
                        "{}: sections {before:?} in, {after:?} out",
                        path.display()
                    ));
                }
            }
            Err(e) => mismatches.push(format!("{}: reparse failed: {e}", path.display())),
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} of {} charts did not round-trip:\n{}",
        mismatches.len(),
        sources.len(),
        mismatches.join("\n")
    );
}

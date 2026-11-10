//! `studio productions`: the same actions, over a vault folder opened
//! with Architect's own vault actions.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

use productions::cli::{Cli, Command};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../adapters")
        .join(rel)
}

fn run(vault: &Path, command: Command) -> String {
    Cli {
        vault: vault.to_path_buf(),
        command,
    }
    .run()
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn scan_list_and_stage_from_a_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let vault = tmp.path().join("vault");
    let session = vault.join("Sessions/Region Test");
    std::fs::create_dir_all(session.join("Audio Files")).unwrap();
    std::fs::copy(
        fixture("protools/tests/fixtures/protools/RegionTest.ptx"),
        session.join("Region Test.ptx"),
    )
    .unwrap();

    let out = run(
        &vault,
        Command::Scan {
            path: None,
            no_bases: false,
        },
    );
    assert!(
        out.starts_with("Region Test — created · writing · Pro Tools"),
        "{out}"
    );
    assert!(out.contains("wrote Productions/By stage.base"), "{out}");
    assert!(vault.join("Projects/Region Test/project.md").exists());

    let out = run(
        &vault,
        Command::Stage {
            project: "Region Test".to_owned(),
            stage: "tracking".to_owned(),
        },
    );
    assert_eq!(out, "Region Test — tracking (was writing)");

    // Each run opens the vault again, as each `studio productions` does.
    let out = run(
        &vault,
        Command::List {
            stage: Some("tracking".to_owned()),
            client: None,
        },
    );
    assert!(
        out.starts_with("Region Test — tracking · Pro Tools"),
        "{out}"
    );
    let out = run(
        &vault,
        Command::Scan {
            path: Some(PathBuf::from("Sessions")),
            no_bases: true,
        },
    );
    assert!(
        out.starts_with("Region Test — unchanged · tracking"),
        "{out}"
    );
}

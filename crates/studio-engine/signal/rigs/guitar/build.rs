//! Lists the shipped library's files for embedding: every capture and IR in
//! `models/`, every frozen Core in `frozen/`, every profile in
//! `default-config/profiles/` (and the same for the bass's
//! `default-config-bass/`) — so what `default-config/ship-library.sh` puts
//! there ships, with no list to keep.
//!
//! The captures and frozen Cores are not in this repository (they are
//! private). They come from `$SIGNAL_RIG_LIBRARY/guitar/{models,frozen}`
//! (and `…/bass/…`) when the app building this crate says where its copy
//! is — Studio does, from its `.cargo/config.toml`, since a crate taken by
//! git has no links of its own — else from `default-config/models` and
//! `default-config/frozen` here. A release build refuses to run without
//! the captures.
//!
//! Each file is copied into `OUT_DIR/embed/<table>/` (a reflink on btrfs and
//! APFS, so no bytes are copied) and the table names it by that path, which
//! never leaves `OUT_DIR` (an `include_bytes!` in an `include!`d file
//! resolves against that file). So the generated text is the same in every
//! checkout and right wherever the target dir is. A path from `OUT_DIR` out
//! to the checkout was neither: kache replays a build script's output by a
//! key that leaves out where the target dir sits, and a run recorded with
//! the target dir outside the checkout (`just test-unbreakable`'s) put
//! `../../../../../../../monorepo/rig-library/…` into an in-checkout
//! `target/`, which named no file.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Every file in `dir` with one of `exts`, by name, at its real path (`models/`
/// can be a link into .assets, `just assets`).
fn listed(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| exts.contains(&e))
                })
                .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    out
}

/// `files` copied into `out_dir/embed/<table>/`: `(file name, its path
/// relative to out_dir)` each.
fn embedded(files: &[PathBuf], out_dir: &Path, table: &str) -> Vec<(String, String)> {
    let dir = out_dir.join("embed").join(table);
    std::fs::create_dir_all(&dir).unwrap();
    files
        .iter()
        .map(|src| {
            let name = src.file_name().unwrap().to_string_lossy().into_owned();
            std::fs::copy(src, dir.join(&name))
                .unwrap_or_else(|e| panic!("copying {} into OUT_DIR: {e}", src.display()));
            (name.clone(), format!("embed/{table}/{name}"))
        })
        .collect()
}

/// One shipped library's tables, named `<prefix>_MODELS`, `_FROZEN`,
/// `_PROFILES` (empty when the directory is not there): the profiles from
/// `root`, the captures and frozen Cores from `captures`.
fn tables(src: &mut String, root: &Path, captures: &Path, out_dir: &Path, prefix: &str) {
    // Only the ones that exist: cargo counts a missing rerun-if-changed path
    // as changed, so the bass's absent `frozen/` re-ran this script — and
    // recompiled guitar and everything above it — on every cargo call. The
    // roots still see one appear.
    for dir in [root, captures] {
        if dir.exists() {
            println!("cargo:rerun-if-changed={}", dir.display());
        }
    }
    for dir in [
        captures.join("models"),
        captures.join("frozen"),
        root.join("profiles"),
    ] {
        if dir.exists() {
            println!("cargo:rerun-if-changed={}", dir.display());
        }
    }
    let bytes = |src: &mut String, name: &str, list: &[(String, String)]| {
        writeln!(src, "const {name}: &[(&str, &[u8])] = &[").unwrap();
        for (file, path) in list {
            writeln!(src, "    ({file:?}, include_bytes!({path:?})),").unwrap();
        }
        writeln!(src, "];").unwrap();
    };
    let models = format!("{prefix}_MODELS");
    let models_list = embedded(
        &listed(&captures.join("models"), &["nam", "wav"]),
        out_dir,
        &models,
    );
    bytes(src, &models, &models_list);
    let frozen = format!("{prefix}_FROZEN");
    let frozen_list = embedded(
        &listed(&captures.join("frozen"), &["nam"]),
        out_dir,
        &frozen,
    );
    bytes(src, &frozen, &frozen_list);
    let profiles = format!("{prefix}_PROFILES");
    writeln!(src, "const {profiles}: &[(&str, &str)] = &[").unwrap();
    for (file, path) in embedded(
        &listed(&root.join("profiles"), &["styx"]),
        out_dir,
        &profiles,
    ) {
        writeln!(src, "    ({file:?}, include_str!({path:?})),").unwrap();
    }
    writeln!(src, "];").unwrap();
}

fn main() {
    // Read at run time, not `env!`: that would put this checkout's path in
    // the build script's own cache key.
    let here = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    // Where the captures and frozen Cores are: the building app's copy
    // (`SIGNAL_RIG_LIBRARY`), else this crate's own directories.
    println!("cargo:rerun-if-env-changed=SIGNAL_RIG_LIBRARY");
    let library = std::env::var_os("SIGNAL_RIG_LIBRARY").map(PathBuf::from);
    let captures = |own: &str, name: &str| {
        library
            .as_ref()
            .map_or_else(|| here.join(own), |dir| dir.join(name))
    };
    let (guitar, bass) = (
        captures("default-config", "guitar"),
        captures("default-config-bass", "bass"),
    );
    // The captures live in the rig-models repo, which is private.
    // A debug build without them is fine (a rig with no captures, tests
    // that need them fail); a release would ship that rig to people.
    // Except where no rig ships: a browser build (the engine is native-only)
    // and a build that says so (`SIGNAL_RIG_NO_CAPTURES`: signal-web's
    // pre-render server, which only renders pages).
    println!("cargo:rerun-if-env-changed=SIGNAL_RIG_NO_CAPTURES");
    let ships_a_rig = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32")
        && std::env::var_os("SIGNAL_RIG_NO_CAPTURES").is_none();
    if ships_a_rig && std::env::var("PROFILE").as_deref() == Ok("release") {
        for dir in [guitar.join("models"), bass.join("models")] {
            assert!(
                !listed(&dir, &["nam", "wav"]).is_empty(),
                "{} is empty: the rig's captures are in the rig-models repo (in Studio, `just assets` links them in) — a release build ships them",
                dir.display(),
            );
        }
    }
    // A file dropped from the library must not stay embedded from a run
    // before.
    let _ = std::fs::remove_dir_all(out_dir.join("embed"));
    let mut src = String::new();
    // The guitar's (`DEFAULT_*`, as they always were) and the bass's.
    tables(
        &mut src,
        &here.join("default-config"),
        &guitar,
        &out_dir,
        "DEFAULT",
    );
    tables(
        &mut src,
        &here.join("default-config-bass"),
        &bass,
        &out_dir,
        "BASS",
    );
    std::fs::write(out_dir.join("default_files.rs"), src).unwrap();
}

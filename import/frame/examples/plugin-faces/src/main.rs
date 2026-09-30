//! `cargo run -p plugin-faces -- OUT_DIR` — write every face + catalogs.
//!
//! Each face is `OUT_DIR/<plugin>/<file>.fm` plus, when it carries
//! textures or fonts, its `<file>.fm.assets/` sidecar (see `document-io`).
//! Node ids are stable (see `face::stable_ids`), so regenerating without a
//! source change rewrites every file byte for byte.

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "examples/plugins".into()));
    let n = plugin_faces::write_all(&out);
    println!("wrote {n} faces to {}", out.display());
}

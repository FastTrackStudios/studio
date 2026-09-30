//! `cargo run -p plugin-faces -- OUT_DIR` — write every face + catalogs.
//!
//! Each face is `OUT_DIR/<plugin>/<file>.fm` plus, when it carries
//! textures or fonts, its `<file>.fm.assets/` sidecar (see `document-io`).

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "examples/plugins".into()));
    let faces = plugin_faces::all();
    for f in &faces {
        let path = out.join(f.plugin).join(format!("{}.fm", f.file));
        document_io::save_surface(&path, &f.scene).unwrap_or_else(|e| panic!("write face: {e}"));
    }
    for (plugin, catalog) in plugin_faces::catalogs(&faces) {
        let path = out.join(plugin).join(format!("{plugin}.catalog.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&catalog).expect("json")).expect("write catalog");
    }
    println!("wrote {} faces to {}", faces.len(), out.display());
}

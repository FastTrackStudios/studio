//! `cargo run -p plugin-faces -- OUT_DIR` — write every face + catalogs.

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "examples/plugins".into()));
    let faces = plugin_faces::all();
    for f in &faces {
        let dir = out.join(f.plugin);
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join(format!("{}.fm", f.file)), document_text::print(&f.scene)).expect("write face");
    }
    for (plugin, catalog) in plugin_faces::catalogs(&faces) {
        let path = out.join(plugin).join(format!("{plugin}.catalog.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&catalog).expect("json")).expect("write catalog");
    }
    println!("wrote {} faces to {}", faces.len(), out.display());
}

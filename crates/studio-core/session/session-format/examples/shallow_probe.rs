//! What a shallow snapshot would make of a song: its size, how long it
//! takes to import, and what one small edit costs to import into it, beside
//! the same for the song as it is stored (its full history).
//!
//! `cargo run --release -p session-format --example shallow_probe -- <Song.session>`
//!
//! Reads the file only; writes nothing to it.

use std::time::Instant;

use loro::{ExportMode, LoroDoc};
use session_format::store::{MAIN, Store};

/// The map the edit touches: the song's items, its largest.
const ITEMS: &str = "items";

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// A small edit to `base`'s items map, made on a replica of `snapshot`: the
/// update it exports, as a save appends one.
fn an_edit(snapshot: &[u8], base: &LoroDoc) -> Vec<u8> {
    let other = LoroDoc::new();
    other.import(snapshot).expect("the replica imports");
    other
        .get_map(ITEMS)
        .insert("shallow_probe", 1)
        .expect("the edit");
    other.commit();
    other
        .export(ExportMode::updates(&base.oplog_vv()))
        .expect("the edit exports")
}

fn main() {
    let path = std::env::args().nth(1).expect("a .session file");
    let store = Store::open(std::path::Path::new(&path)).expect("the store opens");

    let t = Instant::now();
    let full = store.load(MAIN).expect("main loads");
    println!("load as stored (full history):   {:>9.1} ms", ms(t));

    let frontiers = full.oplog_frontiers();
    let t = Instant::now();
    let shallow = full
        .export(ExportMode::shallow_snapshot(&frontiers))
        .expect("a shallow snapshot");
    println!(
        "export shallow at latest:        {:>9.1} ms  {:>7.1} MB",
        ms(t),
        shallow.len() as f64 / 1e6
    );
    let t = Instant::now();
    let packed = zstd::encode_all(shallow.as_slice(), 3).expect("packs");
    println!(
        "pack it (zstd 3):                {:>9.1} ms  {:>7.1} MB",
        ms(t),
        packed.len() as f64 / 1e6
    );

    let t = Instant::now();
    let loaded = LoroDoc::new();
    loaded
        .import(&shallow)
        .expect("the shallow snapshot imports");
    println!("import shallow:                  {:>9.1} ms", ms(t));
    let t = Instant::now();
    let _ = loaded.get_deep_value();
    println!("read it whole:                   {:>9.1} ms", ms(t));

    // One small edit, imported into each: what a save past the snapshot
    // costs the next open.
    let edit = an_edit(&shallow, &loaded);
    println!("the edit:                        {:>9} bytes", edit.len());
    let t = Instant::now();
    loaded.import(&edit).expect("the edit imports (shallow)");
    println!("import the edit (shallow):       {:>9.1} ms", ms(t));
    let t = Instant::now();
    full.import(&edit).expect("the edit imports (full)");
    println!("import the edit (full history):  {:>9.1} ms", ms(t));

    assert_eq!(
        loaded.get_map(ITEMS).len(),
        full.get_map(ITEMS).len(),
        "both hold the same items"
    );
}

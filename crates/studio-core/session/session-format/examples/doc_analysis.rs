//! Where a song's operations are: Loro's `analyze()` of its main branch,
//! the containers grouped by their place in the song (a track's FX, an
//! item's takes), with how many ops and how many bytes of state each
//! group holds.
//!
//! `cargo run --release -p session-format --example doc_analysis -- <Song.session> [top]`
//!
//! Reads the file only; writes nothing to it.

use std::collections::BTreeMap;
use std::time::Instant;

use loro::{ContainerID, Index, LoroDoc};
use session_format::store::{MAIN, Store};

/// A key that names one thing of many (a GUID, a number): folded to `*`,
/// so every track's FX are one group.
fn is_instance(key: &str) -> bool {
    key.starts_with('{')
        || key.len() >= 16 && key.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        || key.chars().all(|c| c.is_ascii_digit())
}

/// The container's place, with instances folded: `tracks/*/fx/*/state`.
fn shape(doc: &LoroDoc, id: &ContainerID) -> String {
    let Some(path) = doc.get_path_to_container(id) else {
        return format!("(unreachable {})", id.container_type());
    };
    let mut parts = Vec::new();
    for (container, index) in path {
        match index {
            Index::Key(key) if is_instance(&key) => parts.push("*".to_owned()),
            Index::Key(key) => parts.push(key.to_string()),
            Index::Seq(_) => parts.push("[]".to_owned()),
            Index::Node(_) => parts.push("<node>".to_owned()),
        }
        if parts.len() == 1 && container.is_root() {
            parts[0] = format!("{}:{}", container.name(), parts[0]);
        }
    }
    format!("{} ({})", parts.join("/"), id.container_type())
}

#[derive(Default)]
struct Group {
    containers: u64,
    dropped: u64,
    ops: u64,
    bytes: u64,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a .session file");
    let top: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(30);
    let store = Store::open(std::path::Path::new(&path)).expect("the store opens");

    let t = Instant::now();
    let doc = store.load(MAIN).expect("main loads");
    println!("loaded in {:.1} s", t.elapsed().as_secs_f64());
    let vv = doc.oplog_vv();
    println!(
        "peers that wrote it: {}; ops by counter: {}",
        vv.len(),
        vv.iter().map(|(_, c)| i64::from(*c)).sum::<i64>()
    );

    let t = Instant::now();
    let analysis = doc.analyze();
    println!(
        "analyze: {:.1} s, {} containers",
        t.elapsed().as_secs_f64(),
        analysis.containers.len()
    );

    // What the unreachable containers are: a sample, with their keys.
    let mut shown = 0;
    let mut unreachable_by_keys: BTreeMap<String, u64> = BTreeMap::new();
    for (id, _) in &analysis.containers {
        if doc.get_path_to_container(id).is_some() {
            continue;
        }
        let keys = match doc.get_container(id.clone()) {
            Some(loro::Container::Map(m)) => {
                let mut k: Vec<String> = m.keys().map(|k| k.to_string()).collect();
                k.sort();
                k.truncate(6);
                k.join(",")
            }
            Some(other) => format!("{:?}", other.get_type()),
            None => "?".into(),
        };
        *unreachable_by_keys.entry(keys.clone()).or_default() += 1;
        if shown < 6 {
            println!("unreachable: {id}  keys: {keys}");
            shown += 1;
        }
    }
    if std::env::var_os("CLEAN").is_some() {
        measure_clean(&doc, &analysis);
    }
    let mut kinds: Vec<_> = unreachable_by_keys.into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1));
    for (keys, n) in kinds.iter().take(8) {
        println!("{n:>8} unreachable with keys {keys}");
    }

    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for (id, info) in &analysis.containers {
        let g = groups.entry(shape(&doc, id)).or_default();
        g.containers += 1;
        g.dropped += u64::from(info.dropped);
        g.ops += u64::from(info.ops_num);
        g.bytes += u64::from(info.size);
    }
    let total_ops: u64 = groups.values().map(|g| g.ops).sum();
    let total_bytes: u64 = groups.values().map(|g| g.bytes).sum();
    let mut by_ops: Vec<_> = groups.iter().collect();
    by_ops.sort_by(|a, b| b.1.ops.cmp(&a.1.ops));
    println!(
        "\n{:>10} {:>6} {:>10} {:>6} {:>9} {:>8}  place",
        "ops", "%", "bytes", "%", "conts", "dropped"
    );
    for (shape, g) in by_ops.iter().take(top) {
        println!(
            "{:>10} {:>5.1}% {:>10} {:>5.1}% {:>9} {:>8}  {shape}",
            g.ops,
            100.0 * g.ops as f64 / total_ops.max(1) as f64,
            g.bytes,
            100.0 * g.bytes as f64 / total_bytes.max(1) as f64,
            g.containers,
            g.dropped,
        );
    }
    println!(
        "\n{} groups; {total_ops} ops, {total_bytes} bytes of state in all",
        groups.len()
    );

    // The largest values in the root maps: what a few ops hold so much of.
    let mut big: Vec<(usize, String)> = Vec::new();
    for root in ["project", "items", "take_markers"] {
        let map = doc.get_map(root);
        map.for_each(|key, v| {
            if let loro::ValueOrContainer::Value(v) = v {
                let n = value_bytes(&v);
                if n > 64 * 1024 {
                    big.push((n, format!("{root}/{key} ({})", kind(&v))));
                }
            }
        });
    }
    big.sort_by(|a, b| b.0.cmp(&a.0));
    println!("\nvalues over 64 KiB in the root maps: {}", big.len());
    for (n, what) in big.iter().take(top) {
        println!("{:>12} bytes  {what}", n);
    }

    // The project's extension state by section, and its largest keys.
    if let Some(loro::ValueOrContainer::Value(loro::LoroValue::Map(ext))) =
        doc.get_map("project").get("ext")
    {
        let mut sections: Vec<(usize, usize, &String)> = ext
            .iter()
            .map(|(name, keys)| {
                let n = match keys {
                    loro::LoroValue::Map(m) => m.len(),
                    _ => 0,
                };
                (value_bytes(keys), n, name)
            })
            .collect();
        sections.sort_by(|a, b| b.0.cmp(&a.0));
        println!("\nproject/ext: {} sections", sections.len());
        for (bytes, keys, name) in sections.iter().take(top) {
            println!("{bytes:>12} bytes {keys:>7} keys  {name}");
        }
        // How much of the mix snapshots is plugin state, and how much of
        // that is the same state again.
        if let Some(loro::LoroValue::Map(mix)) = ext.get("fts.mix") {
            let (mut states, mut total) = (std::collections::HashSet::new(), 0_usize);
            let mut unique = 0_usize;
            let mut text = 0_usize;
            for (_, v) in mix.iter() {
                let loro::LoroValue::String(json) = v else {
                    continue;
                };
                text += json.len();
                for piece in json.split("\"state\":\"").skip(1) {
                    let state = piece.split('"').next().unwrap_or("");
                    total += state.len();
                    if states.insert(state.to_owned()) {
                        unique += state.len();
                    }
                }
            }
            println!(
                "fts.mix: {:.1} MB text, {:.1} MB of it plugin state, {:.1} MB of that unique ({} states)",
                text as f64 / 1e6,
                total as f64 / 1e6,
                unique as f64 / 1e6,
                states.len()
            );
        }
    }
}

/// The shallow snapshot before and after the unreachable mergeable maps
/// are cleared, and ext dropped.
fn measure_clean(doc: &LoroDoc, analysis: &loro::DocAnalysis) {
    let shallow = |doc: &LoroDoc| {
        let t = Instant::now();
        let bytes = doc
            .export(loro::ExportMode::shallow_snapshot(&doc.oplog_frontiers()))
            .expect("shallow");
        let export_s = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let back = LoroDoc::new();
        back.import(&bytes).expect("imports");
        let _ = back.get_deep_value();
        (
            bytes.len() as f64 / 1e6,
            export_s,
            t.elapsed().as_secs_f64(),
        )
    };
    let (mb, ex, im) = shallow(doc);
    println!("shallow as it is:        {mb:>8.1} MB  export {ex:>5.1} s  import+read {im:>5.1} s");
    let t = Instant::now();
    let mut n = 0;
    for id in analysis.containers.keys() {
        if doc.get_path_to_container(id).is_some() {
            continue;
        }
        // A mergeable child of a root map, as `session_sync`'s
        // `clear_deleted_entities` takes them.
        let Some((parent, key, _)) = id.parse_mergeable() else {
            continue;
        };
        let loro::ContainerID::Root { name, .. } = &parent else {
            continue;
        };
        if parent.is_mergeable() {
            continue;
        }
        let parent = doc.get_map(name.as_str());
        if parent.get(&key).is_some() {
            continue;
        }
        let Ok(map) = parent.ensure_mergeable_map(&key) else {
            continue;
        };
        if &loro::ContainerTrait::id(&map) != id {
            let _ = parent.delete(&key);
            continue;
        }
        map.clear().expect("cleared");
        parent.delete(&key).expect("deleted again");
        n += 1;
    }
    doc.commit();
    println!(
        "emptied {n} unreachable roots in {:.1} s",
        t.elapsed().as_secs_f64()
    );
    let (mb, ex, im) = shallow(doc);
    println!("shallow, orphans gone:   {mb:>8.1} MB  export {ex:>5.1} s  import+read {im:>5.1} s");
    let project = doc.get_map("project");
    project.delete("ext").expect("ext dropped");
    doc.commit();
    let (mb, ex, im) = shallow(doc);
    println!("shallow, and no ext:     {mb:>8.1} MB  export {ex:>5.1} s  import+read {im:>5.1} s");
    // What is left, by root: each root's state as a snapshot of it alone
    // is not exportable, so its deep value's size stands in.
    let value = doc.get_deep_value();
    if let loro::LoroValue::Map(roots) = value {
        let mut sizes: Vec<(usize, usize, String)> = roots
            .iter()
            .map(|(k, v)| {
                let n = match v {
                    loro::LoroValue::Map(m) => m.len(),
                    loro::LoroValue::List(l) => l.len(),
                    _ => 1,
                };
                (value_bytes(v), n, k.clone())
            })
            .collect();
        sizes.sort_by(|a, b| b.0.cmp(&a.0));
        for (bytes, n, k) in sizes.iter().take(12) {
            println!("{:>10.1} MB {n:>8} entries  {k}", *bytes as f64 / 1e6);
        }
    }
}

fn kind(v: &loro::LoroValue) -> &'static str {
    match v {
        loro::LoroValue::Binary(_) => "binary",
        loro::LoroValue::String(_) => "string",
        loro::LoroValue::List(_) => "list",
        loro::LoroValue::Map(_) => "map",
        _ => "other",
    }
}

/// About how many bytes `v` holds.
fn value_bytes(v: &loro::LoroValue) -> usize {
    match v {
        loro::LoroValue::Binary(b) => b.len(),
        loro::LoroValue::String(s) => s.len(),
        loro::LoroValue::List(l) => l.iter().map(value_bytes).sum::<usize>() + 8,
        loro::LoroValue::Map(m) => {
            m.iter()
                .map(|(k, v)| k.len() + value_bytes(v))
                .sum::<usize>()
                + 8
        }
        _ => 8,
    }
}

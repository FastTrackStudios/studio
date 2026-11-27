use std::collections::BTreeMap;

use loro::ExportMode;

use super::clear_deleted_entities;
use crate::model::{ItemState, SessionModel};
use crate::{ORIGIN_LOCAL, SessionDoc};

fn with_items(n: usize) -> SessionModel {
    let items: BTreeMap<String, ItemState> = (0..n)
        .map(|i| {
            (
                format!("item-{i}"),
                ItemState {
                    track: "kick".into(),
                    position: i as f64,
                    length: 1.0,
                    ..ItemState::default()
                },
            )
        })
        .collect();
    SessionModel {
        items,
        ..SessionModel::default()
    }
}

fn snapshot_len(doc: &SessionDoc) -> usize {
    let loro = doc.loro();
    loro.export(ExportMode::shallow_snapshot(&loro.oplog_frontiers()))
        .expect("a snapshot")
        .len()
}

#[test]
fn a_deleted_item_leaves_none_of_its_fields_in_the_snapshot() {
    let doc = SessionDoc::new();
    doc.write(&with_items(2_000), ORIGIN_LOCAL)
        .expect("written");
    let live = snapshot_len(&doc);
    doc.write(&with_items(0), ORIGIN_LOCAL)
        .expect("all deleted");
    let gone = snapshot_len(&doc);
    // What is left is the fields' tombstones, about half (with the fields
    // kept, nearly all of it).
    assert!(
        gone * 10 < live * 7,
        "the deleted items' fields stay: {gone} bytes of {live}"
    );

    // Made again at its key, an item starts from what is written now.
    doc.write(&with_items(1), ORIGIN_LOCAL).expect("one again");
    assert_eq!(doc.read().items, with_items(1).items);
}

#[test]
fn deleted_items_from_before_are_cleared_once_and_live_ones_are_kept() {
    let doc = SessionDoc::new();
    doc.write(&with_items(500), ORIGIN_LOCAL).expect("written");
    // Deleted the old way: the key only, the map's fields kept.
    let items = doc.loro().get_map("items");
    for i in 100..500 {
        items.delete(&format!("item-{i}")).expect("a key deleted");
    }
    doc.loro().commit();
    let before = snapshot_len(&doc);

    let cleared = clear_deleted_entities(doc.loro()).expect("cleared");
    assert_eq!(cleared, 400);
    assert!(snapshot_len(&doc) < before);
    assert_eq!(
        doc.read().items,
        with_items(100).items,
        "the live ones kept"
    );
    assert_eq!(clear_deleted_entities(doc.loro()).expect("again"), 0);
}

fn with_ext(snapshots: usize, size: usize) -> SessionModel {
    let mut model = with_items(0);
    let mix = model.project.ext.entry("fts.mix".into()).or_default();
    for i in 0..snapshots {
        mix.insert(format!("snapshot.{i}"), format!("{i}:{}", "x".repeat(size)));
    }
    model
}

#[test]
fn one_key_of_the_projects_ext_is_written_as_that_key() {
    let doc = SessionDoc::new();
    doc.write(&with_ext(20, 50_000), ORIGIN_LOCAL)
        .expect("written");
    let version = doc.loro().oplog_vv();
    let mut more = with_ext(20, 50_000);
    more.project
        .ext
        .get_mut("fts.mix")
        .expect("the section")
        .insert("head".into(), "snapshot.3".into());
    doc.write(&more, ORIGIN_LOCAL).expect("one key more");
    let update = doc
        .loro()
        .export(ExportMode::updates(&version))
        .expect("the update");
    assert!(
        update.len() < 1_000,
        "one key's write is {} bytes",
        update.len()
    );
    assert_eq!(doc.read().project.ext, more.project.ext);
}

#[test]
fn a_projects_ext_written_whole_by_an_older_build_is_read_and_replaced() {
    let doc = SessionDoc::new();
    let old = with_ext(2, 10);
    // As an older build wrote it: one value.
    let value: std::collections::HashMap<String, loro::LoroValue> = old
        .project
        .ext
        .iter()
        .map(|(section, keys)| {
            let keys: std::collections::HashMap<String, loro::LoroValue> = keys
                .iter()
                .map(|(k, v)| (k.clone(), loro::LoroValue::from(v.as_str())))
                .collect();
            (section.clone(), loro::LoroValue::from(keys))
        })
        .collect();
    doc.loro()
        .get_map("project")
        .insert("ext", value)
        .expect("the old value");
    doc.loro().commit();
    assert_eq!(doc.read().project.ext, old.project.ext, "read as it was");

    let mut now = old.clone();
    now.project
        .ext
        .get_mut("fts.mix")
        .expect("the section")
        .remove("snapshot.0");
    doc.write(&now, ORIGIN_LOCAL).expect("written");
    assert!(matches!(
        doc.loro().get_map("project").get("ext"),
        Some(loro::ValueOrContainer::Container(_))
    ));
    assert_eq!(doc.read().project.ext, now.project.ext);
}

#[test]
fn an_edit_to_a_take_marker_whose_key_has_a_slash_is_read_back() {
    use crate::model::TakeMarkerState;
    let mut model = with_items(0);
    let key = "../set in stone/Media/46-Trig.wav#kick.0001".to_owned();
    model.take_markers.insert(
        key.clone(),
        TakeMarkerState {
            name: "hit".into(),
            ..TakeMarkerState::default()
        },
    );
    let doc = SessionDoc::new();
    doc.write(&model, ORIGIN_LOCAL).expect("written");
    let watch = doc.watch();
    *watch.touched.lock().expect("the notes") = crate::doc::Touched::default();
    let mut moved = model.clone();
    moved.take_markers.get_mut(&key).expect("the marker").name = "moved".into();
    // Another peer's edit: the engine's own are not noted.
    doc.write(&moved, "a peer").expect("an edit");
    doc.loro().commit();
    let touched = watch.touched.lock().expect("the notes").clone();
    assert!(
        touched.take_markers.contains(&key),
        "the edit names the marker by its key: {:?}",
        touched.take_markers
    );
}

/// What another device's commits were named reaches this replica with
/// them: the steps it gained, by name, each once, oldest first.
#[test]
fn a_replica_reads_the_names_of_the_steps_it_gained() {
    let theirs = SessionDoc::new();
    let ours = SessionDoc::new();
    theirs.write(&with_items(1), ORIGIN_LOCAL).expect("written");
    ours.import(
        &theirs.updates_since(&ours.version()).expect("updates"),
        "remote",
    )
    .expect("imported");
    let since = ours.version();
    assert!(ours.messages_since(&since).is_empty(), "nothing new yet");

    let one = with_items(1);
    let two = with_items(2);
    let three = with_items(3);
    theirs
        .write_step(&one, &two, ORIGIN_LOCAL, "Agent: tighten snare")
        .expect("a step");
    theirs
        .write_step(&two, &three, ORIGIN_LOCAL, "Agent: tighten snare")
        .expect("a step");
    theirs
        .write_step(&three, &one, ORIGIN_LOCAL, "Set volume")
        .expect("a step");
    ours.import(&theirs.updates_since(&since).expect("updates"), "remote")
        .expect("imported");
    assert_eq!(
        ours.messages_since(&since),
        vec!["Agent: tighten snare".to_owned(), "Set volume".to_owned()]
    );
}

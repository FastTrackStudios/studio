use std::sync::Arc;

use frame::document::proto::{Node, NodeKind, Scene, Size};
use frame::theme::host::ThemeHost;
use frame::theme::runtime::Schema;
use frame_editor::doc::{frame_id, scene_to_doc};
use frame_editor::engine::Session;
use frame_editor::theme::{self, CompileError};

#[test]
fn edited_geometry_compiles_and_failure_keeps_the_published_theme() {
    let mut scene = Scene::new("strip");
    scene.variables.modes.push("dark".into());
    let page = scene.pages()[0];
    let mut root = Node::new(NodeKind::Frame, "root");
    root.size = Size {
        width: 40.0,
        height: 20.0,
    };
    root.parent = Some(page);
    let root_id = root.id;
    scene.nodes.get_mut(&page).unwrap().children.push(root_id);
    scene.nodes.insert(root_id, root);
    let doc = scene_to_doc(&scene).unwrap();
    let id = doc
        .pages
        .iter()
        .flat_map(|p| doc.subtree(*p))
        .find(|id| frame_id(doc.node(*id).unwrap()) == Some(root_id.0))
        .unwrap();
    let mut session = Session::with_document(doc);
    let schema = Schema::new();
    let host = ThemeHost::new(schema);
    theme::publish(session.doc().unwrap(), &host, None).unwrap();
    assert_eq!(
        host.current().unwrap().authored_size("strip"),
        Some((40.0, 20.0))
    );
    let fields = host.fields(&[]);
    let first = host.capture("strip", None, 1.0, &fields).unwrap();
    assert_eq!((first.pixels.width, first.pixels.height), (40, 20));

    session
        .execute(
            "transform.resize",
            &serde_json::json!({"ids": [id], "width": 80.0, "height": 20.0}),
        )
        .unwrap();
    theme::publish(session.doc().unwrap(), &host, None).unwrap();
    let before = host.current().unwrap();
    let version = host.version();
    assert_eq!(before.authored_size("strip"), Some((80.0, 20.0)));
    let edited = host.capture("strip", None, 1.0, &fields).unwrap();
    assert_eq!((edited.pixels.width, edited.pixels.height), (80, 20));
    assert!(matches!(
        theme::publish(session.doc().unwrap(), &host, Some("missing")),
        Err(CompileError::Theme(_))
    ));
    assert!(Arc::ptr_eq(&host.current().unwrap(), &before));
    assert_eq!(host.version(), version);

    // A compile started before a newer host publication cannot replace it.
    let stale = theme::compile(session.doc().unwrap(), host.schema(), None).unwrap();
    host.set(Some(before.clone()));
    let newer_version = host.version();
    assert!(!host.set_if_version(version, stale));
    assert_eq!(host.version(), newer_version);
    assert!(Arc::ptr_eq(&host.current().unwrap(), &before));
}

#[test]
#[ignore = "manual dense-document timing"]
fn dense_document_compile_timing() {
    let mut scene = Scene::new("dense");
    let page = scene.pages()[0];
    let mut root = Node::new(NodeKind::Frame, "root");
    root.size = Size {
        width: 1000.0,
        height: 1000.0,
    };
    root.parent = Some(page);
    let root_id = root.id;
    scene.nodes.get_mut(&page).unwrap().children.push(root_id);
    scene.nodes.insert(root_id, root);
    for i in 0..2000 {
        let mut node = Node::new(NodeKind::Rectangle, format!("node-{i}"));
        node.size = Size {
            width: 10.0,
            height: 10.0,
        };
        node.parent = Some(root_id);
        scene
            .nodes
            .get_mut(&root_id)
            .unwrap()
            .children
            .push(node.id);
        scene.nodes.insert(node.id, node);
    }
    let doc = scene_to_doc(&scene).unwrap();
    let host = ThemeHost::new(Schema::new());
    let mut times = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        theme::publish(&doc, &host, None).unwrap();
        times.push(start.elapsed());
    }
    times.sort();
    eprintln!(
        "2000 nodes, full conversion + compile + publication: median {:?}, max {:?}",
        times[2], times[4]
    );
}

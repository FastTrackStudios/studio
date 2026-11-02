use std::sync::Arc;

use frame::document::proto::{Node, NodeKind, Scene, Size};
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
    let mut published = theme::compile(session.doc().unwrap(), &schema, None).unwrap();
    assert_eq!(published.authored_size("strip"), Some((40.0, 20.0)));

    session.doc_mut().unwrap().node_mut(id).unwrap().width = 80.0;
    published = theme::compile(session.doc().unwrap(), &schema, None).unwrap();
    assert_eq!(published.authored_size("strip"), Some((80.0, 20.0)));
    let before = published.clone();
    assert!(matches!(
        theme::compile(session.doc().unwrap(), &schema, Some("missing")),
        Err(CompileError::Theme(_))
    ));
    assert!(Arc::ptr_eq(&published, &before));
}

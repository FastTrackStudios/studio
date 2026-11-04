use std::sync::Arc;

use frame::document::proto::{Node, NodeKind, Scene, Size};
use frame::theme::host::ThemeHost;
use frame::theme::runtime::Schema;
use frame_editor::doc::{frame_id, scene_to_doc};
use frame_editor::engine::Session;
use frame_editor::live::{LiveEditor, OpenError, PreviewEvent};
use frame_editor::theme::{self, CompileError};

fn event(editor: &LiveEditor) -> PreviewEvent {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(event) = editor.try_event() {
            return event;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "preview worker timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn opening_the_host_preserves_layered_drafts_mode_and_rejects_newer_reload() {
    let dir = std::env::temp_dir().join(format!("frame-editor-snapshot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let base = dir.join("base.frame");
    let overlay = dir.join("overlay.frame");
    std::fs::write(&base, r##"<Document><Variables modes="dark light"><Var name="ground" dark="#202020" light="#eeeeee" /></Variables><Page name="base"><Frame name="root" w=20 h=20 bind:fill="$ground" /></Page></Document>"##).unwrap();
    std::fs::write(&overlay, r##"<Document><Page name="overlay"><Frame name="root" w=20 h=20 fill="#202020" /></Page></Document>"##).unwrap();
    let host = Arc::new(ThemeHost::new(Schema::new()));
    host.load(vec![base, overlay.clone()], Some("light".into()))
        .unwrap();
    host.edit(&overlay, r##"<Document><Page name="overlay"><Frame name="root" w=20 h=20 fill="#d04040" /></Page></Document>"##.into()).unwrap();
    let mut opening = LiveEditor::open_async(host.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut editor = loop {
        if let Some(result) = opening.try_finish() {
            break result.unwrap();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    editor.request_preview(None).unwrap();
    event(&editor).result.unwrap();
    let theme = host.current().unwrap();
    assert_eq!(theme.mode(), "light");
    assert!(theme.has_role("base") && theme.has_role("overlay"));
    let fields = host.fields(&[]);
    assert_eq!(
        host.capture("overlay", None, 1.0, &fields)
            .unwrap()
            .pixels
            .at(10, 10),
        [0xd0, 0x40, 0x40, 0xff]
    );
    host.reload().unwrap();
    assert!(editor.host_changed());
    let after_reload = host.current().unwrap();
    editor.request_preview(Some("missing")).unwrap();
    assert!(matches!(
        event(&editor).result,
        Err(CompileError::HostChanged)
    ));
    assert!(Arc::ptr_eq(&after_reload, &host.current().unwrap()));
}

#[test]
fn opening_without_a_theme_reports_the_error() {
    let host = Arc::new(ThemeHost::new(Schema::new()));
    let mut opening = LiveEditor::open_async(host);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(result) = opening.try_finish() {
            assert!(matches!(result, Err(OpenError::NoTheme)));
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn setting_a_bound_fill_updates_the_visible_pixel() {
    let scene = frame::document::text::parse(r##"<Document><Variables><Var name="base" value="#202020" /></Variables><Page name="tcp.row"><Frame name="Row" w=20 h=20><Rectangle name="Field" w=20 h=20 bind:fill="$base" /></Frame></Page></Document>"##, Default::default()).unwrap();
    let doc = scene_to_doc(&scene).unwrap();
    let field = doc
        .pages
        .iter()
        .flat_map(|p| doc.subtree(*p))
        .find(|id| doc.node(*id).is_some_and(|n| n.name == "Field"))
        .unwrap();
    let host = Arc::new(ThemeHost::new(Schema::new()));
    let mut editor = LiveEditor::new(doc, host.clone());
    editor.request_preview(None).unwrap();
    event(&editor).result.unwrap();
    let fields = host.fields(&[]);
    assert_eq!(
        host.capture("tcp.row", None, 1.0, &fields)
            .unwrap()
            .pixels
            .at(10, 10),
        [0x20, 0x20, 0x20, 0xff]
    );
    editor.set_fill(field, "#ff00ff").unwrap();
    event(&editor).result.unwrap();
    assert_eq!(
        host.capture("tcp.row", None, 1.0, &fields)
            .unwrap()
            .pixels
            .at(10, 10),
        [0xff, 0, 0xff, 0xff]
    );
    assert!(
        frame_editor::doc::effective(editor.session().doc().unwrap(), field)
            .bind
            .iter()
            .all(|b| b.prop != "fill")
    );
}

#[test]
fn editor_command_updates_host_on_worker_and_failure_keeps_it() {
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
    let host = Arc::new(ThemeHost::new(Schema::new()));
    let mut editor = LiveEditor::new(doc, host.clone());
    let initial = editor.request_preview(None).unwrap();
    let completed = event(&editor);
    assert_eq!(completed.request, initial);
    completed.result.unwrap();

    editor
        .execute(
            "transform.resize",
            &serde_json::json!({"ids": [id], "width": 80.0, "height": 20.0}),
            None,
        )
        .unwrap();
    event(&editor).result.unwrap();
    let fields = host.fields(&[]);
    let shot = host.capture("strip", None, 1.0, &fields).unwrap();
    assert_eq!((shot.pixels.width, shot.pixels.height), (80, 20));
    editor
        .execute(
            "node.set",
            &serde_json::json!({"id": id, "props": {"fill": "#d04040"}}),
            None,
        )
        .unwrap();
    event(&editor).result.unwrap();
    let shot = host.capture("strip", None, 1.0, &fields).unwrap();
    assert_eq!(shot.pixels.at(10, 10), [0xd0, 0x40, 0x40, 0xff]);
    let before = host.current().unwrap();
    let version = host.version();
    editor.request_preview(Some("missing")).unwrap();
    assert!(matches!(event(&editor).result, Err(CompileError::Theme(_))));
    assert!(Arc::ptr_eq(&before, &host.current().unwrap()));
    assert_eq!(version, host.version());
}

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

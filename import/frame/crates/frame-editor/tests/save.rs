use std::sync::Arc;
use std::time::{Duration, Instant};

use frame::theme::host::ThemeHost;
use frame::theme::runtime::Schema;
use frame_editor::live::LiveEditor;
use frame_editor::save::{SaveError, SaveSnapshot};

fn wait(editor: &LiveEditor) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = editor.try_event() {
            event.result.unwrap();
            return;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_save(editor: &mut LiveEditor) -> Result<(), SaveError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = editor.try_save_event() {
            return result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn save_a_role_preserves_layers_drafts_and_detects_disk_conflict() {
    let dir = std::env::temp_dir().join(format!("frame-save-role-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let base = dir.join("base.frame");
    let target = dir.join("tcp.frame");
    let base_text = r##"<Document><Variables><Var name="base" value="#202020" /></Variables><Page name="other"><Frame name="Other" w=20 h=20 fill="#000000"><Text name="Label" w=54 h=12 size=9 text="1" /></Frame></Page></Document>"##;
    let target_text = r##"<Document><Page name="tcp.row"><Frame name="Row" w=20 h=20><Rectangle name="Field" w=20 h=20 bind:fill="$base" /></Frame></Page></Document>"##;
    std::fs::write(&base, base_text).unwrap();
    std::fs::write(&target, target_text).unwrap();
    let host = Arc::new(ThemeHost::new(Schema::new()));
    host.load(vec![base.clone(), target.clone()], None).unwrap();
    let base_draft = base_text.replace("#000000", "#111111");
    host.edit(&base, base_draft.clone()).unwrap();
    let mut editor = LiveEditor::from_host(host.clone()).unwrap();
    let doc = editor.session().doc().unwrap();
    let field = doc
        .pages
        .iter()
        .flat_map(|p| doc.subtree(*p))
        .find(|id| doc.node(*id).is_some_and(|n| n.name == "Field"))
        .unwrap();
    editor.set_fill(field, "#ff00ff").unwrap();
    wait(&editor);
    let fields = host.fields(&[]);
    assert_eq!(
        host.capture("tcp.row", None, 1.0, &fields)
            .unwrap()
            .pixels
            .at(10, 10),
        [255, 0, 255, 255]
    );
    editor
        .request_save("tcp.row".into(), target.clone())
        .unwrap();
    wait_save(&mut editor).unwrap();
    assert_eq!(std::fs::read_to_string(&base).unwrap(), base_text);
    assert_eq!(host.file_text(&base).unwrap(), base_draft);
    assert!(
        std::fs::read_to_string(&target)
            .unwrap()
            .contains("#ff00ff")
    );
    host.reload().unwrap();
    assert_eq!(
        host.capture("tcp.row", None, 1.0, &fields)
            .unwrap()
            .pixels
            .at(10, 10),
        [255, 0, 255, 255]
    );
    let restarted = ThemeHost::new(Schema::new());
    restarted
        .load(vec![base.clone(), target.clone()], None)
        .unwrap();
    assert_eq!(
        restarted
            .capture("tcp.row", None, 1.0, &fields)
            .unwrap()
            .pixels
            .at(10, 10),
        [255, 0, 255, 255]
    );

    let mut editor = LiveEditor::from_host(host.clone()).unwrap();
    let external = std::fs::read_to_string(&target)
        .unwrap()
        .replace("#ff00ff", "#00ffff");
    std::fs::write(&target, &external).unwrap();
    editor
        .request_save("tcp.row".into(), target.clone())
        .unwrap();
    let result = wait_save(&mut editor);
    assert!(matches!(result, Err(SaveError::Stage(_))));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), external);
    assert_eq!(
        host.capture("tcp.row", None, 1.0, &fields)
            .unwrap()
            .pixels
            .at(10, 10),
        [255, 0, 255, 255]
    );
}

#[test]
fn saving_rejects_changes_outside_the_role_and_later_overrides() {
    let dir = std::env::temp_dir().join(format!("frame-save-guard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let base = dir.join("base.frame");
    let target = dir.join("tcp.frame");
    let later = dir.join("later.frame");
    let base_text = r##"<Document><Page name="other"><Frame name="Other" w=20 h=20 fill="#000000" /></Page></Document>"##;
    let role_text = r##"<Document><Page name="tcp.row"><Frame name="Row" w=20 h=20 fill="#202020" /></Page></Document>"##;
    std::fs::write(&base, base_text).unwrap();
    std::fs::write(&target, role_text).unwrap();
    let host = Arc::new(ThemeHost::new(Schema::new()));
    host.load(vec![base.clone(), target.clone()], None).unwrap();
    let snapshot = SaveSnapshot::capture(&host).unwrap();
    let mut editor = LiveEditor::from_host(host.clone()).unwrap();
    let doc = editor.session().doc().unwrap();
    let other = doc
        .pages
        .iter()
        .flat_map(|p| doc.subtree(*p))
        .find(|id| doc.node(*id).is_some_and(|n| n.name == "Other"))
        .unwrap();
    editor.set_fill(other, "#ff00ff").unwrap();
    wait(&editor);
    assert!(matches!(
        snapshot.save_role(
            &host,
            editor.session().doc().unwrap(),
            "tcp.row",
            &target,
            host.version()
        ),
        Err(SaveError::OtherChanges)
    ));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), role_text);

    std::fs::write(&later, role_text).unwrap();
    host.load(vec![base, target.clone(), later], None).unwrap();
    let snapshot = SaveSnapshot::capture(&host).unwrap();
    let editor = LiveEditor::from_host(host.clone()).unwrap();
    assert!(matches!(
        snapshot.save_role(
            &host,
            editor.session().doc().unwrap(),
            "tcp.row",
            &target,
            host.version()
        ),
        Err(SaveError::LaterOverride)
    ));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), role_text);
}

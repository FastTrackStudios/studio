//! The theme actions over MCP: an agent inspects, edits, saves, reverts
//! and captures a running app's theme, a person approving each write.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use architect::action::{ActionList, Principal};
use architect_mcp::Server;
use base64::Engine as _;
use frame::theme::host::ThemeHost;
use frame::theme::runtime::Schema;
use serde_json::{Value, json};

const TOKENS: &str = r##"<Document>
  <Variables modes="dark light">
    <Var name="ground" dark="#202020" light="#e8e8e8" />
    <Var name="mute.on" value="#d04040" />
  </Variables>
</Document>
"##;

const STRIP: &str = r##"<Document>
  <Page name="mcp.strip">
    <Frame name="Strip" w=40 h=20 bind:fill="$ground">
      <Frame name="@mute" x=0 y=0 w=20 h=20 bind:fill="track.mute ? $mute.on : $ground" />
    </Frame>
  </Page>
</Document>
"##;

fn theme_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "frame-architect-theme-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a-tokens.frame"), TOKENS).unwrap();
    std::fs::write(dir.join("b-strip.frame"), STRIP).unwrap();
    dir
}

/// An app with its theme loaded from `dir`, the actions on its list, and
/// confirmation on; and MCP over it for the model `service:agent`.
fn app(dir: &Path) -> (Server, Arc<ThemeHost>) {
    let host = Arc::new(ThemeHost::new(
        Schema::new().field("track.name").live("track.mute"),
    ));
    host.load(vec![dir.to_path_buf()], None).expect("compiles");
    let list = ActionList::new();
    frame_architect::theme::register(&list, &host).expect("registered");
    list.validate().expect("valid");
    list.require_confirmation(Duration::from_secs(60));
    let server = Server::new("Test", "0", list).with_principal(Principal::Service {
        name: "agent".to_owned(),
    });
    (server, host)
}

/// `run` `id` with `args` (and `ticket`): the tool result as JSON, and
/// whether it is an error.
fn run(server: &Server, id: &str, args: &Value, ticket: Option<&str>) -> (Value, bool) {
    let mut arguments = json!({"id": id, "args": args});
    if let Some(ticket) = ticket {
        arguments["ticket"] = json!(ticket);
    }
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "run", "arguments": arguments}});
    let reply: Value =
        serde_json::from_str(&server.handle(&request.to_string()).expect("a reply")).unwrap();
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    (
        serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())),
        reply["result"]["isError"] == json!(true),
    )
}

fn ticket(answer: &(Value, bool)) -> String {
    assert!(answer.1, "an error result: {answer:?}");
    assert_eq!(answer.0["confirmation_required"], true, "{answer:?}");
    answer.0["ticket"].as_str().expect("a ticket").to_owned()
}

/// Run a call that needs a person: asked, approved, run with the ticket.
fn approved(server: &Server, id: &str, args: &Value) -> (Value, bool) {
    let t = ticket(&run(server, id, args, None));
    assert!(server.list().approve(&t));
    run(server, id, args, Some(&t))
}

fn ok(answer: (Value, bool)) -> Value {
    assert!(!answer.1, "{answer:?}");
    answer.0
}

#[test]
fn an_agent_edits_saves_and_reverts_with_a_persons_yes() {
    let dir = theme_dir();
    let (s, host) = app(&dir);
    let strip = dir.join("b-strip.frame");
    let original = std::fs::read(&strip).unwrap();

    // Reading never asks.
    let before = ok(run(&s, "THEME_INSPECT", &json!(null), None));
    assert_eq!(before["loaded"], true);
    assert_eq!(before["files"].as_array().unwrap().len(), 2);
    assert_eq!(before["roles"][0]["name"], "mcp.strip");
    let shot = |s: &Server| {
        ok(run(
            s,
            "VIEW_CAPTURE",
            &json!({"role": "mcp.strip", "scale": 2, "fields": {"track.mute": "true"}}),
            None,
        ))
    };
    let first = shot(&s);
    assert_eq!(
        (first["width"].clone(), first["height"].clone()),
        (json!(80), json!(40))
    );
    let png = base64::engine::general_purpose::STANDARD
        .decode(first["png_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(&png[1..4], b"PNG");
    assert!(s.list().pending_confirmations().is_empty());

    // An edit waits for the person; the caller sees the sheet's ticket.
    let edited = STRIP.replace("$ground\">", "$mute.on\">");
    let args = json!({"file": strip.display().to_string(), "markup": edited});
    let asked = run(&s, "THEME_EDIT", &args, None);
    let t = ticket(&asked);
    let pending = s.list().pending_confirmations();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].caller, "service:agent");
    assert_eq!(host.inspect().version, before["version"].as_u64().unwrap());
    // Denied: nothing changes.
    assert!(s.list().deny(&t));
    assert!(run(&s, "THEME_EDIT", &args, Some(&t)).1);
    assert!(host.inspect().files.iter().all(|f| !f.draft));

    // Approved: drawn with, not yet saved.
    let changed = ok(approved(&s, "THEME_EDIT", &args));
    assert!(changed["version"].as_u64().unwrap() > before["version"].as_u64().unwrap());
    assert_ne!(shot(&s)["blake3"], first["blake3"]);
    assert_eq!(std::fs::read(&strip).unwrap(), original);

    // Saving is destructive: asked, then written.
    let saved = ok(approved(&s, "THEME_SAVE", &json!(null)));
    assert_eq!(saved["files"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read_to_string(&strip).unwrap(), edited);

    // Reverting too: every byte back, and the same pixels.
    let reverted = ok(approved(&s, "THEME_REVERT", &json!(null)));
    assert_eq!(reverted["files"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(&strip).unwrap(), original);
    let after = ok(run(&s, "THEME_INSPECT", &json!(null), None));
    assert_eq!(after["files"], before["files"]);
    assert_eq!(after["tokens"], before["tokens"]);
    assert_eq!(shot(&s)["blake3"], first["blake3"]);
}

#[test]
fn a_bad_edit_over_mcp_keeps_the_last_good_theme_and_lists_every_error() {
    let dir = theme_dir();
    let (s, host) = app(&dir);
    let good = host.current().unwrap();
    let broken = STRIP
        .replace("$ground\">", "$nope\">")
        .replace("track.mute ?", "track.nope ?");
    let args = json!({"file": dir.join("b-strip.frame").display().to_string(), "markup": broken});

    let (said, failed) = approved(&s, "THEME_EDIT", &args);
    assert!(failed);
    let said = said
        .as_str()
        .map_or_else(|| said.to_string(), str::to_owned);
    assert!(said.contains("last good theme stays"), "{said}");
    assert!(
        said.contains("nope") && said.contains("track.nope"),
        "{said}"
    );
    assert!(Arc::ptr_eq(&good, &host.current().unwrap()));

    let errors = ok(run(&s, "THEME_ERRORS", &json!(null), None));
    let errors = errors.as_array().unwrap();
    assert!(errors.len() >= 2, "{errors:#?}");
    assert!(
        errors
            .iter()
            .all(|e| e["kind"] == "Compile" && e["role"] == "mcp.strip")
    );

    // A file that is not the theme's is refused, after the person's yes.
    let (said, failed) = approved(
        &s,
        "THEME_EDIT",
        &json!({"file": "/etc/passwd", "markup": STRIP}),
    );
    assert!(failed, "{said}");
    assert!(
        said.to_string().contains("not one of the theme's files"),
        "{said}"
    );

    // Applying a theme that does not load keeps this one too.
    let (_, failed) = approved(
        &s,
        "THEME_APPLY",
        &json!({"paths": [dir.join("missing.frame").display().to_string()]}),
    );
    assert!(failed);
    assert!(Arc::ptr_eq(&good, &host.current().unwrap()));

    // Reload asks no one, and the files on disk are still good.
    ok(run(&s, "THEME_RELOAD", &json!(null), None));
    assert!(
        ok(run(&s, "THEME_ERRORS", &json!(null), None))
            .as_array()
            .unwrap()
            .is_empty()
    );
}

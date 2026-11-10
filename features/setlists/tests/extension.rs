//! The extension against a real vault, as an Architect app hosts it: the
//! app's list holds the vault's and Bases' actions over a folder, and the
//! extension reaches them only through the host service. Libraries by
//! category, setlists from any library, the navigator's two stages, and
//! a share made with the app's own `SHARE_CREATE`.
#![cfg(feature = "extension")]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::needless_pass_by_value
)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use architect::action::{ActionList, ActionMeta, Effect};
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::install::Installs;
use architect_settings::Registry;
use futures::executor::block_on;
use serde_json::{Value, json};
use setlists::layout::Library;
use setlists::song::{Arrangement, Chart, Song};
use setlists::vault::Vault;

static SHARE_CREATE: ActionMeta = ActionMeta {
    id: "SHARE_CREATE",
    trait_name: "",
    method_name: "",
    display_name: "Create a share link",
    description: "The test's stand-in for architect-accounts' SHARE_CREATE.",
    category: "Share",
    group: "Share",
    shortcut: "",
    toggleable: false,
    undo: false,
};

/// What the stand-in `SHARE_CREATE` was asked.
#[derive(facet::Facet)]
struct ShareArgs {
    notes: Vec<String>,
    audience: String,
    title: String,
}

#[derive(facet::Facet)]
struct ShareOut {
    code: String,
    url: String,
}

/// The app's side: its list with `VAULT_*` and `BASES_*` over `dir`, and
/// a `SHARE_CREATE` that records what it was asked.
fn app(dir: &Path, asked: Arc<Mutex<Vec<Vec<String>>>>) -> Arc<ActionList> {
    let list = Arc::new(ActionList::new());
    let open = architect_vault::actions::OpenVault::open(dir).unwrap();
    architect_vault::actions::register(&list, &open).unwrap();
    let types: &'static architect_bases::model::ViewTypes =
        Box::leak(Box::new(architect_bases::model::ViewTypes::new()));
    architect_bases::actions::register(&list, &open, types).unwrap();
    list.register(&SHARE_CREATE, Effect::Mutating, move |a: ShareArgs| {
        assert_eq!(a.audience, "link");
        assert_eq!(a.title, "Christmas Eve");
        asked.lock().unwrap().push(a.notes);
        Ok(ShareOut {
            code: "hzhbdfiz7gaxwv78".to_owned(),
            url: "http://node/s/hzhbdfiz7gaxwv78/".to_owned(),
        })
    })
    .unwrap();
    list
}

fn run(list: &ActionList, id: &str, args: Value) -> Value {
    let out = block_on(list.run_json_async(id, args.to_string().as_bytes()))
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_slice(&out).unwrap_or(Value::Null)
}

fn song(
    vault: &Vault,
    lib: &str,
    title: &str,
    key: &str,
    categories: &[&str],
    arrangement: Option<&str>,
) {
    let s = Song {
        title: title.to_owned(),
        categories: categories.iter().map(|c| (*c).to_owned()).collect(),
        chart: Chart {
            key: Some(key.to_owned()),
            tempo: Some(80.0),
            ..Chart::default()
        },
        arrangements: arrangement
            .map(|a| Arrangement {
                name: a.to_owned(),
                chart: Chart::default(),
            })
            .into_iter()
            .collect(),
        ..Song::default()
    };
    let lib = Library::named(lib);
    vault.ensure_library(&lib).unwrap();
    vault.write_song(&lib, &s, &[]).unwrap();
}

/// Runs `f` until it is true (the vault's index follows its files).
fn until(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "waited for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn libraries_setlists_and_a_share_through_the_app() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("vault");
    std::fs::create_dir_all(&dir).unwrap();
    let vault = Vault::at(&dir);
    song(
        &vault,
        "Christmas",
        "Silent Night",
        "A",
        &["Carols", "Quiet"],
        Some("Acoustic"),
    );
    song(&vault, "Christmas", "Sleigh Ride", "B", &[], None);
    song(&vault, "Hymns", "Old Hundredth", "G", &["Doxology"], None);

    let asked = Arc::new(Mutex::new(Vec::new()));
    let app = app(&dir, Arc::clone(&asked));
    let list = ActionList::new();
    let host = Host::new(list.clone(), SharedSections::new(Registry::new()));
    host.set_app_list((*app).clone());
    host.forward_actions_to(&app);
    let ext = Extensions::new(host, Installs::new(tmp.path().join("installs")))
        .with_linked(&setlists::ext::Setlists)
        .unwrap();
    ext.install_contribution_actions().unwrap();
    let _ = ext.sync();

    // The libraries, by category.
    until("the songs", || {
        run(&list, "STUDIO_SETLISTS_SONGS", json!({}))
            .as_array()
            .is_some_and(|a| a.len() == 3)
    });
    let libs = run(&list, "STUDIO_SETLISTS_LIBRARIES", json!({}));
    assert_eq!(libs[0]["library"], "Christmas");
    assert_eq!(libs[0]["categories"][0]["category"], "Carols");
    assert_eq!(libs[0]["uncategorised"], 1);
    assert_eq!(
        run(
            &list,
            "STUDIO_SETLISTS_SONGS",
            json!({"category": "doxology"})
        )[0]["song"],
        "Old Hundredth"
    );

    // A setlist from two libraries, an arrangement included.
    let made = run(
        &list,
        "STUDIO_SETLISTS_CREATE",
        json!({"title": "Christmas Eve", "songs": ["Silent Night#Acoustic", "Hymns/Old Hundredth"]}),
    );
    assert_eq!(
        made["songs"],
        json!(["Silent Night (Acoustic)", "Old Hundredth"])
    );
    let note = std::fs::read_to_string(dir.join("Setlists/Christmas Eve.md")).unwrap();
    assert!(note.contains("type: setlist"), "{note}");
    assert!(
        note.contains(
            "Libraries/Christmas/Silent Night/Silent Night#Acoustic|Silent Night (Acoustic)"
        ),
        "{note}"
    );

    // Edits: the order is the note's `songs`.
    until("the setlist", || {
        run(&list, "STUDIO_SETLISTS_LIST", json!({}))
            .as_array()
            .is_some_and(|a| a.len() == 1)
    });
    run(
        &list,
        "STUDIO_SETLISTS_ADD",
        json!({"setlist": "Christmas Eve", "song": "Sleigh Ride", "at": 1}),
    );
    run(
        &list,
        "STUDIO_SETLISTS_MOVE",
        json!({"setlist": "Christmas Eve", "song": "Old Hundredth", "to": 1}),
    );
    let order = |list: &ActionList| -> Vec<String> {
        run(list, "STUDIO_SETLISTS_LIST", json!({}))[0]["songs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["song"].as_str().unwrap().to_owned())
            .collect()
    };
    until("the edits", || {
        order(&list) == ["Old Hundredth", "Sleigh Ride", "Silent Night (Acoustic)"]
    });
    assert!(
        block_on(list.run_json_async(
            "STUDIO_SETLISTS_ADD",
            br#"{"setlist":"Christmas Eve","song":"Nope"}"#
        ))
        .is_err()
    );

    // The navigator's two stages: the setlist on the left, its songs on
    // the right in order, each opening as a song's scope tab.
    let snap = run(&list, "STUDIO_SETLISTS_NAV_SNAPSHOT", json!({}));
    let titles: Vec<&str> = snap["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        ["Old Hundredth", "Sleigh Ride", "Silent Night (Acoustic)"]
    );
    assert_eq!(snap["items"][0]["open"]["kind"], "context");
    assert_eq!(snap["items"][0]["open"]["context_type"], "song");
    let libs = run(&list, "STUDIO_SETLISTS_LIBRARIES_NAV_SNAPSHOT", json!({}));
    assert_eq!(libs["tree"].as_array().unwrap().len(), 2);

    // Shared with the app's share links: the setlist and its songs.
    let shared = run(
        &list,
        "STUDIO_SETLISTS_SHARE",
        json!({"setlist": "Christmas Eve"}),
    );
    assert_eq!(shared["code"], "hzhbdfiz7gaxwv78");
    assert_eq!(
        asked.lock().unwrap()[0],
        [
            "Setlists/Christmas Eve.md",
            "Libraries/Hymns/Old Hundredth/Old Hundredth.md",
            "Libraries/Christmas/Sleigh Ride/Sleigh Ride.md",
            "Libraries/Christmas/Silent Night/Silent Night.md",
        ]
    );
    until("the share code", || {
        std::fs::read_to_string(dir.join("Setlists/Christmas Eve.md"))
            .unwrap()
            .contains("share: hzhbdfiz7gaxwv78")
    });
}

//! The extension against a real vault, as an Architect app hosts it: the
//! app's list holds the vault's and Bases' actions over a folder, and the
//! extension reaches them only through the host service.
//!
//! The DAW files are the adapters' own fixtures, copied into the vault at
//! test time (none is kept here): an Ableton set from
//! `features/adapters/ableton/tests/fixtures` and a Pro Tools session
//! from `features/adapters/protools/tests/fixtures` (ptformat's, LGPL).
//! The show set is that Ableton set with locators added, written to a
//! temporary folder by the adapter's writer.
#![cfg(feature = "extension")]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::struct_field_names,
    clippy::significant_drop_tightening,
    clippy::doc_markdown
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use architect::action::ActionList;
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::install::Installs;
use architect_settings::Registry;
use futures::executor::block_on;
use serde_json::{Value, json};

/// The extension keeps one door to its host for the process: one test at
/// a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../adapters")
        .join(rel)
}

const LIVE_SET: &str = "ableton/tests/fixtures/ableton/LucidDreaming.als";
const PT_SESSION: &str = "protools/tests/fixtures/protools/RegionTest.ptx";

/// An app over a new vault with the extension linked in: the list its
/// actions are run on, the vault's folder, and what keeps both alive.
struct App {
    /// The extension's actions.
    list: ActionList,
    /// The app's own: the vault's and Bases'.
    app: Arc<ActionList>,
    dir: PathBuf,
    _ext: Extensions,
    _tmp: tempfile::TempDir,
    _one: MutexGuard<'static, ()>,
}

fn app() -> App {
    let one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("vault");
    std::fs::create_dir_all(&dir).unwrap();

    let app = Arc::new(ActionList::new());
    let open = architect_vault::actions::OpenVault::open(&dir).unwrap();
    architect_vault::actions::register(&app, &open).unwrap();
    let types: &'static architect_bases::model::ViewTypes =
        Box::leak(Box::new(architect_bases::model::ViewTypes::new()));
    architect_bases::actions::register(&app, &open, types).unwrap();

    let list = ActionList::new();
    let host = Host::new(list.clone(), SharedSections::new(Registry::new()));
    host.set_app_list((*app).clone());
    host.forward_actions_to(&app);
    let ext = Extensions::new(host, Installs::new(tmp.path().join("installs")))
        .with_linked(&productions::ext::Productions)
        .unwrap();
    ext.install_contribution_actions().unwrap();
    let _ = ext.sync();
    App {
        list,
        app,
        dir,
        _ext: ext,
        _tmp: tmp,
        _one: one,
    }
}

impl App {
    fn try_run(&self, id: &str, args: Value) -> Result<Value, String> {
        let list = if id.starts_with("STUDIO_") || id.starts_with("EXT_") {
            &self.list
        } else {
            &*self.app
        };
        block_on(list.run_json_async(id, args.to_string().as_bytes()))
            .map(|out| serde_json::from_slice(&out).unwrap_or(Value::Null))
            .map_err(|e| e.to_string())
    }

    fn run(&self, id: &str, args: Value) -> Value {
        self.try_run(id, args)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn note(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    /// An Ableton Project folder and a Pro Tools session folder, dropped
    /// into the vault's `Sessions/`.
    fn drop_two_folders(&self) -> (PathBuf, PathBuf) {
        let live = self.dir.join("Sessions/Lucid Dreaming Project");
        std::fs::create_dir_all(live.join("Ableton Project Info")).unwrap();
        std::fs::create_dir_all(live.join("Backup")).unwrap();
        std::fs::copy(fixture(LIVE_SET), live.join("Lucid Dreaming.als")).unwrap();
        std::fs::copy(
            fixture(LIVE_SET),
            live.join("Backup/Lucid Dreaming [old].als"),
        )
        .unwrap();
        let pt = self.dir.join("Sessions/Region Test");
        std::fs::create_dir_all(pt.join("Audio Files")).unwrap();
        std::fs::create_dir_all(pt.join("Session File Backups")).unwrap();
        std::fs::copy(fixture(PT_SESSION), pt.join("Region Test.ptx")).unwrap();
        std::fs::copy(
            fixture(PT_SESSION),
            pt.join("Session File Backups/Region Test.bak.001.ptx"),
        )
        .unwrap();
        (live, pt)
    }

    fn rows(&self, base: &str) -> Vec<Value> {
        let out = self.run("BASES_ROWS", json!({ "path": base }));
        out["rows"]["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|g| g["rows"].as_array().cloned().unwrap_or_default())
            .collect()
    }
}

fn by_title<'a>(projects: &'a Value, title: &str) -> &'a Value {
    projects
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["title"] == title)
        .unwrap_or_else(|| panic!("no {title} in {projects}"))
}

#[test]
fn two_project_folders_become_two_project_notes() {
    let app = app();
    let (live, pt) = app.drop_two_folders();
    let before = (
        std::fs::read(live.join("Lucid Dreaming.als")).unwrap(),
        std::fs::read(pt.join("Region Test.ptx")).unwrap(),
    );

    // A path inside the vault; the backups in each folder are not projects.
    let out = app.run("STUDIO_PRODUCTIONS_SCAN", json!({ "path": "Sessions" }));
    let projects = &out["projects"];
    assert_eq!(projects.as_array().unwrap().len(), 2, "{out}");
    let set = by_title(projects, "Lucid Dreaming");
    let session = by_title(projects, "Region Test");
    assert_eq!(set["type"], "ableton-set");
    assert_eq!(session["type"], "protools-session");
    for p in [set, session] {
        assert_eq!(p["outcome"], "created", "{p}");
        assert_eq!(p["stage"], "writing", "{p}");
        assert!(p["tempo"].as_f64().is_some_and(|t| t > 0.0), "{p}");
        assert!(p["tracks"].as_u64().is_some_and(|t| t > 0), "{p}");
        assert!(p["error"].is_null(), "{p}");
    }

    // The notes: Architect's project kind, of this extension's types.
    let note = app.note("Projects/Lucid Dreaming/project.md");
    for line in [
        "type: project",
        "projectType: ableton-set",
        "stage: writing",
        "state: active",
        "location: Sessions/Lucid Dreaming Project",
        "file: Sessions/Lucid Dreaming Project/Lucid Dreaming.als",
        "tempo: ",
        "tracks: ",
        "lastSaved: ",
    ] {
        assert!(note.contains(line), "no `{line}` in\n{note}");
    }
    let note = app.note("Projects/Region Test/project.md");
    assert!(note.contains("projectType: protools-session"), "{note}");
    assert!(
        note.contains("file: Sessions/Region Test/Region Test.ptx"),
        "{note}"
    );
    // A Pro Tools session stores no key: none is written.
    assert!(!note.contains("\nkey:"), "{note}");

    // Nothing was written to the foreign files.
    assert_eq!(
        std::fs::read(live.join("Lucid Dreaming.als")).unwrap(),
        before.0
    );
    assert_eq!(std::fs::read(pt.join("Region Test.ptx")).unwrap(), before.1);

    // The list, and its filters.
    let all = app.run("STUDIO_PRODUCTIONS_LIST", json!({}));
    assert_eq!(all.as_array().unwrap().len(), 2, "{all}");
    assert_eq!(all[0]["title"], "Lucid Dreaming");
    assert_eq!(all[0]["daw"], "Ableton Live");
    let pro_tools = app.run(
        "STUDIO_PRODUCTIONS_LIST",
        json!({ "type": "protools-session" }),
    );
    assert_eq!(pro_tools.as_array().unwrap().len(), 1);
    assert_eq!(
        app.run("STUDIO_PRODUCTIONS_LIST", json!({ "stage": "mixing" })),
        json!([])
    );

    // Its project types, on Architect's project kind.
    let types = app.run("EXT_PROJECT_TYPES", json!({}));
    let ids: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "ableton-set",
            "protools-session",
            "reaper-session",
            "logic-project"
        ]
    );
    assert_eq!(types[0]["properties"][0]["key"], "stage");
    assert_eq!(
        types[0]["properties"][0]["choices"]
            .as_array()
            .unwrap()
            .len(),
        6
    );

    // The default bases, written through the host and run by Bases.
    assert_eq!(out["bases"].as_array().unwrap().len(), 5, "{out}");
    assert_eq!(app.rows("Productions/By stage.base").len(), 2);
    assert_eq!(app.rows("Productions/By client.base").len(), 2);
    assert_eq!(app.rows("Productions/By due date.base").len(), 2);
    assert_eq!(app.rows("Productions/Changed this week.base").len(), 2);
    let missing = session["missingMedia"].as_u64().unwrap();
    assert!(
        missing > 0,
        "the session's audio files are not in the folder"
    );
    let rows = app.rows("Productions/Missing media.base");
    assert!(
        rows.iter()
            .any(|r| r["path"] == "Projects/Region Test/project.md"),
        "{rows:?}"
    );
    let board = app.run(
        "BASES_ROWS",
        json!({ "path": "Productions/By stage.base", "view": "By stage" }),
    );
    assert_eq!(board["rows"]["groups"][0]["label"], "writing", "{board}");

    // A stage change is a property edit: the board's group follows it.
    let set_to = app.run(
        "STUDIO_PRODUCTIONS_SET_STAGE",
        json!({ "project": "Region Test", "stage": "Mixing" }),
    );
    assert_eq!(set_to["was"], "writing");
    assert!(
        app.note("Projects/Region Test/project.md")
            .contains("stage: mixing")
    );
    let mixing = app.run("STUDIO_PRODUCTIONS_LIST", json!({ "stage": "mixing" }));
    assert_eq!(mixing[0]["title"], "Region Test");
    assert!(
        app.try_run(
            "STUDIO_PRODUCTIONS_SET_STAGE",
            json!({ "project": "Region Test", "stage": "done" })
        )
        .is_err()
    );

    // Open in its DAW: the platform's open command on the file.
    let opened = app.run(
        "STUDIO_PRODUCTIONS_OPEN",
        json!({ "project": "Lucid Dreaming", "dry_run": true }),
    );
    let command = opened["command"].as_array().unwrap();
    assert_eq!(
        command.last().unwrap().as_str().unwrap(),
        live.join("Lucid Dreaming.als").display().to_string()
    );
    assert_eq!(opened["opened"], false);
}

#[test]
fn a_rescan_keeps_what_a_person_set() {
    let app = app();
    let (live, _) = app.drop_two_folders();
    app.run("STUDIO_PRODUCTIONS_SCAN", json!({ "path": "Sessions" }));

    // By hand: a stage, a client, a due date, a new title; and a base edited.
    let path = "Projects/Lucid Dreaming/project.md";
    for (key, value) in [
        ("stage", json!("mastering")),
        ("client", json!("[[Clients/Ada]]")),
        ("due", json!("2026-12-01")),
    ] {
        app.run(
            "VAULT_SET_PROPERTY",
            json!({ "path": path, "key": key, "value": value.to_string() }),
        );
    }
    let before = app.note(path);

    // Nothing changed on disk: nothing is written.
    let again = app.run("STUDIO_PRODUCTIONS_SCAN", json!({ "path": "Sessions" }));
    assert_eq!(again["projects"].as_array().unwrap().len(), 2);
    for p in again["projects"].as_array().unwrap() {
        assert_eq!(p["outcome"], "unchanged", "{p}");
    }
    assert_eq!(again["bases"], json!([]));
    assert_eq!(app.note(path), before);
    assert_eq!(
        by_title(&again["projects"], "Lucid Dreaming")["stage"],
        "mastering"
    );

    // The set is saved again (its time moves): what was read is updated,
    // what the person set stays.
    let set = live.join("Lucid Dreaming.als");
    let earlier = SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&set)
        .unwrap()
        .set_modified(earlier)
        .unwrap();
    let third = app.run("STUDIO_PRODUCTIONS_SCAN", json!({ "path": "Sessions" }));
    let p = by_title(&third["projects"], "Lucid Dreaming");
    assert_eq!(p["outcome"], "updated", "{p}");
    assert_eq!(p["changed"], json!(["changedThisWeek", "lastSaved"]));
    let after = app.note(path);
    for line in [
        "stage: mastering",
        "client: '[[Clients/Ada]]'",
        "due: 2026-12-01",
        "changedThisWeek: false",
    ] {
        assert!(after.contains(line), "no `{line}` in\n{after}");
    }
    assert_eq!(app.rows("Productions/Changed this week.base").len(), 1);
    assert_eq!(
        app.run("STUDIO_PRODUCTIONS_LIST", json!({ "client": "ada" }))[0]["title"],
        "Lucid Dreaming"
    );
    // Still two notes: a re-scan makes no second one.
    assert_eq!(
        app.run("STUDIO_PRODUCTIONS_LIST", json!({}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// The fixture set as a show: three songs marked the way AbleSet reads
/// them, written by the adapter to `to`.
fn write_show_set(to: &Path) {
    let mut set = ableton::read_live_set(fixture(LIVE_SET)).unwrap();
    let mark = |time: f64, name: &str| ableton::Locator {
        time,
        name: name.to_owned(),
    };
    set.locators = vec![
        mark(0.0, "She's American"),
        mark(16.0, ">Chorus"),
        mark(64.0, "SONG END"),
        mark(80.0, "Robbers"),
        mark(160.0, "Medicine: Live"),
    ];
    set.furthest_bar = set.furthest_bar.max(60.0);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    ableton::write_live_set(&set, to).unwrap();
}

#[test]
fn an_ableton_show_set_lists_its_songs_as_a_setlist() {
    let app = app();
    // Outside the vault: the scan takes a folder anywhere on the machine.
    let shows = app.dir.parent().unwrap().join("shows");
    write_show_set(&shows.join("Spring Tour Project/Spring Tour.als"));

    let out = app.run(
        "STUDIO_PRODUCTIONS_SCAN",
        json!({ "path": shows.display().to_string(), "bases": false }),
    );
    let p = by_title(&out["projects"], "Spring Tour");
    assert_eq!(p["songs"], 3, "{out}");
    assert_eq!(p["setlist"], "Setlists/Spring Tour.md");
    assert_eq!(out["bases"], json!([]));

    // The setlist, in the shape setlists keeps: it reads back as one.
    let text = app.note("Setlists/Spring Tour.md");
    let set = setlists::Setlist::from_note(&text).unwrap_or_else(|| panic!("{text}"));
    assert_eq!(set.title, "Spring Tour");
    let songs: Vec<(&str, &str)> = set
        .songs
        .iter()
        .map(|s| (s.library.as_str(), s.title.as_str()))
        .collect();
    assert_eq!(
        songs,
        [
            ("Spring Tour", "She's American"),
            ("Spring Tour", "Robbers"),
            ("Spring Tour", "Medicine- Live"),
        ]
    );
    assert!(
        text.contains("project: '[[Projects/Spring Tour/project|Spring Tour]]'"),
        "{text}"
    );

    // Each song a song note of the set's library, with its tempo.
    let lib = setlists::Library::named("Spring Tour");
    let song = app.note(&lib.song_note("Robbers"));
    let song = setlists::Song::from_note(&lib, &song).unwrap();
    assert!(song.chart.tempo.is_some_and(|t| t > 0.0));

    // The production links to its setlist, and names a file outside the
    // vault as it is on this machine.
    let note = app.note("Projects/Spring Tour/project.md");
    assert!(
        note.contains("setlist: '[[Setlists/Spring Tour|Spring Tour]]'"),
        "{note}"
    );
    assert!(
        note.contains(&format!(
            "location: {}",
            shows.join("Spring Tour Project").display()
        )),
        "{note}"
    );

    // The show re-ordered in Live: the setlist follows on the next scan.
    let mut live =
        ableton::read_live_set(shows.join("Spring Tour Project/Spring Tour.als")).unwrap();
    live.locators.retain(|l| l.name != "Robbers");
    ableton::write_live_set(&live, shows.join("Spring Tour Project/Spring Tour.als")).unwrap();
    app.run(
        "STUDIO_PRODUCTIONS_SCAN",
        json!({ "path": shows.display().to_string(), "bases": false }),
    );
    let set = setlists::Setlist::from_note(&app.note("Setlists/Spring Tour.md")).unwrap();
    let titles: Vec<&str> = set.songs.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["She's American", "Medicine- Live"]);
}

#[test]
fn a_set_with_plain_locators_is_not_a_show_until_a_person_says_so() {
    let app = app();
    let sets = app.dir.parent().unwrap().join("sets");
    let file = sets.join("Demo.als");
    let mut set = ableton::read_live_set(fixture(LIVE_SET)).unwrap();
    set.locators = vec![
        ableton::Locator {
            time: 0.0,
            name: "Verse".to_owned(),
        },
        ableton::Locator {
            time: 32.0,
            name: "Chorus".to_owned(),
        },
    ];
    set.furthest_bar = set.furthest_bar.max(20.0);
    std::fs::create_dir_all(&sets).unwrap();
    ableton::write_live_set(&set, &file).unwrap();

    let scan = json!({ "path": sets.display().to_string(), "bases": false });
    let out = app.run("STUDIO_PRODUCTIONS_SCAN", scan.clone());
    assert!(out["projects"][0]["setlist"].is_null(), "{out}");
    assert!(!app.dir.join("Setlists/Demo.md").exists());

    app.run(
        "VAULT_SET_PROPERTY",
        json!({ "path": "Projects/Demo/project.md", "key": "show", "value": "true" }),
    );
    let out = app.run("STUDIO_PRODUCTIONS_SCAN", scan);
    assert_eq!(out["projects"][0]["setlist"], "Setlists/Demo.md", "{out}");
    assert!(app.dir.join("Setlists/Demo.md").exists());
}

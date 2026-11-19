//! Build a self-contained vault of songs and a setlist from a plan, through
//! the extension's own code, then read it all back and build its share page
//! locally (nothing shared). Local tool for one setlist; not part of Studio.
//!
//! 1. Each song into the library with `chart::import_song` (what
//!    `studio setlist import` runs): chart engraved, LRC, session, media.
//! 2. The setlist with `STUDIO_SETLISTS_CREATE` (first set) and
//!    `STUDIO_SETLISTS_ADD` (`section`: the second set), run by an app's
//!    action list over the vault, as `tests/it/extension.rs` hosts it.
//! 3. Its page: `_ADD` updates a setlist's properties, not its page
//!    (FastTrackStudios/architect#492), so the body is written again with
//!    `Setlist::page` from what the note now says, the properties untouched.
//! 4. Read back: `Vault::songs`, `Setlist::from_note`, `STUDIO_SETLISTS_LIST`;
//!    each chart's bar times as the share page draws them (`follow_chart`)
//!    against the measured bar starts; the share site built with
//!    `share::blocks()`.
//!
//! cargo run -p setlists --example merry_and_bright --features "cli extension share" -- plan.json
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::too_many_lines,
         clippy::indexing_slicing, clippy::panic, clippy::cast_precision_loss)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use architect::action::ActionList;
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::install::Installs;
use architect_settings::Registry;
use futures::executor::block_on;
use serde_json::{Value, json};
use setlists::chart::{SongFiles, import_song};
use setlists::follow::follow_chart;
use setlists::layout::{Library, setlist_note};
use setlists::setlist::Setlist;
use setlists::vault::Vault;

fn run(list: &ActionList, id: &str, args: &Value) -> Value {
    let out = block_on(list.run_json_async(id, args.to_string().as_bytes()))
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_slice(&out).unwrap_or(Value::Null)
}

fn until(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(60), "waited for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_owned()
}

fn main() {
    let plan_path = std::env::args().nth(1).expect("plan.json");
    let plan: Value = serde_json::from_str(&std::fs::read_to_string(&plan_path).unwrap()).unwrap();
    let dir = PathBuf::from(s(&plan, "vault"));
    assert!(
        !dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none(),
        "{} is not empty: a fresh vault only",
        dir.display()
    );
    std::fs::create_dir_all(&dir).unwrap();
    let vault = Vault::at(&dir);
    let lib = Library::named(&s(&plan, "library"));

    // 1. Songs, each with everything of its own.
    let mut imported = Vec::new();
    for song in plan["songs"].as_array().unwrap() {
        let p = |k: &str| song[k].as_str().map(PathBuf::from);
        let chart = p("chart");
        let lrc = p("lrc");
        let session = p("session");
        let media: Vec<PathBuf> = song["media"].as_array().map_or_else(Vec::new, |m| {
            m.iter().map(|x| PathBuf::from(x.as_str().unwrap())).collect()
        });
        let media_refs: Vec<&Path> = media.iter().map(PathBuf::as_path).collect();
        let files = SongFiles {
            chart: chart.as_deref(),
            lrc: lrc.as_deref(),
            session: session.as_deref(),
            media: &media_refs,
            ..SongFiles::default()
        };
        let title = s(song, "title");
        let made = import_song(&vault, &lib, &files, Some(&title)).unwrap();
        println!(
            "song: {} — {} | chart {:?} | pages {} | lrc {:?} | session {:?} | media {:?}",
            made.title,
            made.summary(),
            made.chart.source,
            made.chart.pages.len(),
            made.lrc,
            made.session,
            made.media
        );
        imported.push(made);
    }

    // 2. The setlist, through the extension's actions over this vault.
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
    let installs = tempfile::tempdir().unwrap();
    let ext = Extensions::new(host, Installs::new(installs.path().join("installs")))
        .with_linked(&setlists::ext::Setlists)
        .unwrap();
    ext.install_contribution_actions().unwrap();
    let _ = ext.sync();
    let n_songs = imported.len();
    until("the songs", || {
        run(&list, "STUDIO_SETLISTS_SONGS", &json!({}))
            .as_array()
            .is_some_and(|a| a.len() == n_songs)
    });
    let sl = &plan["setlist"];
    let title = s(sl, "title");
    let sets = sl["sets"].as_array().unwrap();
    let names = |set: &Value| -> Vec<String> {
        set["songs"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_owned()).collect()
    };
    let made = run(
        &list,
        "STUDIO_SETLISTS_CREATE",
        &json!({"title": title, "library": lib.name, "songs": names(&sets[0]), "section": s(&sets[0], "name")}),
    );
    println!("STUDIO_SETLISTS_CREATE -> {made}");
    until("the setlist", || {
        run(&list, "STUDIO_SETLISTS_LIST", &json!({}))
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x["title"] == title.as_str()))
    });
    for set in &sets[1..] {
        for song in names(set) {
            run(
                &list,
                "STUDIO_SETLISTS_ADD",
                &json!({"setlist": title, "library": lib.name, "song": song, "section": s(set, "name")}),
            );
        }
    }
    let total: usize = sets.iter().map(|x| names(x).len()).sum();
    until("the sets", || {
        run(&list, "STUDIO_SETLISTS_LIST", &json!({}))
            .as_array()
            .and_then(|a| a.iter().find(|x| x["title"] == title.as_str()).cloned())
            .is_some_and(|x| x["songs"].as_array().is_some_and(|v| v.len() == total))
    });
    let note_path = setlist_note(&title);
    let from_actions = vault.read(&note_path).unwrap();
    std::fs::write(PathBuf::from(s(&plan, "work")).join("setlist-as-the-actions-left-it.md"), &from_actions).unwrap();

    // 3. Its page from what the note now says (the properties as the actions wrote them).
    let set = Setlist::from_note(&from_actions).expect("a setlist note");
    let known: Vec<_> = vault
        .songs(&lib)
        .into_iter()
        .map(|song| (setlists::layout::SongRef::new(&lib.name, &song.title, None), song))
        .collect();
    let page = set.page(|at| {
        known
            .iter()
            .find(|(r, _)| r.library == at.library && r.title == at.title)
            .map(|(_, song)| setlists::setlist::SongInfo::of(at, song))
    });
    let head_end = from_actions[3..].find("\n---\n").map(|i| i + 3 + 5).expect("frontmatter");
    let rewritten = format!("{}{}", &from_actions[..head_end], page);
    vault.write(&note_path, rewritten.as_bytes()).unwrap();

    // 4. Read back.
    println!("\n== read back ==");
    for song in vault.songs(&lib) {
        println!("  {} [{}] chart={:?} lrc={:?} session={:?} recording={:?}",
                 song.title, song.summary(), song.chart.source, song.lrc, song.session, song.recording());
    }
    let back = Setlist::from_note(&vault.read(&note_path).unwrap()).unwrap();
    for sec in back.sections() {
        println!("  set {:?}: {:?}", sec.name, sec.songs.iter().map(setlists::layout::SongRef::label).collect::<Vec<_>>());
    }
    let listed = run(&list, "STUDIO_SETLISTS_LIST", &json!({}));
    println!("  STUDIO_SETLISTS_LIST: {listed}");

    // Each chart's bars as the share page times them, against the measured starts.
    println!("\n== follow_chart (the share page's bar times) vs measured ==");
    for song in plan["songs"].as_array().unwrap() {
        let Some(measured) = song.get("measured").and_then(Value::as_str) else { continue };
        let t = s(song, "title");
        let lib_song = vault.song(&lib, &t).unwrap();
        let src = vault.read(&lib.song_file(&t, lib_song.chart.source.as_deref().unwrap())).unwrap();
        let rec = lib_song.recording().unwrap().to_owned();
        let fc = follow_chart(&src, Some(&rec)).unwrap();
        let m: Value = serde_json::from_str(&std::fs::read_to_string(measured).unwrap()).unwrap();
        let starts: Vec<f64> = m["bars"].as_array().unwrap().iter().map(|b| b["start"].as_f64().unwrap()).collect();
        let res: Vec<f64> = fc.bars.iter().zip(&starts).map(|(b, x)| (b.start - x).abs() * 1000.0).collect();
        let max = res.iter().copied().fold(0.0_f64, f64::max);
        let mut sorted = res.clone();
        sorted.sort_by(f64::total_cmp);
        let rects = fc.svg.matches("class=\"kf-bar\"").count();
        println!("  {t}: {} bars timed ({} rects, {} pages), media {rec}; vs measured median {:.1} ms, max {:.1} ms",
                 fc.bars.len(), rects, fc.pages, sorted[sorted.len() / 2], max);
    }

    // The share page, built locally as a node builds a share link's site.
    let out = PathBuf::from(s(&plan, "site"));
    std::fs::create_dir_all(&out).unwrap();
    let mut notes = vec![note_path.clone()];
    notes.extend(back.songs.iter().map(|r| lib.song_note(&r.title)));
    let mut opts = architect_sites::Options::new(&dir, &title);
    opts.scope = architect_sites::Scope::Notes(notes);
    let report = architect_sites::build(&opts, &setlists::share::blocks(), &out).unwrap();
    println!("\n== share site (local build) ==\n{report:?}");
}

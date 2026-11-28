//! Each page's data from a home with nothing in it is nothing: the page
//! then shows its honest empty state (how to add the real thing), never
//! made-up content. And from a home laid out as `docs/storage.md` says,
//! each page finds what is really there (fixtures, written by the tests).
//!
//! Each test points `STUDIO_HOME` at a folder of its own before anything
//! reads the home (nextest runs each test in its own process).

use std::path::Path;

use drive_spine::schema;
use loro::LoroDoc;

fn use_home(dir: &Path) {
    // SAFETY: set before any thread reads the environment (each test is
    // its own process under nextest, and this is its first step).
    unsafe { std::env::set_var("STUDIO_HOME", dir) };
    assert_eq!(crate::storage::home().as_deref(), Some(dir));
}

#[test]
fn an_empty_home_shows_nothing_made_up_on_any_page() {
    let home = tempfile::tempdir().expect("a home");
    use_home(home.path());
    // Home: no setlists, no records, no recent songs.
    let work = crate::pages::home::OrgWork::of("adult-jam", &[], &[], &[]);
    assert_eq!(work, crate::pages::home::OrgWork::default());
    assert!(crate::pages::home::recent_songs(6).is_empty());
    // Records.
    assert!(crate::records::on_disk("adult-jam").is_empty());
    // Setlists: no files, and a new org's index has no sets or events.
    #[cfg(feature = "drive")]
    assert!(crate::drive_sets::on_disk("adult-jam").is_empty());
    let index = LoroDoc::new();
    crate::orgs::stamp_org(&index, "adult-jam");
    schema::upgrade(&index, crate::setlist::INDEX_STEPS);
    assert!(crate::setlist::faces(&index).is_empty());
    assert!(crate::setlist::series_list(&index).is_empty());
    // A set opened by a prototype's id is empty.
    let set = LoroDoc::new();
    crate::setlist::stamp_set(&set, "rockstars-of-tomorrow", "jt");
    schema::upgrade(&set, crate::setlist::SETLIST_STEPS);
    assert!(crate::setlist::entries(&set).is_empty());
    // Library.
    let lib = LoroDoc::new();
    crate::orgs::stamp_org(&lib, "adult-jam");
    schema::upgrade(&lib, crate::orgs::LIBRARY_STEPS);
    assert!(crate::songs::entries(&lib).is_empty());
    // People.
    let team = LoroDoc::new();
    crate::orgs::stamp_org(&team, "crossroads-church");
    schema::upgrade(&team, crate::orgs::ORG_STEPS);
    assert!(crate::orgs::people(&team).is_empty());
    // Signal.
    assert!(crate::pages::signal::rig_folders().is_empty());
}

#[test]
fn a_home_laid_out_as_storage_says_shows_its_sets_and_its_record() {
    let home = tempfile::tempdir().expect("a home");
    let h = home.path();
    let write = |rel: &str, text: &str| {
        let p = h.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    };
    write(
        "Setlists/Recital.setlist",
        "# Act I\n= Opening\n= Closing\n",
    );
    write(
        "Setlists/Sunday.setlist",
        "# org: crossroads-church\n= Amazing Grace\n",
    );
    // A record kept where production work goes, its last track a session
    // folder by path (an older line), the rest with no session yet.
    let take = h.join("Elsewhere/Third");
    std::fs::create_dir_all(&take).unwrap();
    std::fs::write(take.join("Third.session"), b"SQLite format 3\0").unwrap();
    write(
        "Projects/Album/Album.setlist",
        &format!(
            "# id: 3f0c6a2e-8b1d-4c57-9e2a-7d4b1f6c9a10\n# kind: studio\n\n= Intro\n= Second\n{}\n",
            take.display()
        ),
    );
    // The same record's older file in Setlists/ is the same record.
    write("Setlists/Album.setlist", "# kind: studio\n= Intro\n");
    std::fs::create_dir_all(h.join("Songs/Opening - Someone")).unwrap();
    use_home(h);

    let records = crate::records::on_disk("codywright");
    assert_eq!(records.len(), 1, "{records:?}");
    let r = &records[0];
    assert_eq!(r.title, "Album");
    assert!(r.file.path.starts_with("Projects/"), "{}", r.file.path);
    let tracks: Vec<(&str, crate::records::TrackState)> = r
        .tracks
        .iter()
        .map(|t| (t.title.as_str(), t.state))
        .collect();
    use crate::records::TrackState::{NoSession, Session};
    assert_eq!(
        tracks,
        vec![
            ("Intro", NoSession),
            ("Second", NoSession),
            ("Third", Session)
        ]
    );

    #[cfg(feature = "drive")]
    {
        let sets = crate::drive_sets::on_disk("codywright");
        let names: Vec<(&str, &str)> = sets
            .iter()
            .map(|h| (h.name.as_str(), h.space.as_str()))
            .collect();
        // The record's file is not a setlist; Sunday is in its own org.
        assert_eq!(
            names,
            vec![
                ("Recital.setlist", "codywright"),
                ("Sunday.setlist", "crossroads-church")
            ]
        );
        let work = crate::pages::home::OrgWork::of("codywright", &[], &sets, &records);
        assert_eq!(work.files.len(), 1);
        assert_eq!(work.records.len(), 1);
    }

    let recent = crate::pages::home::recent_songs(6);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].title, "Opening");
    assert_eq!(recent[0].artist.as_deref(), Some("Someone"));
}

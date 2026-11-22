use super::*;
use crate::songs::Entry;

fn song(id: &str, title: &str, shared: &[&str]) -> Entry {
    Entry {
        id: id.into(),
        title: title.into(),
        artist: Some("Ron Kenoli".into()),
        shared_with: shared.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn nl(location: &str, path: &str, dir: bool) -> NewLink {
    NewLink {
        space: "crossroads-church".into(),
        location: location.into(),
        path: path.into(),
        dir,
        kind: None,
    }
}

#[test]
fn files_are_recognised_by_extension_and_name() {
    let cases: &[(&str, bool, FileKind)] = &[
        (
            "Charts/Goodness of God - Master Rhythm.pdf",
            false,
            FileKind::ChartPdf,
        ),
        ("Goodness of God.kf", false, FileKind::Keyflow),
        ("Goodness of God.musicxml", false, FileKind::Keyflow),
        ("Goodness of God.lrc", false, FileKind::Lyrics),
        ("Goodness of God lyrics.txt", false, FileKind::Lyrics),
        ("notes.txt", false, FileKind::Other),
        ("Goodness of God.mid", false, FileKind::Midi),
        ("Sessions/GOG/GOG.rpp", false, FileKind::Session),
        ("GOG 2.7.ptx", false, FileKind::Session),
        ("GOG.als", false, FileKind::Session),
        ("Song.logicx", true, FileKind::Session),
        ("Video/GOG lyric video.mp4", false, FileKind::Video),
        ("MT/Click.wav", false, FileKind::Click),
        (
            "Goodness Of God 61 BPM Village Stems/CLK.wav",
            false,
            FileKind::Click,
        ),
        ("MT/Guide.wav", false, FileKind::Guide),
        ("MT/Cues.wav", false, FileKind::Guide),
        (
            "Goodness of God - Backing Track.mp3",
            false,
            FileKind::Backing,
        ),
        (
            "Goodness of God (Instrumental).wav",
            false,
            FileKind::Backing,
        ),
        (
            "Goodness Of God 61 BPM Village Stems/Drums.wav",
            false,
            FileKind::Stems,
        ),
        (
            "Goodness of God Multitracks/Kick In.wav",
            false,
            FileKind::Multitrack,
        ),
        ("Audio Files/Snare Top_03.wav", false, FileKind::Multitrack),
        ("Recording/Kick In.wav", false, FileKind::Multitrack),
        ("Recording/Bass.wav", false, FileKind::Stems),
        (
            "Bounced Files/AMAZING GRACE Re-Master 2022.wav",
            false,
            FileKind::Reference,
        ),
        ("GOG rough mix.mp3", false, FileKind::Reference),
        ("Goodness of God.wav", false, FileKind::Reference),
        (
            "Goodness Of God 61 BPM Village Stems",
            true,
            FileKind::Stems,
        ),
        ("Goodness of God Multitracks", true, FileKind::Multitrack),
        ("Audio Files", true, FileKind::Multitrack),
        ("Charts", true, FileKind::ChartPdf),
        ("GOODNESS OF GOD PRO TOOLS", true, FileKind::Session),
        ("Bounced Files", true, FileKind::Reference),
        ("Goodness of God - Ron Kenoli", true, FileKind::Other),
        ("setup.exe", false, FileKind::Other),
    ];
    for (path, dir, want) in cases {
        assert_eq!(recognise(path, *dir), *want, "{path}");
    }
    // By what is in it, when the name says nothing.
    let mt: Vec<(&str, bool)> = [
        "Kick In.wav",
        "Kick Out.wav",
        "Snare Top.wav",
        "OH L.wav",
        "Bass DI.wav",
    ]
    .iter()
    .map(|n| (*n, false))
    .collect();
    assert_eq!(recognise_folder("GOG 4.4", &mt), FileKind::Multitrack);
    let stems = [
        ("Drums.wav", false),
        ("Bass.wav", false),
        ("Keys.wav", false),
        ("Vocals.wav", false),
    ];
    assert_eq!(recognise_folder("GOG", &stems), FileKind::Stems);
    let sess = [("GOG.ptx", false), ("Audio Files", true)];
    assert_eq!(recognise_folder("GOG", &sess), FileKind::Session);
    assert_eq!(
        recognise_folder("GOG", &[("Click.wav", false)]),
        FileKind::Click
    );
    assert_eq!(
        recognise_folder("GOG", &[("a.txt", false)]),
        FileKind::Other
    );
    // Ids and names round-trip; labels parse back.
    for k in FileKind::ALL {
        assert_eq!(FileKind::parse(k.id()), Some(k));
        assert_eq!(FileKind::parse(k.name()), Some(k));
    }
}

/// Every name, size, byte, time and permission under `dir`.
fn snapshot(dir: &std::path::Path) -> Vec<(String, Vec<u8>, std::time::SystemTime, bool)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            let md = e.metadata().unwrap();
            if md.is_dir() {
                stack.push(e.path());
                out.push((
                    e.path().display().to_string(),
                    Vec::new(),
                    md.modified().unwrap(),
                    md.permissions().readonly(),
                ));
            } else {
                out.push((
                    e.path().display().to_string(),
                    std::fs::read(e.path()).unwrap(),
                    md.modified().unwrap(),
                    md.permissions().readonly(),
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn linking_leaves_files_where_they_are() {
    // A location's folder on this computer: a multitrack, read-only.
    let root = std::env::temp_dir().join(format!(
        "studio-proto-files-{}-{}",
        std::process::id(),
        drive_files::now_ms()
    ));
    let mt = root.join("Goodness of God Multitracks");
    std::fs::create_dir_all(&mt).unwrap();
    for (n, b) in [
        ("Click.wav", &b"RIFFclick"[..]),
        ("Kick In.wav", b"RIFFkick"),
    ] {
        std::fs::write(mt.join(n), b).unwrap();
        let mut p = std::fs::metadata(mt.join(n)).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(mt.join(n), p).unwrap();
    }
    let before = snapshot(&root);

    let (reg, meta) = (LoroDoc::new(), LoroDoc::new());
    let e = song("goodness-of-god", "Goodness of God", &[]);
    let org = "crossroads-church";
    let folder = link(
        &reg,
        &meta,
        org,
        &e,
        &nl("loc-mt", "Goodness of God Multitracks", true),
    )
    .unwrap();
    let file = link(
        &reg,
        &meta,
        org,
        &e,
        &nl("loc-mt", "Goodness of God Multitracks/Click.wav", false),
    )
    .unwrap();
    // Linking the same thing twice is the same part.
    assert_eq!(
        link(
            &reg,
            &meta,
            org,
            &e,
            &nl("loc-mt", "/Goodness of God Multitracks/", true)
        ),
        Some(folder.clone())
    );
    let ls = links(&reg, &meta, &e.id, org);
    assert_eq!(ls.len(), 2);
    let f = ls.iter().find(|l| l.part == folder).unwrap();
    assert_eq!(
        (f.kind, f.dir, f.space.as_str()),
        (FileKind::Multitrack, true, org)
    );
    assert_eq!(f.target(), "loc-mt:Goodness of God Multitracks");
    // The project's parts point at the files where they are.
    let p = project(&reg, &e.id).unwrap();
    assert_eq!(p.kind, "MusicRecording");
    assert_eq!(p.name, "Goodness of God");
    assert!(p.parts.iter().all(|x| x.location == "loc-mt"));
    assert_eq!(
        p.docs,
        [
            "studio:files:goodness-of-god",
            "studio:song:goodness-of-god"
        ]
    );

    unlink(&reg, &meta, &e.id, &file);
    unlink(&reg, &meta, &e.id, &folder);
    assert!(links(&reg, &meta, &e.id, org).is_empty());
    // Nothing on disk changed: no file moved, copied, written or deleted.
    assert_eq!(snapshot(&root), before);
    for n in ["Click.wav", "Kick In.wav"] {
        let mut p = std::fs::metadata(mt.join(n)).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        p.set_readonly(false);
        std::fs::set_permissions(mt.join(n), p).unwrap();
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn nothing_here_touches_files() {
    // The song-files code writes Drive's and Studio's documents only:
    // nothing that creates, writes, moves or deletes a file.
    let banned = [
        "fs::write",
        "fs::remove",
        "fs::rename",
        "fs::copy",
        "remove_file",
        "remove_dir",
        "OpenOptions",
        "File::create",
        "set_permissions",
        "/trash",
    ];
    let model = include_str!("../files.rs");
    let live = include_str!("../files_live.rs");
    let page = include_str!("../pages/files.rs");
    for (name, src) in [
        ("files.rs", model),
        ("files_live.rs", live),
        ("pages/files.rs", page),
    ] {
        for b in banned {
            assert!(!src.contains(b), "{name} has {b}");
        }
    }
}

#[test]
fn a_song_shares_its_files_with_the_orgs_it_is_shared_with() {
    let (reg, meta) = (LoroDoc::new(), LoroDoc::new());
    let org = "crossroads-church";
    let mut e = song("goodness-of-god", "Goodness of God", &["days-to-praise"]);
    link(&reg, &meta, org, &e, &nl("loc", "GOG/Click.wav", false)).unwrap();
    assert_eq!(
        project(&reg, &e.id).unwrap().shared_with,
        ["days-to-praise"]
    );

    // Shared with one more org, and renamed: the project follows.
    e.shared_with.push("cbu".into());
    e.title = "Goodness of God (live)".into();
    let other = song("other", "Other", &["cbu"]);
    let lib = [e.clone(), other.clone()];
    assert!(projects_differ(&reg, &lib));
    assert!(mirror_projects(&reg, &lib));
    assert!(!projects_differ(&reg, &lib));
    assert!(!mirror_projects(&reg, &lib), "nothing left to change");
    let p = project(&reg, &e.id).unwrap();
    assert_eq!(p.shared_with, ["cbu", "days-to-praise"]);
    assert_eq!(p.name, "Goodness of God (live)");
    // A song without files gets no project.
    assert!(project(&reg, &other.id).is_none());

    // What the other org sees: Drive's project (as `/shared` lists it) and
    // the owner's notes document, which the project holds.
    let seen = links_of(&p.parts, &meta, org);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].kind, FileKind::Click);
    // Without the notes, the part's label says the kind.
    assert_eq!(
        links_of(&p.parts, &LoroDoc::new(), org)[0].kind,
        FileKind::Click
    );

    // Unshared: the project is no longer shared with it.
    e.shared_with = vec!["cbu".into()];
    mirror_projects(&reg, &[e.clone()]);
    assert_eq!(project(&reg, &e.id).unwrap().shared_with, ["cbu"]);
    e.shared_with.clear();
    mirror_projects(&reg, &[e.clone()]);
    assert!(project(&reg, &e.id).unwrap().shared_with.is_empty());
}

#[test]
fn kinds_can_be_changed_and_other_orgs_computers_noted() {
    let (reg, meta) = (LoroDoc::new(), LoroDoc::new());
    let org = "crossroads-church";
    let e = song("gog", "Goodness of God", &[]);
    // A folder on the person's own computer (their personal org's).
    let part = link(
        &reg,
        &meta,
        org,
        &e,
        &NewLink {
            space: "codywright".into(),
            location: "audiohaven".into(),
            path: "Project/PNG Worship Collective Album/11 GOODNESS OF GOD - RK version".into(),
            dir: true,
            kind: None,
        },
    )
    .unwrap();
    let l = &links(&reg, &meta, &e.id, org)[0];
    assert_eq!((l.space.as_str(), l.kind), ("codywright", FileKind::Other));
    set_link_kind(&reg, &meta, &e.id, &part, FileKind::Multitrack);
    assert_eq!(links(&reg, &meta, &e.id, org)[0].kind, FileKind::Multitrack);
    assert_eq!(project(&reg, &e.id).unwrap().parts[0].label, "Multitrack");

    // One file inside it is something else.
    let t = "audiohaven:Project/PNG/11 GOG/Click Track.wav";
    assert_eq!(file_kind(&meta, t), None);
    set_file_kind(&meta, t, Some(FileKind::Guide));
    assert_eq!(file_kind(&meta, t), Some(FileKind::Guide));
    set_file_kind(&meta, t, None);
    assert_eq!(file_kind(&meta, t), None);

    // Kept on the node, then not.
    set_available(&reg, &e.id, std::slice::from_ref(&part), true);
    assert!(links(&reg, &meta, &e.id, org)[0].available);
    set_available(&reg, &e.id, &[part], false);
    assert!(!links(&reg, &meta, &e.id, org)[0].available);

    set_alias(&meta, "GOG", true);
    set_alias(&meta, "Bondad de Dios", true);
    assert_eq!(aliases(&meta), ["Bondad de Dios", "GOG"]);
    set_alias(&meta, "GOG", false);
    assert_eq!(aliases(&meta), ["Bondad de Dios"]);
}

fn hit(loc: &str, dev: &str, online: bool, path: &str, dir: bool, size: i64) -> Hit {
    Hit {
        space: "codywright".into(),
        location: loc.into(),
        location_name: loc.into(),
        device_name: dev.into(),
        online,
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        dir,
        size,
        sampled: if dir {
            String::new()
        } else {
            format!("s{size}")
        },
        copies: 1,
        ..Default::default()
    }
}

#[test]
fn finding_a_songs_files_proposes_the_best_first() {
    assert_eq!(
        queries("Goodness of God (RK version)", &["Bondad de Dios".into()]),
        ["goodness god rk version", "goodness god", "bondad de dios"]
    );
    assert_eq!(
        queries("This Is Amazing Grace", &[]),
        ["this amazing grace"]
    );

    let task = "tombrooksmusic/Projects";
    let hits = vec![
        // The project's own folder, on the share and its mirror.
        hit(
            "task",
            "Starcommand",
            false,
            &format!("{task}/Goodness of God - Ron Kenoli, Belen Jose, Aline Barros"),
            true,
            0,
        ),
        hit(
            "task-mirror",
            "THEBATTLESHIP",
            true,
            &format!("{task}/Goodness of God - Ron Kenoli, Belen Jose, Aline Barros"),
            true,
            0,
        ),
        // Deep inside an inbox, an old copy.
        hit(
            "task",
            "Starcommand",
            false,
            &format!("{task}/El Artista/Z - Inbox/Copy of X/Desktop/GOODNESS of GOD - Belen-Josue"),
            true,
            0,
        ),
        // A file inside the project folder: the folder covers it.
        hit(
            "task",
            "Starcommand",
            false,
            &format!(
                "{task}/Goodness of God - Ron Kenoli, Belen Jose, Aline Barros/Mix/Goodness of God.wav"
            ),
            false,
            900,
        ),
        // Stems, by name.
        hit(
            "audiohaven",
            "THEBATTLESHIP",
            true,
            "Project/Goodness Of God 61 BPM Village Stems",
            true,
            0,
        ),
        // The same chart PDF in two places, by content.
        hit(
            "charts",
            "THEBATTLESHIP",
            true,
            "Charts/Goodness of God.pdf",
            false,
            120,
        ),
        hit(
            "charts2",
            "Voyager",
            false,
            "Backup/Goodness of God.pdf",
            false,
            120,
        ),
        // Nothing to do with the song.
        hit(
            "audiohaven",
            "THEBATTLESHIP",
            true,
            "Project/Amazing Grace.wav",
            false,
            5,
        ),
    ];
    let linked = vec![Link {
        part: "p".into(),
        kind: FileKind::Stems,
        space: "codywright".into(),
        location: "audiohaven".into(),
        path: "Project/Goodness Of God 61 BPM Village Stems".into(),
        dir: true,
        available: false,
    }];
    let p = propose("Goodness of God", Some("Ron Kenoli"), &[], &hits, &linked);
    let names: Vec<(&str, &str, usize)> = p
        .iter()
        .map(|x| (x.hit.location.as_str(), x.hit.name.as_str(), x.also.len()))
        .collect();
    assert_eq!(
        names,
        [
            // The online copy of the project folder first, its mirror folded in.
            (
                "task-mirror",
                "Goodness of God - Ron Kenoli, Belen Jose, Aline Barros",
                1
            ),
            ("charts", "Goodness of God.pdf", 1),
            ("task", "GOODNESS of GOD - Belen-Josue", 0),
        ]
    );
    assert_eq!(p[1].kind, FileKind::ChartPdf);
    assert!(p[0].score > p[2].score);
    assert!(p[0].why.contains("artist"), "{}", p[0].why);
    // An alias finds what the title does not.
    let es = [hit("x", "X", true, "Bondad de Dios/Click.wav", false, 3)];
    assert!(propose("Goodness of God", None, &[], &es, &[]).is_empty());
    assert_eq!(
        propose(
            "Goodness of God",
            None,
            &["Bondad de Dios".into()],
            &es,
            &[]
        )
        .len(),
        1
    );
}

#[test]
fn the_search_language_narrows_what_the_node_finds() {
    let t = typed("grace kind:audio path:Multitracks/")
        .unwrap()
        .unwrap();
    assert!(!t.plain);
    assert!(t.exts.iter().any(|e| e == "wav"));
    assert!(t.queries.iter().any(|q| q.contains("grace")));
    let yes = hit(
        "l",
        "D",
        true,
        "Multitracks/Amazing Grace/Click.wav",
        false,
        1,
    );
    let no_kind = hit(
        "l",
        "D",
        true,
        "Multitracks/Amazing Grace/chart.pdf",
        false,
        1,
    );
    let no_path = hit("l", "D", true, "Mixes/Amazing Grace.wav", false, 1);
    // `grace` names no tag: a name search on the hit's name.
    let named = hit("l", "D", true, "Multitracks/Grace Click.wav", false, 1);
    assert!(!t.matches(&yes), "the name has no `grace`");
    assert!(t.matches(&named));
    assert!(!t.matches(&no_kind));
    assert!(!t.matches(&no_path));
    let t = typed("goodness NOT backup").unwrap().unwrap();
    assert!(t.matches(&hit("l", "D", true, "A/Goodness.wav", false, 1)));
    assert!(!t.matches(&hit("l", "D", true, "A/Goodness backup.wav", false, 1)));
    assert!(typed("(goodness").is_err());
    assert!(typed("  ").unwrap().is_none());
    assert!(typed("goodness of god").unwrap().unwrap().plain);
}

fn st(
    kind: FileKind,
    size: i64,
    copies: &[(&str, bool)],
    on_node: bool,
    here: Option<bool>,
) -> FileState {
    FileState {
        target: format!("l:{size}"),
        name: String::new(),
        size,
        kind: Some(kind),
        copies: copies
            .iter()
            .map(|(d, on)| Copy {
                device_name: d.to_string(),
                location_name: String::new(),
                online: *on,
            })
            .collect(),
        on_node,
        here,
        missing: false,
    }
}

#[test]
fn readiness_is_the_worst_file_and_the_worst_song() {
    let click = st(
        FileKind::Click,
        10,
        &[("Mac Pro", false), ("THEBATTLESHIP", true)],
        false,
        None,
    );
    let chart = st(FileKind::ChartPdf, 2, &[], true, None);
    let stems = st(FileKind::Stems, 500, &[("Mac Pro", false)], false, None);
    assert_eq!(file_ready(&click), Ready::Reachable);
    assert_eq!(file_ready(&chart), Ready::Kept);
    assert_eq!(file_ready(&stems), Ready::Offline);
    let mut gone = stems.clone();
    gone.missing = true;
    gone.copies.clear();
    assert_eq!(file_ready(&gone), Ready::Missing);
    let mut here = stems.clone();
    here.here = Some(true);
    assert_eq!(file_ready(&here), Ready::Here);

    // A song: its worst file; what it lacks; what warming would bring.
    let a = song_ready(
        &[click.clone(), chart.clone(), stems.clone()],
        &wanted(true),
    );
    assert_eq!(a.state, Some(Ready::Offline));
    assert_eq!(a.needs, ["Mac Pro"]);
    assert!(a.lacks.is_empty());
    assert_eq!((a.to_warm, a.bytes_to_warm), (2, 510));
    let b = song_ready(&[stems.clone()], &wanted(true));
    assert_eq!(b.lacks, [FileKind::ChartPdf], "stems stand in for a click");
    let c = song_ready(&[chart.clone()], &wanted(true));
    assert_eq!(c.lacks, [FileKind::Click]);
    assert_eq!(c.state, Some(Ready::Kept));
    let none = song_ready(&[], &wanted(false));
    assert_eq!(
        (none.state, none.lacks.clone()),
        (None, vec![FileKind::ChartPdf])
    );

    // On a device: warmed files count as here.
    let warmed: Vec<FileState> = [click, chart, stems]
        .into_iter()
        .map(|mut f| {
            f.here = Some(true);
            f
        })
        .collect();
    let d = song_ready(&warmed, &wanted(true));
    assert_eq!((d.state, d.to_warm), (Some(Ready::Here), 0));

    let set = set_ready(&[a, c.clone(), none, d]);
    assert_eq!(set.songs, 4);
    assert_eq!(set.without_files, 1);
    assert_eq!(set.state, Some(Ready::Offline));
    assert_eq!(set.ready_offline, 2);
    assert_eq!(set.needs, ["Mac Pro"]);
    assert_eq!(set.bytes_to_warm, 510);
    let ok = set_ready(&[c]);
    assert_eq!((ok.state, ok.ready_offline), (Some(Ready::Kept), 1));
}

#[test]
fn uploads_go_to_the_songs_folder() {
    assert_eq!(upload_folder("AC/DC: Live"), "Studio/Songs/AC-DC- Live");
    assert_eq!(
        target(TREE, "/Studio/Songs/X/a.lrc"),
        "drive:Studio/Songs/X/a.lrc"
    );
    assert_eq!(bytes(1_500_000), "2 MB");
    assert_eq!(bytes(2_340_000_000), "2.3 GB");
}

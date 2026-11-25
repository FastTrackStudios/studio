//! The `frame.surface` kind: declared once, named by `frame-surface`'s
//! manifest, and refused by an app that has no provider for it; and the
//! surface it shows: a document's, its params and feeds the ones its
//! controls bind, painted as SVG that follows them, its knob dragged, a
//! save to its file reloading it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use architect_extensions::{Kind, Manifest};
use architect_workspace::{KindError, Placement, ViewHost, ViewKinds};
use frame::embed::{Button, Modifiers, Pointer};
use frame_view::shown;
use frame_view::surface::{DOCUMENT, Live};
use frame_view::svg::Node;

#[test]
fn the_kind_registers_once() {
    let mut kinds = ViewKinds::new();
    frame_view::register(&mut kinds).unwrap();
    let spec = kinds
        .resolve_for(frame_view::KIND, ViewHost::Blitz, Placement::Pane)
        .unwrap();
    assert_eq!(spec.provider, frame_view::PROVIDER);
    assert!(matches!(
        frame_view::register(&mut kinds),
        Err(KindError::Taken { .. })
    ));
}

#[test]
fn the_extension_shows_a_view_of_the_kind() {
    let manifest = Manifest::parse(frame_view::MANIFEST).unwrap();
    manifest.check().unwrap();
    assert_eq!(manifest.kind, Kind::Process);
    let [view] = manifest.views.as_slice() else {
        panic!("not one view: {:?}", manifest.views);
    };
    assert_eq!(view.kind, frame_view::KIND);

    // Refused where nobody provides it, shown where it is declared.
    let err = ViewKinds::new().resolve(&view.kind).unwrap_err();
    assert!(matches!(err, KindError::Unknown { .. }));
    let mut kinds = ViewKinds::new();
    frame_view::register(&mut kinds).unwrap();
    kinds
        .resolve_for(&view.kind, ViewHost::Blitz, Placement::Pane)
        .unwrap();
}

fn paths(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Path(_) => 1,
            Node::Group { children, .. } => paths(children),
        })
        .sum()
}

#[test]
fn the_document_says_what_moves_it() {
    let surface = Live::new(DOCUMENT, 212.0).unwrap();
    let params = surface.params();
    let [drive] = params.as_slice() else {
        panic!("not one param: {params:?}");
    };
    assert_eq!(
        (drive.addr.as_str(), drive.name.as_str()),
        ("surface/drive", "Drive")
    );
    assert!(
        (drive.value - 0.5).abs() < 1e-9,
        "at its default: {drive:?}"
    );
    let feeds = surface.feeds();
    let [level] = feeds.as_slice() else {
        panic!("not one feed: {feeds:?}");
    };
    assert_eq!((level.id.as_str(), level.name.as_str()), ("level", "Level"));
    assert_eq!(level.values, [frame_view::surface::FLOOR_DB]);
}

#[test]
fn the_surface_paints_its_params_and_feeds() {
    let mut surface = Live::new(DOCUMENT, 212.0).unwrap();
    let svg = surface.svg();
    assert_eq!(svg.width, 212.0);
    assert!(
        svg.height > 200.0,
        "laid out at its document's aspect: {svg:?}"
    );
    assert!(paths(&svg.nodes) > 5, "too little painted: {svg:?}");
    let markup = svg.markup();
    assert!(markup.starts_with("<svg "), "{markup}");
    // The document's colours: the knob's value arc; the meter at its
    // floor has no bar, a reading gives it one.
    assert!(markup.contains("#ff7a1a"), "no value arc: {markup}");
    assert!(!markup.contains("#3dd67a"), "a bar at the floor: {markup}");

    surface.feed("level", &[-20.0]).unwrap();
    let before = surface.svg().markup();
    assert!(before.contains("#3dd67a"), "no meter: {before}");
    surface.set_param("surface/drive", 0.9).unwrap();
    assert!((surface.param_real("surface/drive").unwrap() - 90.0).abs() < 1e-6);
    let knob_moved = surface.svg().markup();
    assert_ne!(before, knob_moved, "the knob did not move");

    surface.feed("level", &[-54.0]).unwrap();
    let meter_fell = surface.svg().markup();
    assert_ne!(knob_moved, meter_fell, "the meter did not fall");

    // The same values paint the same surface.
    surface.feed("level", &[-20.0]).unwrap();
    assert_eq!(surface.svg().markup(), knob_moved);

    // What it does not have is refused, by name.
    let err = surface.set_param("surface/tone", 0.5).unwrap_err();
    assert!(err.contains("surface/drive"), "{err}");
    let err = surface.feed("gain", &[0.0]).unwrap_err();
    assert!(err.contains("level"), "{err}");
    assert!(surface.feed("level", &[]).is_err());
}

#[test]
fn dragging_the_knob_up_turns_it_up() {
    let mut surface = Live::new(DOCUMENT, 212.0).unwrap();
    let (w, _) = surface.size();
    // The dial's middle: the document's 106 px drawn 212 wide.
    let k = w / 106.0;
    let (x, y) = (36.0 * k, 56.0 * k);
    let before = surface.param("surface/drive").unwrap();
    surface.pointer(Pointer::Down {
        x,
        y,
        button: Button::Left,
        mods: Modifiers::default(),
    });
    let mut moved = Vec::new();
    for dy in 1..=8 {
        moved.extend(surface.pointer(Pointer::Move {
            x,
            y: y - f64::from(dy) * 8.0,
        }));
    }
    moved.extend(surface.pointer(Pointer::Up {
        x,
        y: y - 64.0,
        button: Button::Left,
    }));
    let after = surface.param("surface/drive").unwrap();
    assert!(after > before + 0.05, "{before} → {after}");
    assert_eq!(
        moved.last().map(|(addr, v)| (addr.as_str(), *v)),
        Some(("surface/drive", after)),
        "the drag says what it moved: {moved:?}"
    );
}

#[test]
fn a_reload_keeps_what_stays_and_takes_what_is_new() {
    let mut surface = Live::new(DOCUMENT, 212.0).unwrap();
    surface.set_param("surface/drive", 0.8).unwrap();
    surface.feed("level", &[-6.0]).unwrap();
    // A second knob, bound to a param of its own.
    let two = DOCUMENT.replacen("surface/drive", "surface/tone", 1);
    let two = two.replace(r#"<Frame name="Drive""#, r#"<Frame name="Tone""#);
    surface.reload(&two).unwrap();
    let params = surface.params();
    assert_eq!(params.len(), 1, "{params:?}");
    assert_eq!(params[0].addr, "surface/tone");
    assert_eq!(params[0].name, "Tone");
    assert_eq!(surface.feeds()[0].values, [-6.0], "the feed stays");

    surface.reload(DOCUMENT).unwrap();
    assert!(
        (surface.param("surface/drive").unwrap() - 0.5).abs() < 1e-9,
        "a param that went and came back is new"
    );
    // A document that does not parse leaves it as it was.
    assert!(surface.reload("<Document").is_err());
    assert_eq!(surface.params()[0].addr, "surface/drive");
}

/// One test: [`shown`] is the process's one surface.
#[test]
fn a_loaded_document_reloads_when_it_is_saved() {
    let dir = std::env::temp_dir().join(format!("frame-view-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("drive.frame");
    std::fs::write(&path, DOCUMENT).unwrap();

    let state = shown::load(&path).unwrap();
    assert_eq!(state["document"], path.display().to_string());
    assert_eq!(state["loads"], 1);
    assert_eq!(state["error"], serde_json::Value::Null);
    let state = shown::set_param("surface/drive", 0.75).unwrap();
    assert_eq!(state["params"][0]["value"], 0.75);
    let state = shown::feed("level", &[-12.0]).unwrap();
    assert_eq!(state["feeds"][0]["values"][0], -12.0);
    assert!(!shown::check(), "nothing saved yet");

    // A save: the knob's label colour changes, the values stay.
    let saved = DOCUMENT.replace("#9aa0ab", "#ffffff");
    let before = shown::paint().unwrap().markup();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&path, &saved).unwrap();
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(1))
        .unwrap();
    drop(file);
    assert!(shown::check(), "the save was seen");
    let state = shown::state();
    assert_eq!(state["loads"], 2);
    assert_eq!(state["params"][0]["value"], 0.75);
    assert_ne!(
        shown::paint().unwrap().markup(),
        before,
        "redrawn from the save"
    );

    // A save that does not parse: kept as it was, and why.
    std::fs::write(&path, "<Document").unwrap();
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(2))
        .unwrap();
    drop(file);
    assert!(shown::check());
    let state = shown::state();
    assert_eq!(state["loads"], 2);
    assert!(
        state["error"]
            .as_str()
            .is_some_and(|e| e.contains("does not parse")),
        "{state}"
    );
    assert_eq!(state["params"][0]["value"], 0.75);

    // A load that cannot be read keeps the one that was.
    assert!(shown::load(&dir.join("missing.frame")).is_err());
    assert_eq!(shown::state()["document"], path.display().to_string());
    let _ = std::fs::remove_dir_all(&dir);
}

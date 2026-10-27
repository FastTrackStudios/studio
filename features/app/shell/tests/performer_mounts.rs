//! The performer screen as Session mounts it (STA-360, B3), headless on
//! Blitz at the 11-inch iPad's 1194 x 834 — the rig dock behind a fake
//! [`RigClient`], the compact transport's Go to start / Go to end, and the
//! one panel registry both apps go through:
//!
//! - a reachable rig: ten switches, and a press reaches the client;
//! - the rig drops: the switches dim and take no press, and no region of
//!   the screen moves (to 0 px);
//! - no rig ever paired: the transport spans 1170 px, the app bar says
//!   Pair a rig;
//! - the now band carries the clock, bar.beat, BPM and key, and the
//!   transport's ⋯ holds Go to start and Go to end (the iPad audit's fix 7);
//! - neither app builds the performer panels itself: both call
//!   `performer_registry` (read from their sources).
//!
//! `PERFORMER_SHOT=<path>.png` writes the screenshots (rig, rig offline,
//! no rig) beside it.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use app_shell::ui::perform::{NowBand, SyncPill, TransportNudgeRow};
use app_shell::ui::performer::{self, PerformerBodies, SESSION_SCOPE, performer_registry};
use app_shell::ui::playback::{PlaybackModel, PlaybackView, SectionView, SongView};
use app_shell::ui::rig::{RigClient, RigLink, SwitchNo};
use app_shell::ui::{
    PerformDock, PerformerScreen, RigCrumb, provide_playback, provide_rig, rig_half,
    use_panel_registry,
};
use dioxus::prelude::*;
use dioxus_test::DocumentTester;

// ── Fakes ──────────────────────────────────────────────────────────────────

/// What the fakes were asked to do, read back by the tests.
#[derive(Default, Debug)]
struct Calls {
    presses: Vec<SwitchNo>,
    reconnects: u32,
    pairs: u32,
    starts: u32,
    ends: u32,
}

thread_local! {
    static CALLS: RefCell<Calls> = RefCell::new(Calls::default());
    /// The link the next screen opens with.
    static OPENS: RefCell<RigLink> = const { RefCell::new(RigLink::Unpaired) };
}

fn calls<T>(f: impl FnOnce(&Calls) -> T) -> T {
    CALLS.with(|c| f(&c.borrow()))
}

fn reset(link: RigLink) {
    CALLS.with(|c| *c.borrow_mut() = Calls::default());
    OPENS.with(|o| *o.borrow_mut() = link);
}

/// A rig on a peer: its link a signal the test flips.
struct FakeRig {
    link: Signal<RigLink>,
}

impl RigClient for FakeRig {
    fn link(&self) -> RigLink {
        (self.link)()
    }
    fn press(&self, switch: SwitchNo) {
        CALLS.with(|c| c.borrow_mut().presses.push(switch));
    }
    fn reconnect(&self) {
        CALLS.with(|c| c.borrow_mut().reconnects += 1);
    }
    fn pair(&self) {
        CALLS.with(|c| c.borrow_mut().pairs += 1);
    }
    fn switches(&self) -> Element {
        let link = self.link;
        // Ten tiles at the Gig size (two 60 px rows of five, 8 apart), as
        // Signal's TouchSwitches draws them; each press goes to the client.
        rsx! {
            div { style: "display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); grid-template-rows: 60px 60px; gap: 8px; height: 128px;",
                for no in [6u8, 7, 8, 9, 10, 1, 2, 3, 4, 5] {
                    button {
                        key: "{no}",
                        "data-switch": "{no}",
                        "aria-label": "Switch {no}",
                        style: "border: none; border-radius: 8px; background: #1b1b20; color: #e4e4e7;",
                        onclick: move |_| FakeRig { link }.press(no),
                        "{no}"
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct FakePlayback {
    view: Rc<RefCell<PlaybackView>>,
}

impl PlaybackModel for FakePlayback {
    fn view(&self) -> PlaybackView {
        self.view.borrow().clone()
    }
    fn step_song(&self, _by: i32) {}
    fn load_song(&self, _index: usize) {}
    fn seek(&self, seconds: f64) {
        self.view.borrow_mut().position = seconds;
    }
    fn play_pause(&self) {}
    fn section_step(&self, _by: i32) {}
    fn set_loop(&self, _on: bool) {}
    fn set_speed(&self, _rate: f64) {}
    fn set_level(&self, _level: f64) {}
    fn can_go_to(&self) -> bool {
        true
    }
    fn go_to_start(&self) {
        CALLS.with(|c| c.borrow_mut().starts += 1);
    }
    fn go_to_end(&self) {
        CALLS.with(|c| c.borrow_mut().ends += 1);
    }
}

fn view() -> PlaybackView {
    let s = |name: &str, colour: &str, start: f64, end: f64| SectionView {
        name: name.into(),
        colour: colour.into(),
        start,
        end,
    };
    let song = |title: &str, key: &str, bpm: u32, colour: &str| SongView {
        title: title.into(),
        key: key.into(),
        bpm,
        colour: colour.into(),
        sections: vec![
            s("INT", "#2563eb", 0.0, 16.0),
            s("VER 1", "#7c3aed", 16.0, 64.0),
            s("CHO", "#0f766e", 64.0, 112.0),
            s("BRI", "#3f6212", 112.0, 160.0),
            s("CHO", "#0c4a6e", 160.0, 248.0),
        ],
    };
    PlaybackView {
        songs: vec![
            song("Harbor Light", "G", 72, "#7dd3fc"),
            song("Lantern Hymn", "E", 98, "#facc15"),
            song("Morning Tide", "A", 96, "#86efac"),
        ],
        current: Some(1),
        position: 108.0,
        duration: 248.0,
        playing: true,
        bar_beat: Some("41.2".into()),
        ..PlaybackView::default()
    }
}

// ── The Session mount, with stand-ins for the slots its crate fills ─────────

fn stand_in(label: &'static str) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; justify-content: center; flex: 1; min-width: 0; min-height: 0; color: #8e8e98; font-size: 15px; border: 1px dashed #2b2b31; border-radius: 8px; margin: 6px;",
            "{label}"
        }
    }
}

fn now() -> Element {
    rsx! { NowBand {} }
}
fn setlist() -> Element {
    stand_in("perform.setlist · Session's setlist")
}
fn chart() -> Element {
    stand_in("perform.chart · Chart { paged }")
}
/// Session's dock, as `session_ui::performer` builds it: the rig half
/// when one is paired, the transport (TransportButtons over the nudge row).
fn dock() -> Element {
    rsx! {
        PerformDock {
            rig: rig_half(),
            transport: rsx! {
                div { style: "display: flex; height: 60px; border: 1px dashed #2b2b31; border-radius: 8px; align-items: center; justify-content: center; color: #8e8e98;",
                    "TransportButtons {{ height: 60 }}"
                }
                TransportNudgeRow {}
            },
        }
    }
}

const BODIES: PerformerBodies = PerformerBodies {
    now,
    setlist,
    chart,
    dock,
};

#[component]
fn Session() -> Element {
    provide_playback(FakePlayback {
        view: Rc::new(RefCell::new(view())),
    });
    let opens = OPENS.with(|o| o.borrow().clone());
    let mut link = use_signal(|| opens);
    if link.peek().paired() {
        provide_rig(FakeRig { link });
    }
    use_panel_registry(|| performer_registry(BODIES));
    rsx! {
        // The test's hand on the network: drops the rig, then brings it
        // back. Out of flow and zero-sized, so it moves nothing.
        button { "data-test": "drop", style: "position: absolute; width: 0; height: 0; padding: 0; border: none;",
            onclick: move |_| link.set(RigLink::Offline { peer: Some("Studio Mac".into()) }),
        }
        PerformerScreen {
            scope: SESSION_SCOPE,
            app_bar: rsx! {
                div { style: "display: flex; align-items: center; gap: 12px; flex: 1; padding: 0 12px;",
                    span { style: "font-size: 18px; font-weight: 750;", "Session" }
                    span { style: "flex: 1;" }
                    RigCrumb {}
                    SyncPill {}
                }
            },
        }
    }
}

fn session() -> Element {
    rsx! { Session {} }
}

fn open(link: RigLink) -> DocumentTester {
    reset(link);
    DocumentTester::from_element(session)
        .with_window_size(performer::WIDTH, performer::HEIGHT)
        .build()
}

fn remote() -> RigLink {
    RigLink::Reachable {
        peer: Some("Studio Mac".into()),
    }
}

fn find(doc: &DocumentTester, sel: &str) -> dioxus_test::ResolvedElement {
    doc.query(sel)
        .immediately()
        .unwrap_or_else(|e| panic!("{sel}: {e:?}"))
}

fn size(doc: &DocumentTester, which: &str) -> (f32, f32) {
    find(doc, &format!("[data-perform=\"{which}\"]")).size()
}

/// A bordered control's height as a finger meets it: dioxus-test's
/// `size()` is the content box, so add back its 1 px border either side.
fn touch_height(e: &dioxus_test::ResolvedElement) -> f32 {
    e.size().1 + 2.0
}

fn near(a: f32, b: u32) -> bool {
    (a - b as f32).abs() <= 2.0
}

/// Every region's laid-out size, to hold them still.
const REGIONS: [&str; 9] = [
    "screen",
    "status-inset",
    "app-bar",
    "now",
    "setlist",
    "chart",
    "dock",
    "transport",
    "home-inset",
];

fn rects(doc: &DocumentTester) -> Vec<(&'static str, (f32, f32))> {
    REGIONS.iter().map(|&r| (r, size(doc, r))).collect()
}

fn shot(doc: &DocumentTester, suffix: &str) {
    if let Ok(path) = std::env::var("PERFORMER_SHOT") {
        doc.render_png(path.replace(".png", &format!("-{suffix}.png")));
    }
}

// The dock's first tile (switch 6, top-left): dock top 670, padding 8 and
// 12; a 112 x 60 tile. Its centre, in page coordinates.
const TILE: (f32, f32) = (12.0 + 56.0, 670.0 + 8.0 + 30.0);

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn a_reachable_rig_shows_ten_switches_and_a_press_reaches_the_client() {
    let doc = open(remote());
    let tiles = doc.query_all("[data-switch]").immediately();
    assert_eq!(tiles.len(), 10, "ten switches");
    let (w, h) = size(&doc, "rig");
    assert!(near(w, 592) && near(h, 128), "rig half {w}x{h}");
    assert!(near(size(&doc, "transport").0, 562));
    // Through Blitz's hit-testing, where a finger would land.
    doc.click_at(TILE.0, TILE.1);
    doc.drain();
    assert_eq!(calls(|c| c.presses.clone()), vec![6], "switch 6 pressed");
    // And every tile, by its own press.
    for t in &tiles {
        t.click();
    }
    doc.drain();
    assert_eq!(calls(|c| c.presses.len()), 11);
    // The crumb says whose rig it is.
    let crumb = find(&doc, "[data-rig=\"crumb\"]");
    assert!(
        crumb.inner_html().contains("REMOTE"),
        "{}",
        crumb.outer_html()
    );
    shot(&doc, "rig");
}

#[test]
fn a_rig_that_drops_dims_takes_no_press_and_moves_nothing() {
    let doc = open(remote());
    let before = rects(&doc);
    let rig_before = size(&doc, "rig");
    find(&doc, "[data-test=\"drop\"]").click();
    doc.drain();
    doc.relayout();
    // Dimmed to 45%, marked disabled, with a shield over the tiles.
    let half = find(&doc, "[data-rig=\"offline\"]");
    assert_eq!(half.attribute("aria-disabled").as_deref(), Some("true"));
    let style = half.attribute("style").unwrap_or_default();
    assert!(style.contains("opacity: 0.45"), "{style}");
    let shield = find(&doc, "[data-rig=\"shield\"]").size();
    let tiles = find(&doc, "[data-rig=\"offline\"]").size();
    assert_eq!(shield, tiles, "the shield covers every tile");
    // A finger on a tile reaches nothing.
    doc.click_at(TILE.0, TILE.1);
    doc.drain();
    assert!(
        calls(|c| c.presses.is_empty()),
        "{:?}",
        calls(|c| c.presses.clone())
    );
    // No region moved, to the pixel; the rig's box neither.
    assert_eq!(rects(&doc), before);
    assert_eq!(size(&doc, "rig"), rig_before);
    // The crumb offers Reconnect, and it reaches the client.
    let again = find(&doc, "[data-rig=\"reconnect\"]");
    assert!(again.inner_html().contains("Rig offline · Reconnect"));
    again.click();
    doc.drain();
    assert_eq!(calls(|c| c.reconnects), 1);
    shot(&doc, "rig-offline");
}

#[test]
fn with_no_rig_the_transport_spans_the_dock_and_the_bar_offers_pairing() {
    let doc = open(RigLink::Unpaired);
    assert!(
        doc.query("[data-perform=\"rig\"]").immediately().is_err(),
        "no rig half"
    );
    assert!(doc.query_all("[data-switch]").immediately().is_empty());
    let (w, _) = size(&doc, "transport");
    assert!(near(w, 1170), "transport {w} wide, want 1170");
    let pair = find(&doc, "[data-rig=\"pair\"]");
    assert!(pair.inner_html().contains("Pair a rig"));
    let h = touch_height(&pair);
    assert!(h >= 48.0, "Pair a rig is {h} tall");
    // Every other region is where it is with a rig.
    let with = open(remote());
    for r in [
        "status-inset",
        "app-bar",
        "now",
        "setlist",
        "chart",
        "dock",
        "home-inset",
    ] {
        assert_eq!(size(&doc, r), size(&with, r), "{r}");
    }
    shot(&doc, "no-rig");
}

#[test]
fn the_band_reads_time_bar_beat_bpm_and_key_and_the_menu_goes_to_either_end() {
    let doc = open(RigLink::Unpaired);
    let band = find(&doc, "[data-perform=\"now\"]").inner_html();
    for want in ["1:48 / 4:08", "bar 41.2", "98 bpm", "Key E"] {
        assert!(band.contains(want), "the band lacks {want:?}");
    }
    find(&doc, "button[aria-label=\"More playback options\"]").click();
    doc.drain();
    doc.relayout();
    let items = doc.query_all("[role=\"menuitem\"]").immediately();
    let labels: Vec<String> = items
        .iter()
        .filter_map(|i| i.attribute("aria-label"))
        .collect();
    assert_eq!(&labels[..2], ["Go to start", "Go to end"], "{labels:?}");
    for i in &items[..2] {
        let h = touch_height(i);
        assert!(h >= 48.0, "a menu row {h} tall");
    }
    items[0].click();
    doc.drain();
    assert_eq!(calls(|c| c.starts), 1);
    find(&doc, "button[aria-label=\"More playback options\"]").click();
    doc.drain();
    find(&doc, "[role=\"menuitem\"][aria-label=\"Go to end\"]").click();
    doc.drain();
    assert_eq!(calls(|c| c.ends), 1);
}

// ── No fork: both apps register through one function ───────────────────────

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n == "target" || n == "node_modules")
            {
                continue;
            }
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn both_apps_register_the_performer_panels_through_one_function() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    // The function itself registers every id where it lives.
    let reg = performer_registry(BODIES);
    for (id, region) in performer::ids::ALL {
        let p = reg.get(id).unwrap_or_else(|| panic!("{id} not registered"));
        assert_eq!(p.region, region, "{id}");
    }
    // Session's mount and Signal's gig mount, and every other app source.
    let mut files = Vec::new();
    for app in ["apps/session", "apps/signal"] {
        rust_files(&root.join(app), &mut files);
    }
    assert!(!files.is_empty(), "no app sources under {}", root.display());
    let mut mounts = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap_or_default();
        let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
        // Nobody builds a performer panel by hand.
        for id in [
            "perform.now",
            "perform.setlist",
            "perform.chart",
            "perform.dock",
        ] {
            assert!(
                !src.contains(&format!("\"{id}\"")),
                "{rel} names {id} itself"
            );
        }
        assert!(
            !src.contains("Panel::new(ids::") && !src.contains("Panel::new(performer::ids::"),
            "{rel} registers a performer panel itself"
        );
        if src.contains("PerformerScreen {") {
            assert!(
                src.contains("performer_registry("),
                "{rel} mounts the PerformerScreen without performer_registry"
            );
            mounts.push(rel);
        }
    }
    assert!(
        mounts.iter().any(|m| m.starts_with("apps/session/")),
        "Session mounts the PerformerScreen: {mounts:?}"
    );
}

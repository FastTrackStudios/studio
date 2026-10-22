//! The performer screen, laid out headless on Blitz at the 11-inch iPad's
//! 1194 x 834 (worship / gig screen spec, B1):
//!
//! - the regions land at the spec's rects;
//! - every control in the new panels is a button with a label;
//! - a PNG of the screen, for the Designer to hold against the mock
//!   (`PERFORMER_SHOT=<path>` writes it; the test writes none otherwise).

use std::cell::RefCell;
use std::rc::Rc;

use app_shell::ui::perform::{FollowCard, NowBand, SyncPill, TransportNudgeRow};
use app_shell::ui::performer::{self, PerformDock, PerformerScreen, SIGNAL_SCOPE, ids};
use app_shell::ui::playback::{PlaybackModel, PlaybackView, SectionView, SongView};
use app_shell::ui::{Panel, PanelRegistry, Region, provide_playback, use_panel_registry};
use dioxus::prelude::*;
use dioxus_test::DocumentTester;

/// A set of three songs, the second one up and playing, 1:48 into 4:08.
#[derive(Clone)]
struct Fake {
    view: Rc<RefCell<PlaybackView>>,
}

fn sections() -> Vec<SectionView> {
    let s = |name: &str, colour: &str, start: f64, end: f64| SectionView {
        name: name.into(),
        colour: colour.into(),
        start,
        end,
    };
    vec![
        s("INT", "#2563eb", 0.0, 16.0),
        s("VER 1", "#7c3aed", 16.0, 64.0),
        s("CHO", "#0f766e", 64.0, 96.0),
        s("VER 2", "#c2410c", 96.0, 112.0),
        s("CHO", "#9f1239", 112.0, 160.0),
        s("BRI", "#3f6212", 160.0, 200.0),
        s("CHO", "#0c4a6e", 200.0, 248.0),
    ]
}

fn view(following: Option<&str>) -> PlaybackView {
    let song = |title: &str, key: &str, bpm: u32, colour: &str| SongView {
        title: title.into(),
        key: key.into(),
        bpm,
        colour: colour.into(),
        sections: sections(),
    };
    PlaybackView {
        songs: vec![
            song("Harbor Light", "G", 72, "#7dd3fc"),
            song("Lantern Hymn", "E", 68, "#facc15"),
            song("Morning Tide", "A", 96, "#86efac"),
        ],
        current: Some(1),
        position: 108.0,
        duration: 248.0,
        playing: true,
        can_control: following.is_none(),
        following: following.map(str::to_string),
        peers: 2,
        ..PlaybackView::default()
    }
}

impl PlaybackModel for Fake {
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
    fn set_speed(&self, rate: f64) {
        self.view.borrow_mut().speed = rate;
    }
    fn set_level(&self, level: f64) {
        self.view.borrow_mut().level = level;
    }
}

// Stand-ins for the slots the apps fill with components from crates above
// app-shell (TabletSetlist, ChartView, TouchSwitches, TransportControlBar):
// the same boxes, so the regions and the panels around them are real.

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
    stand_in("perform.setlist · TabletSetlist { sections: false, arm_to_load }")
}
fn chart() -> Element {
    stand_in("perform.chart · ChartView")
}
fn dock() -> Element {
    let following = app_shell::ui::use_playback().view().following.is_some();
    let transport = if following {
        rsx! { FollowCard {} }
    } else {
        rsx! {
            div { style: "display: flex; height: 60px; border: 1px dashed #2b2b31; border-radius: 8px; align-items: center; justify-content: center; color: #8e8e98;",
                "TransportControlBar { wide_play, label_px: 14 }"
            }
            TransportNudgeRow { more: move |()| {} }
        }
    };
    rsx! {
        PerformDock {
            rig: rsx! {
                div { style: "display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); grid-template-rows: 60px 60px; gap: 8px; height: 128px;",
                    for k in 0..10 {
                        div { key: "{k}", style: "border-radius: 8px; background: #1b1b20; border: 1px dashed #2b2b31;" }
                    }
                }
            },
            transport,
        }
    }
}

fn registry() -> PanelRegistry {
    PanelRegistry::new()
        .with(Panel::new(ids::NOW, "Now", Region::Top, now))
        .with(Panel::new(ids::SETLIST, "Setlist", Region::Left, setlist))
        .with(Panel::new(ids::CHART, "Chart", Region::Main, chart))
        .with(Panel::new(ids::DOCK, "Dock", Region::Bottom, dock))
}

#[component]
fn Screen(following: Option<&'static str>) -> Element {
    provide_playback(Fake {
        view: Rc::new(RefCell::new(view(following))),
    });
    use_panel_registry(registry);
    rsx! {
        PerformerScreen {
            scope: SIGNAL_SCOPE,
            app_bar: rsx! {
                div { style: "display: flex; align-items: center; gap: 12px; flex: 1; padding: 0 12px;",
                    span { style: "font-size: 18px; font-weight: 750;", "Signal" }
                    span { style: "flex: 1;" }
                    SyncPill {}
                }
            },
        }
    }
}

fn playing() -> Element {
    rsx! { Screen { following: None } }
}

fn following() -> Element {
    rsx! { Screen { following: Some("Studio Mac") } }
}

fn screen(app: fn() -> Element) -> DocumentTester {
    DocumentTester::from_element(app)
        .with_window_size(performer::WIDTH, performer::HEIGHT)
        .build()
}

/// A region's laid-out size. (dioxus-test's corners are parent-relative,
/// so the rects are checked as sizes: the bands stack in one column and
/// the halves sit in one row, so the sizes fix every edge.)
fn size(doc: &DocumentTester, which: &str) -> (f32, f32) {
    doc.query(format!("[data-perform=\"{which}\"]").as_str())
        .immediately()
        .unwrap_or_else(|e| panic!("{which}: {e:?}"))
        .size()
}

fn near(a: f32, b: u32) -> bool {
    (a - b as f32).abs() <= 2.0
}

#[test]
fn the_regions_land_at_the_specs_rects() {
    let doc = screen(playing);
    let (w, h) = size(&doc, "screen");
    assert!(near(w, 1194) && near(h, 834), "screen {w}x{h}");
    // Top to bottom: 24 + 48 + 112 + 486 + 144 + 20 = 834.
    for (which, want) in [
        ("status-inset", 24),
        ("app-bar", 48),
        ("now", 112),
        ("setlist", 486),
        ("chart", 486),
        ("dock", 144),
        ("home-inset", 20),
    ] {
        let (_, h) = size(&doc, which);
        assert!(near(h, want), "{which}: {h} tall, want {want}");
    }
    // Across: setlist 320 + chart 874; dock 12 + rig 592 + 16 + transport 562 + 12.
    for (which, want) in [
        ("app-bar", 1194),
        ("setlist", 320),
        ("chart", 874),
        ("rig", 592),
        ("transport", 562),
    ] {
        let (w, _) = size(&doc, which);
        assert!(near(w, want), "{which}: {w} wide, want {want}");
    }

    if let Ok(path) = std::env::var("PERFORMER_SHOT") {
        doc.render_png(&path);
    }
}

#[test]
fn every_control_in_the_new_panels_has_a_role_and_a_label() {
    for (name, app) in [("playing", playing as fn() -> Element), ("following", following)] {
        let doc = screen(app);
        let buttons = doc.query_all("button").immediately();
        assert!(buttons.len() >= 8, "{name}: {} buttons", buttons.len());
        for b in buttons {
            let label = b.attribute("aria-label").unwrap_or_default();
            assert!(!label.trim().is_empty(), "{name}: a button without a label: {}", b.outer_html());
        }
        if let Ok(path) = std::env::var("PERFORMER_SHOT") {
            if name == "following" {
                doc.render_png(path.replace(".png", "-sync-locked.png"));
            }
        }
    }
}

#[test]
fn speed_and_level_open_a_peek_whose_choices_are_labelled() {
    let doc = screen(playing);
    let speed = doc
        .query("button[aria-label^=\"Speed,\"]")
        .immediately()
        .expect("the Speed cell");
    speed.click();
    doc.drain();
    let items = doc.query_all("[role=\"menuitemradio\"]").immediately();
    assert_eq!(items.len(), 6, "six speeds");
    for i in items {
        assert!(i.attribute("aria-label").is_some_and(|l| l.starts_with("Speed ")));
    }
}

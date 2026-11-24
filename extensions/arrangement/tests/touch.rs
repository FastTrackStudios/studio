//! A finger in the arrangement's pane does what it does in
//! `studio-desktop`'s window: Architect's `ViewInput::Touch`, through the
//! pane's paint view (`studio_arrangement::host::Hosted::input`, as the
//! host calls it), reaches the widget as a finger.
//!
//! The gesture: a swipe right across the compact track panel expands it,
//! as the window's does (and the toolbar's switch).
//!
//! `cargo test -p studio-arrangement --test touch`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "a test"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use architect::action::{ActionCaller, ActionList};
use architect_workspace::paint::PaintView as _;
use architect_workspace::{
    FrameClock, Geometry, KindId, PaneId, Placement, ViewCtx, ViewHost, ViewInput,
};
use arrangement_view::panel::ArrangementPanel;
use arrangement_view::widget::ArrangementWidget;
use dioxus::prelude::*;
use session_engine_client::song_model::StudioSession;
use studio_arrangement::host::{Hosted, InScope, Stats};

const W: u32 = 1280;
const H: u32 = 780;

fn session() -> StudioSession {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp");
    session_engine::open::open_silent(&path).expect("open the golden template");
    let raw = daw_view_kit::studio::fetch().expect("read it back");
    let planner = daw_view_kit::studio::Planner {
        raw: Arc::new(raw),
        scene: Some("drum-mixing"),
        kinds: Arc::new(session_engine::plan::Kinds::read(&path)),
    };
    let (project, rows) = planner.plan(&planner.raw);
    let previews = daw_view_kit::studio::previews_of(&project);
    StudioSession {
        project,
        rows,
        previews,
        chart: None,
        chart_file: None,
        planner,
    }
}

type Built = (Rc<RefCell<ArrangementWidget>>, ArrangementPanel, InScope);

#[derive(Clone, Default)]
struct Got(Rc<RefCell<Option<Built>>>);

impl PartialEq for Got {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The panel and its widget as the extension's pane builds them: the
/// track panel compact.
#[component]
fn Root(song: Song, got: Got) -> Element {
    use_context_provider(|| song.0.clone());
    use_context_provider(|| Signal::new(session::modes::Mode::Live));
    use_context_provider(|| arrangement_view::panel::StartCompact(true));
    let engine: arrangement_view::panel::Engine = Rc::new(|_| {});
    let (panel, widget) =
        arrangement_view::panel::use_arrangement_panel(None, engine, |w| Rc::new(RefCell::new(w)));
    use_hook(|| *got.0.borrow_mut() = Some((widget, panel, InScope::here())));
    rsx! { div {} }
}

fn ctx() -> ViewCtx {
    ViewCtx::new(
        PaneId::new("arrangement"),
        KindId::paint(),
        (ViewHost::Blitz, Placement::Pane),
        (ActionList::new(), ActionCaller::app()),
        FrameClock::new(Duration::from_millis(4), || Duration::ZERO),
    )
    .with_geometry(Geometry {
        width: W as f32,
        height: H as f32,
        scale: 1.0,
    })
}

const fn touch(id: u64, (x, y): (f32, f32), phase: &'static str) -> ViewInput {
    ViewInput::Touch { id, x, y, phase }
}

#[test]
fn a_finger_swiped_across_the_track_panel_expands_it() {
    let got = Got::default();
    let mut vdom = VirtualDom::new_with_props(
        Root,
        RootProps {
            song: Song(session()),
            got: got.clone(),
        },
    );
    vdom.rebuild_in_place();
    let (widget, panel, scope) = got.0.borrow().clone().expect("the panel mounted");
    let mut hosted = Hosted::new(
        Rc::clone(&widget),
        panel.clone(),
        scope,
        Arc::new(Mutex::new(Stats::default())),
    );
    let ctx = ctx();
    for _ in 0..5 {
        let _ = hosted.layers(&ctx);
    }
    assert!(panel.compact.get(), "the panel starts compact");
    // On a row of the compact panel, then right, well past a swipe.
    let (x, y) = (40.0, 300.0);
    assert!(hosted.input(&ctx, &touch(7, (x, y), "start")));
    for step in 1..=6 {
        let dx = 20.0 * step as f32;
        assert!(hosted.input(&ctx, &touch(7, (x + dx, y), "move")));
    }
    assert!(hosted.input(&ctx, &touch(7, (x + 120.0, y), "end")));
    // The panel takes the swipe up on its next frame.
    for _ in 0..3 {
        let _ = hosted.layers(&ctx);
    }
    assert!(
        !panel.compact.get(),
        "a swipe right did not expand the panel"
    );
}

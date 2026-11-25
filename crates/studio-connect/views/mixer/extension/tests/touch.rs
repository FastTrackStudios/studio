//! A finger in the mixer's pane does what it does on a phone or a
//! touchscreen in `studio-desktop`: Architect's `ViewInput::Touch`,
//! through the pane's paint view (`studio_mixer::host::Hosted::input`, as
//! the host calls it), reaches the widget as a finger.
//!
//! The gesture: a finger dragged sideways scrolls the strips.
//!
//! `cargo test -p studio-mixer --test touch`

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
use dioxus::prelude::*;
use engine_client::song_model::StudioSession;
use mixer_view::mixer_panel::{HostedMixer, Links};
use studio_mixer::host::Hosted;
use studio_view_link::{InScope, Stats};

/// Narrower than the strips, so they scroll.
const SIZE: (u32, u32) = (400, 400);

fn session() -> StudioSession {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../studio-engine/adapters/dynamic-template/fixtures/golden/template.rpp");
    studio_engine::open::open_silent(&path).expect("open the golden template");
    let raw = daw_view_kit::studio::fetch().expect("read it back");
    let planner = engine_client::song_model::template_planner(
        Arc::new(raw),
        Some("drum-mixing"),
        Arc::new(engine_client::plan::Kinds::read(&path)),
    );
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

#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

type Built = (Rc<HostedMixer>, InScope);

#[derive(Clone, Default)]
struct Got(Rc<RefCell<Option<Built>>>);

impl PartialEq for Got {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The widget as the extension's mixer pane builds it: every strip.
#[component]
fn Root(song: Song, got: Got) -> Element {
    let session = use_context_provider(|| song.0.clone());
    use_context_provider(|| Signal::new(engine_domain::modes::Mode::Live));
    use_hook(|| {
        let rows = session
            .planner
            .plan_strips(&session.planner.raw)
            .as_slice()
            .to_vec();
        let mut links = Links::alone(rows, std::collections::HashMap::new());
        links.planner = Some(session.planner.clone());
        *got.0.borrow_mut() = Some((Rc::new(HostedMixer::new(links)), InScope::here()));
    });
    rsx! { div {} }
}

fn ctx() -> ViewCtx {
    ViewCtx::new(
        PaneId::new("mixer"),
        KindId::paint(),
        (ViewHost::Blitz, Placement::Pane),
        (ActionList::new(), ActionCaller::app()),
        FrameClock::new(Duration::from_millis(4), || Duration::ZERO),
    )
    .with_geometry(Geometry {
        width: SIZE.0 as f32,
        height: SIZE.1 as f32,
        scale: 1.0,
    })
}

const fn touch(id: u64, (x, y): (f32, f32), phase: &'static str) -> ViewInput {
    ViewInput::Touch { id, x, y, phase }
}

#[test]
fn a_finger_dragged_sideways_scrolls_the_strips() {
    // The faders the whole height: no rack under the finger.
    mixer_view::mixer_panel::set_racks_shown(false);
    let got = Got::default();
    let mut vdom = VirtualDom::new_with_props(
        Root,
        RootProps {
            song: Song(session()),
            got: got.clone(),
        },
    );
    vdom.rebuild_in_place();
    let (mixer, scope) = got.0.borrow().clone().expect("the widget built");
    let mut hosted = Hosted::new(mixer, scope, Arc::new(Mutex::new(Stats::default())));
    let ctx = ctx();
    for _ in 0..3 {
        let _ = hosted.layers(&ctx);
    }
    let before = hosted.last_scene().cloned().expect("a picture");
    // On a strip's ground, then left along the strips.
    let (x, y) = (300.0, 380.0);
    assert!(hosted.input(&ctx, &touch(3, (x, y), "start")));
    for step in 1..=10 {
        let dx = 20.0 * step as f32;
        assert!(hosted.input(&ctx, &touch(3, (x - dx, y), "move")));
        let _ = hosted.layers(&ctx);
    }
    assert!(hosted.input(&ctx, &touch(3, (x - 200.0, y), "end")));
    let _ = hosted.layers(&ctx);
    let after = hosted.last_scene().cloned().expect("a picture");
    assert!(before != after, "the strips did not move under the finger");
}

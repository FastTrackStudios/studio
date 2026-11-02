//! The `frame.surface` kind: declared once, named by `frame-surface`'s
//! manifest, and refused by an app that has no provider for it; and the
//! surface it shows, painted as SVG that follows its knob and meter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use architect_extensions::{Kind, Manifest};
use architect_workspace::{KindError, Placement, ViewHost, ViewKinds};
use frame_view::surface::{KnobMeter, State};
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
fn the_surface_paints_its_knob_and_meter() {
    let mut surface = KnobMeter::new(212.0).unwrap();
    let svg = surface.svg();
    assert_eq!(svg.width, 212.0);
    assert!(
        svg.height > 200.0,
        "laid out at its document's aspect: {svg:?}"
    );
    assert!(paths(&svg.nodes) > 5, "too little painted: {svg:?}");
    let markup = svg.markup();
    assert!(markup.starts_with("<svg "), "{markup}");
    // The document's colours: the knob's value arc and the meter's bar.
    assert!(markup.contains("#ff7a1a"), "no value arc: {markup}");
    assert!(markup.contains("#3dd67a"), "no meter: {markup}");

    let before = svg.markup();
    surface.set(State {
        knob: 0.9,
        meter: 0.6,
    });
    assert!((surface.knob_real().unwrap() - 90.0).abs() < 1e-6);
    let knob_moved = surface.svg().markup();
    assert_ne!(before, knob_moved, "the knob did not move");

    surface.set(State {
        knob: 0.9,
        meter: 0.1,
    });
    let meter_fell = surface.svg().markup();
    assert_ne!(knob_moved, meter_fell, "the meter did not fall");

    // The same state paints the same surface.
    surface.set(State {
        knob: 0.9,
        meter: 0.6,
    });
    assert_eq!(surface.svg().markup(), knob_moved);
}

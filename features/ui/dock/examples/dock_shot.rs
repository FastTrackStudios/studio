//! Draws every built-in screenset at every form class to PNGs, headless,
//! with stand-in panels: the layouts reviewed before a real panel is in
//! them.
//!
//!     cargo run -p dock --features host --example dock_shot -- /tmp/dock
//!
//! Writes `<dir>/<set>-<class>.png` at a phone, an iPad (each both ways)
//! and a desk window.

use anyrender::ImageRenderer as _;
use anyrender_vello::VelloImageRenderer;
use blitz_dom::{Document as _, DocumentConfig};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;

use dock::builtin::{self, kind};
use dock::dockspace::{InstanceId, Place, Side};
use dock::host::{use_dock, use_dock_provider, use_panel, Catalog, DockHost, Kind, ScreensetBar};
use dock::screenset::{FormClass, ScreensetId, Screensets};

const SIZES: [(FormClass, u32, u32); 5] = [
    (FormClass::PhonePortrait, 402, 874),
    (FormClass::PhoneLandscape, 874, 402),
    (FormClass::TabletPortrait, 834, 1194),
    (FormClass::TabletLandscape, 1194, 834),
    (FormClass::Desk, 1728, 1080),
];

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/dock".into());
    std::fs::create_dir_all(&dir).expect("output directory");
    let sets = builtin::screensets();
    for set in &sets.sets {
        for (class, w, h) in SIZES {
            let png = picture(set.id, class, w, h);
            let mode = if std::env::var_os("FTS_DOCK_ARRANGE").is_some() {
                "-arrange"
            } else {
                ""
            };
            let path = format!("{dir}/{}-{class:?}{mode}.png", set.name.to_lowercase());
            png.save(&path).expect("write png");
            println!("{path}");
        }
    }
}

fn picture(set: ScreensetId, class: FormClass, w: u32, h: u32) -> image::RgbaImage {
    let vdom = VirtualDom::new_with_props(
        Shot,
        ShotProps {
            set,
            class,
            size: (w, h),
        },
    );
    let mut document = DioxusDocument::new(
        vdom,
        DocumentConfig {
            viewport: Some(Viewport::new(w, h, 1.0, ColorScheme::Dark)),
            ..Default::default()
        },
    );
    document.initial_build();
    for _ in 0..4 {
        document.poll(None);
        document.inner_mut().resolve(0.0);
    }
    let mut image = VelloImageRenderer::new(w, h);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            let mut inner = document.inner_mut();
            blitz_paint::paint_scene(painter, &mut inner, 1.0, w, h, 0, 0);
        },
        &mut buffer,
    );
    image::RgbaImage::from_raw(w, h, buffer).expect("image size")
}

#[derive(Props, Clone, PartialEq)]
struct ShotProps {
    set: ScreensetId,
    class: FormClass,
    size: (u32, u32),
}

#[component]
fn Shot(props: ShotProps) -> Element {
    let sets = use_signal(|| {
        let mut s: Screensets = builtin::screensets();
        s.recall(props.set);
        s
    });
    let class = use_signal(|| props.class);
    let dock = use_dock_provider(sets, class, catalog());
    // FTS_DOCK_ARRANGE=1: drawn as while the arrange key is held.
    use_hook(move || {
        if std::env::var_os("FTS_DOCK_ARRANGE").is_some() {
            dock.toggle_arrange();
        }
    });
    let (w, h) = props.size;
    rsx! {
        style { "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; }}" }
        div { style: "position:relative; width:{w}px; height:{h}px; display:flex; flex-direction:column; overflow:hidden; \
                      font-family:system-ui, sans-serif; background:#09090b;",
            if !props.class.is_phone() {
                div { style: "flex:0 0 48px; display:flex; align-items:center; padding:0 12px; \
                              border-bottom:1px solid #2b2b31;",
                    ScreensetBar {}
                }
            }
            div { style: "flex:1 1 0; min-height:0; display:flex;", DockHost {} }
        }
    }
}

fn catalog() -> Catalog {
    let k = |kind: &'static str, title: &'static str, short: &'static str, home: Place| Kind {
        kind,
        title,
        short,
        home,
        chrome: dock::dockspace::Chrome::Bare,
        attach: (kind == kind::TCP).then_some((kind::ARRANGEMENT, Side::Left, 200.0)),
        render: stand_in,
    };
    Catalog::new(vec![
        k(kind::ARRANGEMENT, "Arrangement", "ARR", Place::Center),
        k(kind::TCP, "Tracks", "TCP", Place::Edge(Side::Left)),
        k(kind::MIXER, "Mixer", "MIX", Place::Edge(Side::Bottom)),
        k(kind::CHART, "Chart", "CHT", Place::Edge(Side::Right)),
        k(
            kind::CHART_EDITOR,
            "Chart editor",
            "CED",
            Place::Edge(Side::Right),
        ),
        k(kind::LYRICS, "Lyrics", "LYR", Place::Edge(Side::Right)),
        k(
            kind::LYRIC_EDITOR,
            "Lyric editor",
            "LED",
            Place::Edge(Side::Right),
        ),
        k(kind::FX_ROW, "FX", "FX", Place::Edge(Side::Bottom)),
        k(kind::EDITOR, "Editor", "EDT", Place::Edge(Side::Bottom)),
        k(kind::TRANSPORT, "Transport", "▶", Place::Edge(Side::Bottom)),
        k(kind::TOOLBAR, "Toolbar", "TB", Place::Edge(Side::Top)),
        k(kind::BROWSER, "Browser", "BRW", Place::Edge(Side::Left)),
        k(kind::SETLIST, "Setlist", "SET", Place::Edge(Side::Left)),
        k(kind::RIG, "Rig", "RIG", Place::Center),
        k(
            kind::INSPECTOR,
            "Inspector",
            "INS",
            Place::Edge(Side::Right),
        ),
        k(kind::PROGRESS, "Progress", "PRG", Place::Edge(Side::Top)),
    ])
}

fn stand_in(id: InstanceId) -> Element {
    rsx! { StandIn { id } }
}

/// A panel's place, coloured by kind and labelled.
#[component]
fn StandIn(id: InstanceId) -> Element {
    let dock = use_dock();
    let me = use_panel();
    let kind = dock
        .space()
        .panel(me)
        .map(|p| p.kind.as_str().to_owned())
        .unwrap_or_default();
    let hue = kind
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
        % 360;
    let _ = id;
    rsx! {
        div {
            style: "flex:1; min-width:0; min-height:0; display:flex; align-items:center; \
                    justify-content:center; background:hsl({hue}, 30%, 14%); \
                    color:hsl({hue}, 60%, 75%); font-size:12px; letter-spacing:0.04em; \
                    text-transform:uppercase; overflow:hidden;",
            "{kind}"
        }
    }
}

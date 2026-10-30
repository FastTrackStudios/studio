//! The rig's panels in the Session window's dock (`HostPanels`): the
//! whole rig workspace (`rig`), and the gig screen's pieces as panels of
//! their own — the footswitch scenes (`switches`), the set (`navigator`),
//! the song up with its progress (`now`), the chart (`gig-chart`) and the
//! tone browser (`tones`).
//!
//! They share ONE rig connection and ONE gig: [`scope`], drawn around the
//! dock, connects (as a vox client of the headless rig engine, started if
//! none answers — never an engine in this process, docs/design/one-engine.md)
//! only once the dockspace on screen holds a rig panel, then keeps the
//! connection for the window's life. Connected, it provides the clients,
//! the rig's view and performance state (`guitar::ui::panels::RigProviders`),
//! the gig's state and its playback once, for every panel; a rig panel
//! mounted before then says it is connecting. The dock's panels are drawn
//! afresh when the connection comes up (their client reads are taken at
//! mount).

use dioxus::prelude::*;
use dock::builtin::kind;
use dock::dockspace::{Chrome, Place, Side};
use dock::host::Kind;

use crate::gig::shell::{GigChartPanel, GigPlay, GigScope, NavigatorPanel, NowPanel};
use crate::rig_view::{SharedRig, SupervisedOnly, provide_guitar_clients, use_guitar_link};

/// The kinds that need the rig: one of them on screen connects it.
const RIG_KINDS: [&str; 6] = [
    kind::RIG,
    kind::SWITCHES,
    kind::NAVIGATOR,
    kind::NOW,
    kind::GIG_CHART,
    kind::TONES,
];

/// Studio's kinds for the Session window's dock.
pub(super) fn kinds() -> Vec<Kind> {
    let k = |kind: &'static str,
             title: &'static str,
             short: &'static str,
             home: Place,
             render: fn(dock::dockspace::InstanceId) -> Element| Kind {
        kind,
        title,
        short,
        home,
        chrome: Chrome::Bare,
        attach: None,
        render,
    };
    vec![
        k(kind::RIG, "Rig", "RIG", Place::Center, |_| {
            rsx! { DockRig {} }
        }),
        k(
            kind::SWITCHES,
            "Switches",
            "SW",
            Place::Edge(Side::Bottom),
            |_| rsx! { guitar::ui::panels::Switches {} },
        ),
        k(
            kind::NAVIGATOR,
            "Navigator",
            "NAV",
            Place::Edge(Side::Left),
            |_| rsx! { NavigatorPanel {} },
        ),
        k(kind::NOW, "Now", "NOW", Place::Edge(Side::Top), |_| {
            rsx! { NowPanel {} }
        }),
        k(kind::GIG_CHART, "Gig chart", "GIG", Place::Center, |_| {
            rsx! { GigChartPanel {} }
        }),
        k(
            kind::TONES,
            "Tones",
            "TON",
            Place::Edge(Side::Left),
            |_| rsx! { guitar::ui::panels::Tones {} },
        ),
    ]
}

/// `HostPanels::scope`: the rig's connection and the gig around the dock.
pub(super) fn scope(dock: Element) -> Element {
    rsx! { RigScope { {dock} } }
}

/// The dock as given until the dockspace on screen holds a rig panel; from
/// then on (for the window's life: a screenset without one does not hang
/// up), the dock under the shared connection.
#[component]
fn RigScope(children: Element) -> Element {
    let dock = dock::host::use_dock();
    let shows = use_memo(move || {
        dock.space()
            .panels
            .iter()
            .any(|p| RIG_KINDS.contains(&p.kind.as_str()))
    });
    let wanted = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(false)));
    if shows() {
        wanted.set(true);
    }
    if !wanted.get() {
        return children;
    }
    rsx! { RigConnection { {children} } }
}

/// The one connection, and the gig, for every panel below.
#[component]
fn RigConnection(children: Element) -> Element {
    let link = use_guitar_link(true);
    use_context_provider(|| SharedRig(link));
    use_context_provider(|| SupervisedOnly);
    let generation = (link.generation)();
    let up = link.current().is_some();
    rsx! {
        div { style: "position:relative; flex:1; min-width:0; min-height:0; display:flex;",
            GigScope {
                if up {
                    Connected { key: "{generation}", {children} }
                } else {
                    Waiting { {children} }
                }
            }
        }
    }
}

/// The dock while the rig is not connected: its rig panels say so.
#[component]
fn Waiting(children: Element) -> Element {
    children
}

/// The dock under the connected rig: its clients, then everything the rig's
/// panels share, then the gig's playback.
#[component]
fn Connected(children: Element) -> Element {
    let SharedRig(link) = use_context::<SharedRig>();
    if let Some(clients) = link.current() {
        provide_guitar_clients(clients);
    }
    rsx! {
        guitar::ui::panels::RigProviders {
            GigPlay { {children} }
        }
    }
}

/// The live rig inside the DAW view: the rig workspace (its menu, then the
/// rig picked), a client of the headless rig engine — never an engine in
/// this process, which already runs the window's.
#[component]
fn DockRig() -> Element {
    use_context_provider(|| crate::rig_view::SupervisedOnly);
    // The rig lays out by its panel, not the window (`PanelSize`): measured
    // when mounted, when the window resizes, and on redraws at most four
    // times a second — which catches a divider dragged across it.
    let mut size = use_signal(|| (0.0_f64, 0.0_f64));
    use_context_provider(|| crate::rig_view::PanelSize(size));
    let node =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<std::rc::Rc<MountedData>>)));
    let measure = {
        let node = std::rc::Rc::clone(&node);
        move || {
            let Some(node) = node.borrow().clone() else {
                return;
            };
            spawn(async move {
                if let Ok(r) = node.get_client_rect().await {
                    let now = (r.size.width, r.size.height);
                    let was = *size.peek();
                    if (was.0 - now.0).abs() > 1.0 || (was.1 - now.1).abs() > 1.0 {
                        size.set(now);
                    }
                }
            });
        }
    };
    #[cfg(not(target_os = "ios"))]
    {
        let measure = measure.clone();
        let last = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(None::<std::time::Instant>)));
        dioxus_native::use_window_event(move |event, _| {
            use dioxus_native::winit::event::WindowEvent;
            let due = match event {
                WindowEvent::SurfaceResized(_) => true,
                WindowEvent::RedrawRequested => last
                    .get()
                    .is_none_or(|t| t.elapsed() > std::time::Duration::from_millis(250)),
                _ => false,
            };
            if due {
                last.set(Some(std::time::Instant::now()));
                measure();
            }
        });
    }
    let on_mount = {
        let node = std::rc::Rc::clone(&node);
        move |e: MountedEvent| {
            *node.borrow_mut() = Some(e.data());
            measure();
        }
    };
    rsx! {
        div {
            style: "position:relative; flex:1; min-width:0; min-height:0; overflow:hidden;",
            onmounted: on_mount,
            div { style: "position:absolute; inset:0; display:flex; flex-direction:column;",
                crate::rig_view::SignalWorkspace {}
            }
        }
    }
}

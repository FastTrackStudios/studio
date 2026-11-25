//! The Mixer panel: the session's strips, docked under the arrangement,
//! toggled by the profile's "Toggle mixer" (`x`, REAPER's 40078), the way
//! REAPER docks its mixer.
//!
//! The painting, hit test and edits are the mixer's own:
//! [`crate::mcp::Mixer`] records the
//! strips' chrome, [`crate::strip_overlay::controls`] draws the values over it
//! every frame (so a fader moves without re-recording anything), and
//! [`crate::mcp::click`] / [`crate::mcp::drag`] turn a gesture into
//! an edit. What is new is only the widget holding them, and the
//! [`Links`] it shares with the arrangement:
//!
//! - **rows**: the mixer shows the arrangement's rows, so a group the
//!   visibility manager hides leaves both;
//! - **edits** each way: a widget applies its own edits at once (a fader
//!   that waited for the engine would lag the hand), so the panel hands
//!   each widget's edits to the OTHER one to apply too. Never back to the
//!   one that made them, or a toggle would flip twice and do nothing;
//! - **open**, and a toggle either widget can ask for.
//!
//! v1 leaves out what the retired winit window's mixer had beyond that:
//! the routing panel, renaming, folding a folder's strip, the Tone rack,
//! and balance groups (`crate::balance`, whose rule is implemented and
//! tested but not wired to a fader here yet).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use anyrender::{RenderContext, Scene};
use blitz_dom::node::{ComputedStyles, Widget};
use blitz_traits::events::{BlitzPointerId, MouseEventButton, UiEvent};
use dioxus::prelude::*;
use dsp::eq::ui::eq_graph_interaction::Mods;
use vello::kurbo::Affine;

use crate::dock::Rows;
pub use crate::dock::{Key, Links, MixerMemory, racks_shown, set_racks_shown};
use crate::engine::Edit;
use crate::mcp::Control;
use crate::pointer::{Pointer, Spot};
use crate::song_model::SongView;

/// How tall the docked mixer is: the strip height the mixer was designed
/// at.
pub const HEIGHT: f64 = crate::mcp::DEFAULT_HEIGHT;

/// Where the arrangement stops and how tall the mixer under it is, as CSS
/// lengths.
///
/// Alone, the mixer fills the panel. On a finger's screen the two share
/// it — evenly in the Overview, a little more to the mixer in the DAW
/// view: an iPad's pane is some six hundred pixels, and a fixed mixer at
/// touch size left the arrangement a strip a third of a row tall.
/// Otherwise the mixer is its own height, in pixels.
fn split(open: bool, mixer_only: bool, docked: bool, touch: bool) -> (String, String) {
    if mixer_only {
        return ("0px".to_owned(), "100%".to_owned());
    }
    let mixer = if touch {
        // The Overview's pane is shared with the chart beside it, so the
        // arrangement keeps half; the DAW view's is the arrangement's own,
        // and the mixer, opened there, gets the larger share.
        if docked { "50%" } else { "55%" }.to_owned()
    } else {
        format!("{HEIGHT}px")
    };
    let bottom = if open {
        mixer.clone()
    } else {
        "0px".to_owned()
    };
    (bottom, mixer)
}

/// The phase rails' state, mixing — see [`MixerWidget`]'s `rails`.
#[derive(Clone, Copy, Debug)]
struct RailState {
    scene: Option<&'static str>,
    phase: fts_session::mix_phases::MixPhase,
    settings: crate::settings::Settings,
    audience: crate::app::Audience,
    /// The rail button under the pointer, and the one it is pressing.
    hover: Option<crate::rails::Action>,
    pressed: Option<crate::rails::Action>,
}

impl Default for RailState {
    fn default() -> Self {
        Self {
            scene: None,
            phase: fts_session::mix_phases::MixPhase::Tone,
            settings: crate::settings::Settings::default(),
            audience: crate::app::Audience::Engineer,
            hover: None,
            pressed: None,
        }
    }
}

/// The DAW view's panels: the arrangement, and the mixer under it when
/// it is open.
#[cfg(feature = "native")]
#[component]
pub fn DawPanels(
    /// Open the mixer from the start — the Overview, where the two are one
    /// docked pair rather than a panel `x` summons.
    #[props(default)]
    docked: bool,
    /// The mixer alone, filling the panel — the small-screen layout's
    /// Mixer view.
    #[props(default)]
    mixer_only: bool,
) -> Element {
    let session: SongView = use_context();
    // What the app shows: the mode the mixer's open state is remembered
    // by, whether it opens by default, Mix's and Record's strips.
    let shows = crate::shows::use_shows();
    let mode = shows.mode;
    let memory = try_use_context::<MixerMemory>();
    // The arrangement's own (not the Overview's pair, not the mixer
    // alone): the window's mixer toggle is what opens it.
    let switch = try_use_context::<crate::chrome::MixerToggle>().filter(|_| !docked && !mixer_only);
    let links = use_context_provider(|| {
        let mut links = Links::new(
            session
                .planner
                .plan_strips(&session.planner.raw)
                .as_slice()
                .to_vec(),
        );
        links.open = switch.map_or_else(
            || Signal::new(MixerMemory::wanted(memory, docked, mode, shows.mixer_open)),
            |s| s.open,
        );
        links.docked = docked;
        links.alone = mixer_only;
        links.switch = switch;
        links.planner = Some(session.planner.clone());
        links
    });
    // Under the window's toggle the mixer is pinned only when it is
    // docked; otherwise it is the Mixer view, and this pane is the
    // arrangement alone.
    let memory = memory.filter(|_| switch.is_none());
    links.mix.set(mixer_only && shows.mix);
    links.recording.set(shows.record);
    links.mixing.set(shows.mix);
    // Into another mode: the mixer as that mode last had it.
    let mut open_sig = links.open;
    let mut shown_in = use_signal(|| mode);
    use_effect(use_reactive!(|mode| {
        if *shown_in.peek() != mode {
            shown_in.set(mode);
            let want = MixerMemory::wanted(memory, docked, mode, shows.mixer_open);
            if *open_sig.peek() != want {
                open_sig.set(want);
            }
        }
    }));
    // And whatever it is now (`x`), remembered for this mode.
    use_effect(move || {
        let open = open_sig();
        if let (Some(memory), Some(mode)) = (memory, *shown_in.peek()) {
            memory.remember(docked, mode, open);
        }
    });
    let open = ((links.open)() && switch.is_none_or(|s| (s.docked)())) || mixer_only;
    let touch = crate::touch::use_touch();
    let (arrange_bottom, mixer_height) = split(open, mixer_only, docked, touch);
    let mixer_display = if open { "block" } else { "none" };
    // The inspector, down the arrangement's right — the left is the
    // navigator's, when it is open: the arrangement view's own, not a
    // docked pair's, and only while it is switched on.
    // Read before the condition, never inside it: `try_use_context` is a
    // hook, and a hook skipped on one render (the mixer alone) and called
    // on the next panicked — "unable to retrieve the hook".
    let pins = try_use_context::<crate::chrome::Pins>();
    let inspecting = !mixer_only && !docked && pins.is_some_and(|pins| (pins.inspector)());
    let quantizing = !mixer_only && pins.is_some_and(|pins| (pins.quantize)());
    let strips = use_context_provider(|| InspectorStrips(Signal::new(1)));
    // A phone on its side with the camera housing on the right: the strip
    // moves left of it, its column still reaching the edge.
    let housing = crate::form::use_sides().right;
    let inspector_w = if inspecting {
        inspector_width((strips.0)())
    } else {
        0.0
    };
    let column_w = if inspecting {
        inspector_w + housing
    } else {
        0.0
    };
    rsx! {
        if !mixer_only {
            div {
                                style: "position:absolute; top:0; left:0; right:{column_w}px; bottom:{arrange_bottom};",
                {crate::dock::arrangement()}
                if quantizing {
                    crate::quantize_panel::QuantizePanel {}
                }
            }
        }
        if inspecting {
                        div {
                style: "position:absolute; top:0; right:0; width:{column_w}px; bottom:{arrange_bottom}; \
                        border-left:1px solid #000; overflow:hidden; background:#101113;",
                div {
                    style: "position:absolute; top:0; left:0; width:{inspector_w}px; bottom:0;",
                    Inspector {}
                }
            }
        }
        div {
            style: "display:{mixer_display}; position:absolute; left:0; right:0; bottom:0; \
                    height:{mixer_height}; border-top:1px solid #000;",
            // Where others' pointers over the mixer are placed.
            onmounted: move |e| crate::ghosts::region_mounted("mixer", e.data()),
            Mixer {}
        }
    }
}

/// How much bigger the inspector draws its strips: mute, solo and the
/// fader's cap a finger's size, the strips still slim.
const INSPECTOR_ZOOM: f64 = 1.25;

/// How many strips the inspector shows, for its column's width: the
/// inspector's widget says, and its host carries it here each frame.
#[derive(Clone, Copy)]
struct InspectorStrips(Signal<usize>);

/// How wide the inspector is: `strips` of its slim strips.
fn inspector_width(strips: usize) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a strip count")]
    let strips = strips.max(1) as f64;
    strips * (f64::from(INSPECTOR_STRIP) + crate::mcp::STRIP_GAP) * INSPECTOR_ZOOM + 2.0
}

/// The inspector: the arrangement's rows, as the selected track's strip
/// and the one it goes out through (see [`Links::inspector`]) — a fader,
/// mute and solo to hand without widening the track panel.
#[cfg(feature = "native")]
#[component]
fn Inspector() -> Element {
    let outer: Links = use_context();
    use_context_provider(|| Links {
        inspector: true,
        fill: false,
        open: Signal::new(true),
        ..outer
    });
    rsx! { Mixer {} }
}

/// The mixer panel. No props: the session and the [`Links`] come from
/// context.
#[cfg(feature = "native")]
#[component]
pub fn Mixer() -> Element {
    let links: Links = use_context();
    let touch = use_hook(|| Rc::new(Cell::new(false)));
    touch.set(crate::touch::use_touch());
    let scroll = use_hook(|| Rc::new(Cell::new(0.0_f64)));
    let content_w = use_hook(|| Rc::new(Cell::new(0.0_f64)));
    let strips = use_hook(|| {
        let strips: Rc<RefCell<Strips>> = Rc::default();
        crate::ghosts::register_anchor(
            "mixer",
            Rc::new(MixerAnchor {
                strips: Rc::clone(&strips),
                scroll: Rc::clone(&scroll),
            }),
        );
        strips
    });
    let widget = use_hook(|| {
        let mut widget = MixerWidget::new(
            links.clone(),
            Rc::clone(&scroll),
            Rc::clone(&content_w),
            Rc::clone(&touch),
        );
        widget.strips = Rc::clone(&strips);
        widget.redraw = crate::touch::redraw_hook();
        // The racks' settings, shared with a close-up; and where the
        // widget asks for one.
        if let Some(closeups) = try_consume_context::<crate::closeup::Closeups>() {
            widget.tone = Rc::clone(&closeups.tone);
            widget.signal = std::sync::Arc::clone(&closeups.signal);
            widget.closeups = Some(closeups);
        }
        dioxus_native_dom::CustomWidgetAttr::new(widget)
    });

    // The wheel, which Blitz does not send to the DOM: read at the window
    // and taken only over this panel's rectangle. Either direction runs
    // along the strips.
    let mut rect = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    let mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let measured = use_hook(|| Rc::new(Cell::new(None::<web_time::Instant>)));
    let pointer = use_hook(|| Rc::new(Cell::new((0.0_f64, 0.0_f64))));
    let measuring = Rc::clone(&mounted);
    let scrolling = Rc::clone(&scroll);
    let arrange_node = Rc::clone(&links.arrange_node);
    let refocus = use_hook(|| Rc::new(Cell::new(false)));
    let window = dioxus_native::use_window();
    let asking = try_use_context::<crate::closeup::Closeups>();
    let inspecting = (
        links.inspector,
        try_use_context::<InspectorStrips>().map(|s| s.0),
    );
    let inspector_strips = Rc::clone(&links.strips);
    dioxus_native::use_window_event(move |event, _| match event {
        // A press here: hand the keyboard back to the arrangement. Not
        // from this event: the document is still borrowed while Blitz
        // handles it, and focusing from inside that panics ("RefCell
        // already borrowed"). The next redraw does it, as the rect is
        // read.
        winit::event::WindowEvent::PointerButton {
            state, position, ..
        } if state.is_pressed() => {
            pointer.set(crate::studio::css_point(&*window, position.x, position.y));
            let r = *rect.peek();
            let (x, y) = pointer.get();
            if x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3 {
                refocus.set(true);
            }
        }
        // In CSS pixels, as the rect is.
        winit::event::WindowEvent::PointerMoved { position, .. } => {
            pointer.set(crate::studio::css_point(&*window, position.x, position.y));
        }
        winit::event::WindowEvent::MouseWheel { delta, .. } => {
            let r = *rect.peek();
            let (x, y) = pointer.get();
            if !(x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3) {
                return;
            }
            let (dx, dy) = match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => {
                    (f64::from(*x) * 40.0, f64::from(*y) * 40.0)
                }
                winit::event::MouseScrollDelta::PixelDelta(at) => {
                    crate::studio::css_delta(&*window, at.x, at.y)
                }
                // winit's delta is non-exhaustive: nothing else scrolls.
                _ => (0.0, 0.0),
            };
            let most = (content_w.get() - r.2).max(0.0);
            scrolling.set((scrolling.get() - dx - dy).clamp(0.0, most));
        }
        winit::event::WindowEvent::RedrawRequested => {
            // A close-up the widget asked for, shown.
            if let Some(closeups) = &asking {
                closeups.take_ask();
            }
            // The inspector's column, as wide as the strips it shows now.
            if inspecting.0
                && let Some(mut shown) = inspecting.1
                && *shown.peek() != inspector_strips.get()
            {
                shown.set(inspector_strips.get());
            }
            if refocus.take()
                && let Some(node) = arrange_node.borrow().clone()
            {
                spawn(async move {
                    let _ = node.set_focus(true).await;
                });
            }
            let stale = measured
                .get()
                .is_none_or(|at| at.elapsed() > std::time::Duration::from_millis(200));
            if stale && let Some(node) = measuring.borrow().clone() {
                measured.set(Some(web_time::Instant::now()));
                spawn(async move {
                    if let Ok(got) = node.get_client_rect().await {
                        let next = (got.origin.x, got.origin.y, got.size.width, got.size.height);
                        if next != *rect.peek() {
                            rect.set(next);
                        }
                    }
                });
            }
        }
        _ => {}
    });

    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{colors.surface};",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },

            object {
                style: "position:absolute; left:0; top:0; width:100%; height:100%;",
                tabindex: "0",
                data: widget.clone(),
            }
        }
    }
}

/// The DAW view's panels in a browser: [`DawPanels`], with the widgets in
/// canvases.
#[cfg(feature = "web")]
#[component]
pub fn WebDawPanels(
    engine: crate::page_engine::PageEngineRef,
    /// Open the mixer from the start — the Overview, where the two are one
    /// docked pair rather than a panel `x` summons.
    #[props(default)]
    docked: bool,
    /// The mixer alone, filling the panel — the small-screen layout's
    /// Mixer view.
    #[props(default)]
    mixer_only: bool,
) -> Element {
    let session: SongView = use_context();
    let links = use_context_provider(|| {
        let mut links = Links::new(
            session
                .planner
                .plan_strips(&session.planner.raw)
                .as_slice()
                .to_vec(),
        );
        links.open = Signal::new(docked || mixer_only);
        links.docked = docked;
        links.alone = mixer_only;
        links.planner = Some(session.planner.clone());
        links
    });
    let open = (links.open)() || mixer_only;
    let (arrange_bottom, mixer_height) = split(open, mixer_only, docked, crate::touch::use_touch());
    // Hidden rather than removed: kept at its size, the mixer builds its
    // strips and its GPU context at load, so `x` opens it at once instead
    // of after the second a first build takes.
    let (visibility, events) = if open {
        ("visible", "auto")
    } else {
        ("hidden", "none")
    };
    rsx! {
        if !mixer_only {
            div {
                style: "position:absolute; top:0; left:0; right:0; bottom:{arrange_bottom};",
                {crate::dock::web_arrangement(engine)}
            }
        }
        div {
            style: "visibility:{visibility}; pointer-events:{events}; position:absolute; \
                    left:0; right:0; bottom:0; height:{mixer_height}; border-top:1px solid #000;",
            WebMixer { hidden: !open }
        }
    }
}

/// The mixer panel in a browser. The wheel runs along the strips; a press
/// hands the keyboard back to the arrangement, as [`Mixer`] does.
#[cfg(feature = "web")]
#[component]
pub fn WebMixer(hidden: bool) -> Element {
    use crate::panel_input::PanelEvent;
    let links: Links = use_context();
    let touch = use_hook(|| Rc::new(Cell::new(false)));
    touch.set(crate::touch::use_touch());
    let scroll = use_hook(|| Rc::new(Cell::new(0.0_f64)));
    let content_w = use_hook(|| Rc::new(Cell::new(0.0_f64)));
    let element = use_hook(|| Rc::new(RefCell::new(None::<web_sys::HtmlElement>)));
    let asking = try_use_context::<crate::closeup::Closeups>();
    let widget = use_hook(|| {
        let mut widget = MixerWidget::new(
            links.clone(),
            Rc::clone(&scroll),
            Rc::clone(&content_w),
            Rc::clone(&touch),
        );
        if let Some(closeups) = try_consume_context::<crate::closeup::Closeups>() {
            widget.tone = Rc::clone(&closeups.tone);
            widget.signal = std::sync::Arc::clone(&closeups.signal);
            widget.closeups = Some(closeups);
        }
        crate::web_canvas::HostedRef(Rc::new(RefCell::new(widget)))
    });
    let slot = crate::web_canvas::ElementSlot(Rc::clone(&element));
    let arrange_node = Rc::clone(&links.arrange_node);
    let on_input = move |event: PanelEvent| match event {
        PanelEvent::Wheel { dx, dy } => {
            let width = element
                .borrow()
                .as_ref()
                .map_or(0.0, |el| el.get_bounding_client_rect().width());
            let most = (content_w.get() - width).max(0.0);
            scroll.set((scroll.get() - dx - dy).clamp(0.0, most));
        }
        PanelEvent::Button { pressed: false, .. } => {
            if let Some(node) = arrange_node.borrow().clone() {
                spawn(async move {
                    let _ = node.set_focus(true).await;
                });
            }
        }
        _ => {}
    };
    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{colors.surface};",
            crate::web_canvas::WidgetCanvas {
                widget,
                panel: on_input,
                element: Some(slot),
                hidden,
                // A close-up the widget asked for, shown.
                frame: move |()| {
                    if let Some(closeups) = &asking {
                        closeups.take_ask();
                    }
                },
            }
        }
    }
}

/// The mixer for a host of its own — an Architect pane, beside the
/// window's [`Mixer`] and the browser's [`WebMixer`]: the widget over
/// `links` (from [`Links::alone`], or an inspector's), and what the host
/// scrolls it with. The host paints it ([`MixerWidget::paint_scene`]),
/// hands it its input ([`MixerWidget::event`]) and turns the wheel into
/// [`HostedMixer::wheel`], as the other two do.
pub struct HostedMixer {
    pub widget: Rc<RefCell<MixerWidget>>,
    scroll: Rc<Cell<f64>>,
    content_w: Rc<Cell<f64>>,
    /// Touch mode, as the host has it (bigger strips).
    pub touch: Rc<Cell<bool>>,
    /// Studio's keymap, for the keys pressed in the mixer: the window's
    /// arrangement reads them beside one; a mixer alone reads them here.
    #[cfg(feature = "native")]
    keys: RefCell<crate::keys::Keys>,
}

impl HostedMixer {
    #[must_use]
    pub fn new(links: Links) -> Self {
        let scroll = Rc::new(Cell::new(0.0_f64));
        let content_w = Rc::new(Cell::new(0.0_f64));
        let touch = Rc::new(Cell::new(false));
        let widget = MixerWidget::new(
            links,
            Rc::clone(&scroll),
            Rc::clone(&content_w),
            Rc::clone(&touch),
        );
        Self {
            widget: Rc::new(RefCell::new(widget)),
            scroll,
            content_w,
            touch,
            #[cfg(feature = "native")]
            keys: RefCell::new(crate::keys::Keys::load()),
        }
    }

    /// A key pressed in the mixer alone: what Studio's keymap binds it to,
    /// run (`window::key_alone`) — Space plays, `x` hides the mixer, as in
    /// the window. Whether it did anything; a key up does nothing.
    #[cfg(feature = "native")]
    pub fn key(&self, event: &UiEvent) -> bool {
        match event {
            UiEvent::KeyDown(e) => crate::window::key_alone(&mut self.keys.borrow_mut(), e),
            _ => false,
        }
    }

    /// A signal where there is none, as `FTS_SIMULATE=1` gives the
    /// window's mixer: the meters and the racks fed on the clock from now,
    /// as a playing engine would feed them. For a measurement or a test of
    /// a playing mixer with no audio.
    pub fn simulate(&self) {
        self.widget.borrow_mut().simulated =
            Some((web_time::Instant::now(), crate::simulate::Feed::new()));
    }

    /// The simulated signal ([`Self::simulate`]) held at `at` seconds, so
    /// one moment can be drawn twice; `None` lets it run.
    pub fn hold_simulation(&self, at: Option<f64>) {
        self.widget.borrow_mut().simulated_at = at;
    }

    /// The levels the last picture was drawn at, by project track index.
    #[must_use]
    pub fn painted_levels(&self) -> Vec<daw_proto::TrackLevels> {
        self.widget.borrow().painted_levels.clone()
    }

    /// The wheel over a mixer `width` wide: either direction runs along
    /// the strips (winit's sign, as [`Mixer`] reads it).
    pub fn wheel(&self, dx: f64, dy: f64, width: f64) {
        let most = (self.content_w.get() - width).max(0.0);
        self.scroll
            .set((self.scroll.get() - dx - dy).clamp(0.0, most));
    }
}

#[cfg(feature = "web")]
impl crate::web_canvas::Hosted for MixerWidget {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_scene(width, height, scale)
    }
    fn event(&mut self, event: &UiEvent) {
        MixerWidget::event(self, event);
    }
}

/// A fader or pan knob mid-drag.
struct Turn {
    spot: Spot,
    from_y: f64,
    /// How far, in content pixels, takes the value end to end: the
    /// fader's own travel for a finger, so the cap stays under it.
    travel: f64,
    /// The track as it was when the drag began: the drag is relative to
    /// where the value started, not to where it has got to.
    was: daw_proto::Track,
}

/// What one pointer is doing, from its press to its release. One each, so
/// two fingers move two faders.
enum Hold {
    /// A fader or a knob, being turned.
    Turn(Turn),
    /// A press waiting to be a click: one if it lets go where it landed.
    /// A finger that wanders past [`crate::touch::SLOP`] first is
    /// scrolling instead.
    Press {
        spot: Option<Spot>,
        from: (f64, f64),
        /// Whether a click here selects the strip's track (`Some`), and
        /// whether it adds to the selection there is (Shift or Cmd held).
        /// `None` for a press on a rack, which is the rack's.
        select: Option<bool>,
    },
    /// A press in the overview band ([`OVERVIEW_H`]): the strips go
    /// wherever it points, as it moves.
    Overview,
    /// A rack grip in a hand — an EQ band, a threshold, a knob — moved by
    /// the pointer's travel in layout pixels since `last`.
    Rack {
        row: usize,
        grip: crate::tone::Grip,
        last: (f64, f64),
    },
    /// A finger on a rack where no grip is: the chain scrolls under it.
    RackScroll { from_y: f64, was: f64 },
    /// A Signal phase's make-up, dragged up or down from what it `was`;
    /// sent through `SIGNAL_SET` when let go.
    SignalMakeup {
        track: String,
        phase: crate::signal::chain::PhaseId,
        from_y: f64,
        was: f64,
    },
    /// A finger scrolling the strips, from where it and the scroll were,
    /// and how fast it is going (to throw the strips when it lets go).
    Pan {
        from_x: f64,
        was: f64,
        speed: crate::touch::Speed,
    },
}

/// The widget: the recorded strips and what the pointer is doing to them.
pub struct MixerWidget {
    links: Links,
    palette: crate::palette::Palette,
    font: crate::text::Font,
    layout: crate::layout::Layout,
    /// The rows as last taken from [`Links::rows`], and its generation.
    rows: Rows,
    generation: Option<u64>,
    /// The live values the overlay draws, and which track each strip is.
    tracks: Vec<daw_proto::Track>,
    map: crate::plan::Rows,
    mixer: Option<crate::mcp::Mixer>,
    scroll: Rc<Cell<f64>>,
    content_w: Rc<Cell<f64>>,
    pointer: Pointer<Spot>,
    /// Each pointer's gesture in flight.
    holds: Vec<(BlitzPointerId, Hold)>,
    /// The latest value of each drag in flight, not yet sent (see
    /// [`MixerWidget::flush_drag`]).
    dragged: Vec<(Spot, Edit)>,
    /// Touch mode, as the panel last said: bigger strips (drawn at
    /// [`crate::touch::ZOOM`]).
    touch: Rc<Cell<bool>>,
    /// The engine's track events: what changed anywhere else — another
    /// client, REAPER itself — so the strips do not silently disagree.
    watch: Option<crate::engine::Watch>,
    /// The engine, for a mixer with no arrangement beside it to hand its
    /// edits to ([`Links::alone`]).
    applier: Option<crate::engine::Applier>,
    /// The tracks [`Links::arm_of`] arms in place of their strips'.
    targets: Vec<daw_proto::Track>,
    /// An inspector's whole session, as the rows it picks from, with every
    /// track kept current — its selection among them.
    all: Rows,
    /// The track the inspector shows, as it was last built for.
    inspected: Option<String>,
    /// The Tone rack: each track's EQ, compression and saturation
    /// settings (`crate::tone`), what its strip draws in the rack and what
    /// a grip edits.
    tone: Rc<crate::tone::Shared>,
    /// The settings' generation the racks were last drawn at: an edit
    /// elsewhere (a close-up) makes them stale.
    tone_seen: u64,
    /// Where a close-up is asked for ([`crate::closeup`]).
    closeups: Option<crate::closeup::Closeups>,
    /// Every Signal FX's chain, fold and meters (`crate::signal`), shared
    /// with a block's close-up.
    signal: crate::signal::closeup::Shared,
    /// The Signal rows' generation last painted, and the last
    /// `SIGNAL_INSERTS` ask taken up.
    signal_seen: std::cell::Cell<u64>,
    signal_asks_seen: u64,
    /// Each rack's cached picture and live meters, one per track: every
    /// rack is drawn live (so a fold or a scroll applies to all of them),
    /// from a cache an edit invalidates.
    spectra: std::collections::HashMap<String, crate::tone::Analyser>,
    /// Each track's level history, for the rack's compressors.
    levels: std::collections::HashMap<String, crate::tone::Levels>,
    /// `FTS_SIMULATE=1`: a signal where there is none — the meters and
    /// every rack's history and spectrum fed by [`crate::simulate::Feed`]
    /// on the clock since the window opened, as the engine would feed them
    /// while playing.
    simulated: Option<(web_time::Instant, crate::simulate::Feed)>,
    /// Which mix phases are folded shut in the racks.
    folds: crate::tone::Fold,
    /// How far the racks' chains are scrolled under their boxes.
    rack_scroll: f64,
    /// The rack grip under a mouse, lit.
    rack_hover: Option<(usize, crate::tone::Grip)>,
    /// The last rack grip pressed, and when: a second press on it soon
    /// after puts it back to its default.
    rack_pressed: Option<(usize, crate::tone::Grip, web_time::Instant)>,
    /// The selected track the mixer was built with: another selection
    /// opens another strip, which is a rebuild.
    built_open: Option<String>,
    /// Whether the racks were shown when it was built ([`racks_shown`]).
    built_racks: bool,
    /// Whether the racks were shown at the last paint, built or not.
    racks_seen: Cell<bool>,
    /// Where the meters' moving parts are drawn when a host asked for
    /// them apart ([`MixerWidget::paint_layers`]); `None` (the window's):
    /// into the picture.
    apart: Option<crate::strip_overlay::Apart>,
    /// The levels the last picture was drawn at: what a host's values
    /// must say for the parts apart to show the same.
    painted_levels: Vec<daw_proto::TrackLevels>,
    /// The simulated signal's clock held at this many seconds (a test
    /// drawing one moment twice); `None`: it runs.
    simulated_at: Option<f64>,
    /// The last `MIXER_VIEW` ask taken up (`mixer_actions::MIXER_ASKS`).
    view_seen: u64,
    /// The folds the rows were planned at ([`crate::folds::generation`]).
    folds_seen: u64,
    /// A mixer on its own: the song's tracks as the engine has them now,
    /// every track (the ones not shown too), kept by its events, and
    /// whether a track's visibility changed since the rows were planned
    /// (the visibility manager, run in an arrangement elsewhere: the
    /// window's mixer is handed the arrangement's rows instead).
    raw_now: Option<daw_ui::studio::project::Project>,
    replan: bool,
    /// Whether the rows were last planned for Mix mode (`Links::mix`);
    /// `None` before the first plan.
    mix_seen: Option<bool>,
    /// What the phase rails have chosen, mixing: the scene shown (`None`:
    /// the session's), the phase, the right rail's switches, and who the
    /// window is for.
    rails: RailState,
    /// The shared phase choice's version last followed
    /// (`session::mode_phases`): a newer one was chosen elsewhere.
    phase_seen: u64,
    /// Where the strips start in the panel (CSS px): past the phase
    /// rails when mixing, under the overview band otherwise.
    origin: Cell<(f64, f64)>,
    /// The panel's height when last painted (CSS px): the extent a Mix
    /// mode plan lays the scene out for.
    extent: Cell<f64>,
    /// The strips still moving after a finger threw them.
    fling: Option<crate::touch::Fling>,
    /// Ask the host for another frame: Blitz paints a widget after an
    /// event, and a fling has none. (The page paints every frame.)
    redraw: Option<Rc<dyn Fn()>>,
    clips: crate::strip_overlay::Clips,
    meters: Option<crate::engine::Meters>,
    dirty: Cell<bool>,
    /// The widget's width as last painted, in CSS pixels: how far a
    /// finger can scroll.
    width: Cell<f64>,
    /// The zoom the last paint drew at ([`MixerWidget::zoom_for`]).
    drawn_zoom: Cell<f64>,
    /// How tall the overview band over the strips was drawn, in CSS
    /// pixels: 0 where there is none.
    band: Cell<f64>,
    /// Each strip as last laid out, for anchoring others' pointers.
    strips: Rc<RefCell<Strips>>,
}

/// How tall the overview band over the strips is, in CSS pixels
/// ([`MixerWidget::overview`]).
const OVERVIEW_H: f64 = 30.0;

/// The most a filling mixer ([`Links::fill`]) zooms its strips to fill
/// its width: past this, four strips are four slabs.
const FILL_MAX: f64 = 2.2;

/// The shortest a strip is laid out when zoomed: where the name plate
/// still clears the routing and the buttons over it.
const MIN_STRIP_H: f64 = 230.0;

/// Each strip's track, left edge and width, in content pixels.
type Strips = Vec<(String, f64, f64)>;

/// A pointer over the mixer, anchored to a strip: the track's guid, how
/// far across the strip, how far down — the same fader on the same track
/// in every window, whatever each has scrolled to.
struct MixerAnchor {
    strips: Rc<RefCell<Strips>>,
    scroll: Rc<Cell<f64>>,
}

impl crate::ghosts::Anchor for MixerAnchor {
    fn anchor(&self, x: f64, y: f64, (_, h): (f64, f64)) -> Option<(String, f64, f64)> {
        let cx = x + self.scroll.get();
        let strips = self.strips.borrow();
        let (guid, left, width) = strips.iter().find(|(_, l, w)| cx >= *l && cx < l + w)?;
        Some((guid.clone(), (cx - left) / width.max(1.0), y / h.max(1.0)))
    }

    fn place(&self, key: &str, u: f64, v: f64, (_, h): (f64, f64)) -> Option<(f64, f64)> {
        let strips = self.strips.borrow();
        let (_, left, width) = strips.iter().find(|(g, _, _)| g == key)?;
        Some((u.mul_add(*width, *left) - self.scroll.get(), v * h))
    }
}

/// How far past the edges, in content pixels, strips are recorded ahead
/// of being scrolled to — a couple of strips, so a scroll does not
/// arrive at an unrecorded one.
const NEAR: f64 = 240.0;

/// How long a frame may spend recording strips it has not recorded yet.
const RECORD_BUDGET: std::time::Duration = std::time::Duration::from_millis(10);

impl MixerWidget {
    fn new(
        links: Links,
        scroll: Rc<Cell<f64>>,
        content_w: Rc<Cell<f64>>,
        touch: Rc<Cell<bool>>,
    ) -> Self {
        let theme = daw_ui::theming::Theme::dark();
        let applier = if links.alone {
            crate::engine::Applier::start()
        } else {
            None
        };
        let targets = links.arm_of.values().cloned().collect();
        Self {
            links,
            palette: crate::palette::Palette::from_theme(&theme),
            font: crate::text::Font::embedded().expect("the embedded font"),
            layout: crate::layout::Layout::from_env(),
            rows: Vec::new(),
            generation: None,
            tracks: Vec::new(),
            map: crate::plan::Rows::of(&[], &[]),
            mixer: None,
            scroll,
            content_w,
            pointer: Pointer::default(),
            holds: Vec::new(),
            dragged: Vec::new(),
            touch,
            watch: crate::engine::Watch::start(),
            applier,
            targets,
            fling: None,
            redraw: None,
            tone: Rc::default(),
            tone_seen: 0,
            closeups: None,
            signal: std::sync::Arc::new(std::sync::Mutex::new(crate::signal::Inserts::installed())),
            signal_seen: std::cell::Cell::new(0),
            signal_asks_seen: 0,
            spectra: std::collections::HashMap::new(),
            levels: std::collections::HashMap::new(),
            simulated: std::env::var_os("FTS_SIMULATE")
                .is_some_and(|v| !v.is_empty() && v != "0")
                .then(|| (web_time::Instant::now(), crate::simulate::Feed::new())),
            folds: crate::tone::Fold::rest(),
            rack_scroll: 0.0,
            rack_hover: None,
            rack_pressed: None,
            built_open: None,
            built_racks: racks_shown(),
            racks_seen: Cell::new(racks_shown()),
            apart: None,
            painted_levels: Vec::new(),
            simulated_at: None,
            view_seen: 0,
            folds_seen: crate::folds::generation(),
            raw_now: None,
            replan: false,
            mix_seen: None,
            rails: RailState::default(),
            // None followed yet: the first frame tells the rest of the
            // window which phase the rails start in.
            phase_seen: u64::MAX,
            origin: Cell::new((0.0, 0.0)),
            extent: Cell::new(1440.0),
            all: Vec::new(),
            inspected: None,
            clips: crate::strip_overlay::Clips::default(),
            meters: crate::engine::Meters::start(),
            strips: Rc::default(),
            dirty: Cell::new(false),
            width: Cell::new(0.0),
            drawn_zoom: Cell::new(1.0),
            band: Cell::new(0.0),
        }
    }

    /// Take the arrangement's rows when they have changed, and the edits
    /// it has made since the last frame.
    fn catch_up(&mut self) {
        // A mixer on its own plans its own rows when a folder opens or
        // shuts; beside an arrangement, the arrangement does and sends
        // them. The strips keep what they show now (a level, a mute) —
        // the plan is the song as it opened.
        let folds = crate::folds::generation();
        let mix = self.links.mix.get();
        if folds != self.folds_seen || Some(mix) != self.mix_seen || self.replan {
            self.folds_seen = folds;
            self.mix_seen = Some(mix);
            self.replan = false;
            if self.links.alone
                && let Some(planner) = &self.links.planner
            {
                // Mixing, the scene's own layout for the mixer; otherwise
                // the scene's strips, folded as the window has them.
                let raw = self.raw_now.as_ref().unwrap_or(&planner.raw);
                let planned = if mix {
                    planner.plan_mixer(
                        raw,
                        self.rails.scene,
                        self.rails.settings,
                        self.extent.get(),
                    )
                } else {
                    planner.plan_strips(raw)
                };
                let now: std::collections::HashMap<&str, &daw_proto::Track> =
                    self.tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
                let rows: Rows = planned
                    .as_slice()
                    .iter()
                    .map(|(track, depth)| {
                        let track = now
                            .get(track.guid.as_str())
                            .map_or_else(|| track.clone(), |current| (*current).clone());
                        (track, *depth)
                    })
                    .collect();
                let mut shared = self.links.rows.borrow_mut();
                shared.0 = shared.0.wrapping_add(1);
                shared.1 = rows;
            }
        }
        {
            let shared = self.links.rows.borrow();
            if self.generation != Some(shared.0) {
                self.generation = Some(shared.0);
                if self.links.inspector {
                    // Picked from below, once the engine's word on the
                    // selection is in.
                    self.all.clone_from(&shared.1);
                    self.inspected = None;
                    self.rows = inspect(&self.all);
                    self.links.strips.set(self.rows.len().max(1));
                } else {
                    self.rows.clone_from(&shared.1);
                }
                self.tracks = self.rows.iter().map(|(t, _)| t.clone()).collect();
                self.map = crate::plan::Rows::of(&self.rows, &self.tracks);
                self.mixer = None;
                self.pointer = Pointer::default();
                self.holds.clear();
                // Every track a rack: its settings, and a live picture.
                self.tone.store.borrow_mut().seed(&self.rows);
                for (track, _) in &self.rows {
                    self.spectra.entry(track.guid.clone()).or_default();
                }
            }
        }
        let echoed: Vec<Edit> = self.links.to_mixer.borrow_mut().drain(..).collect();
        for edit in &echoed {
            predict(&mut self.tracks, edit);
        }
        self.heard();
        if self.links.inspector {
            self.reinspect();
        }
    }

    /// An inspector's strips follow the selection: built again for the
    /// newly selected track, carrying what each strip shows now.
    fn reinspect(&mut self) {
        let selected = self
            .all
            .iter()
            .find(|(t, _)| t.selected)
            .map(|(t, _)| t.guid.clone());
        if selected == self.inspected {
            return;
        }
        self.inspected = selected;
        self.rows = inspect(&self.all);
        self.links.strips.set(self.rows.len().max(1));
        self.tracks = self.rows.iter().map(|(t, _)| t.clone()).collect();
        self.map = crate::plan::Rows::of(&self.rows, &self.tracks);
        self.mixer = None;
        self.pointer = Pointer::default();
        self.holds.clear();
        self.tone.store.borrow_mut().seed(&self.rows);
    }

    /// What the engine says changed, applied to the strips. A fader or a
    /// knob in a hand keeps the hand's value: the engine's answer to an
    /// earlier step of the same drag would pull it back.
    fn heard(&mut self) {
        let Some(watch) = &self.watch else { return };
        let events: Vec<_> = watch.drain().collect();
        for event in &events {
            use daw_proto::track::TrackEvent as E;
            // The rows are the arrangement's (or the panel's) to change;
            // an event that adds, removes or reorders tracks would
            // misalign them here.
            if matches!(event, E::Added(_) | E::Removed(_) | E::Moved { .. }) {
                continue;
            }
            if let Some(guid) = crate::engine::continuous_for(event)
                && self
                    .holds
                    .iter()
                    .any(|(_, hold)| matches!(hold, Hold::Turn(turn) if turn.was.guid == guid))
            {
                continue;
            }
            crate::engine::apply_event(&mut self.tracks, event);
            crate::engine::apply_event(&mut self.targets, event);
            // A name and a colour are recorded chrome (the strip's label
            // is fitted, its ground tinted, as the mixer is recorded):
            // recorded again. Beside an arrangement it echoes them
            // (`Links::to_mixer`); alone, they come only as here.
            if matches!(event, E::Renamed { .. } | E::ColorChanged { .. }) {
                // The strips are laid out from the rows.
                let mut laid: Vec<daw_proto::Track> =
                    self.rows.iter().map(|(t, _)| t.clone()).collect();
                crate::engine::apply_event(&mut laid, event);
                for ((row, _), now) in self.rows.iter_mut().zip(laid) {
                    *row = now;
                }
                self.mixer = None;
            }
            // On its own, every track kept: a track shown or hidden plans
            // the strips again.
            if self.links.alone
                && let Some(planner) = &self.links.planner
            {
                let raw = self.raw_now.get_or_insert_with(|| (*planner.raw).clone());
                let shown = |tracks: &[daw_proto::Track]| -> Vec<bool> {
                    tracks.iter().map(|t| t.visible_in_mixer).collect()
                };
                let was = shown(&raw.tracks);
                crate::engine::apply_event(&mut raw.tracks, event);
                if shown(&raw.tracks) != was {
                    self.replan = true;
                }
            }
            if self.links.inspector {
                let mut all: Vec<daw_proto::Track> =
                    self.all.iter().map(|(t, _)| t.clone()).collect();
                crate::engine::apply_event(&mut all, event);
                for ((row, _), now) in self.all.iter_mut().zip(all) {
                    *row = now;
                }
            }
        }
        self.show_targets();
    }

    /// A strip that arms another track shows that track's arm.
    fn show_targets(&mut self) {
        if self.targets.is_empty() {
            return;
        }
        for track in &mut self.tracks {
            if let Some(target) = self.links.arm_of.get(&track.guid)
                && let Some(now) = self.targets.iter().find(|t| t.guid == target.guid)
            {
                track.armed = now.armed;
                track.input_monitor = now.input_monitor;
            }
        }
    }

    /// The track a control on `track`'s strip acts on: the strip's own,
    /// except an arm (and the monitoring under it) that [`Links::arm_of`]
    /// sends elsewhere.
    fn acts_on(&self, control: Control, track: &daw_proto::Track) -> daw_proto::Track {
        if matches!(control, Control::RecArm | Control::Monitor)
            && let Some(target) = self.links.arm_of.get(&track.guid)
        {
            return self
                .targets
                .iter()
                .find(|t| t.guid == target.guid)
                .unwrap_or(target)
                .clone();
        }
        track.clone()
    }

    /// The strip under a point in the widget's own coordinates, if the
    /// point is on one (between its top and its floor).
    fn strip_row_at(&self, x: f64, y: f64) -> Option<usize> {
        let (content_x, content_y) = self.content(x, y);
        let mixer = self.mixer.as_ref()?;
        let row = mixer.strip_at(content_x)?;
        let (_, _, height) = mixer.strip_box(row)?;
        (0.0..=height).contains(&content_y).then_some(row)
    }

    /// The control under a point in the widget's own coordinates.
    fn spot_at(&self, x: f64, y: f64) -> Option<Spot> {
        let (content_x, content_y) = self.content(x, y);
        let mixer = self.mixer.as_ref()?;
        let row = mixer.strip_at(content_x)?;
        let (left, _, _) = mixer.strip_box(row)?;
        let control = match self.track_at(row) {
            Some(track) => crate::mcp::control_in(
                mixer,
                row,
                (&self.palette, &self.font),
                track,
                (content_x - left, content_y),
            )?,
            None => crate::mcp::control_at(mixer, row, content_x - left, content_y)?,
        };
        Some(Spot { row, control })
    }

    /// How much bigger the strips are drawn than they are laid out, as the
    /// last paint drew them: what a press is read back through.
    fn zoom(&self) -> f64 {
        self.drawn_zoom.get()
    }

    /// How much bigger to draw strips in a `width` by `height` panel (CSS
    /// pixels): touch mode's zoom for that width; for a mixer that
    /// [`Links::fill`]s, as big as fills the width, up to [`FILL_MAX`];
    /// and never so big that a strip is laid out shorter than
    /// [`MIN_STRIP_H`] — below that its name runs into the buttons above
    /// it, so a short dock (the Overview's, on a tablet) gets strips
    /// somewhat less big instead.
    fn zoom_for(&self, width: f64, height: f64) -> f64 {
        // The inspector's strips are slim at their own size.
        if self.links.inspector {
            return (height / MIN_STRIP_H).clamp(0.5, INSPECTOR_ZOOM);
        }
        let mut zoom = crate::touch::mixer_zoom(self.touch.get(), width);
        if self.links.fill && !self.rows.is_empty() {
            #[expect(clippy::cast_precision_loss, reason = "a strip count")]
            let across = self.rows.len() as f64 * (crate::mcp::STRIP_W + crate::mcp::STRIP_GAP);
            zoom = zoom.max((width / across).min(FILL_MAX));
        }
        zoom.min((height / MIN_STRIP_H).max(1.0))
    }

    /// A point in the widget, in the strips' own (content) coordinates.
    /// The scroll is kept in CSS pixels, so the wheel and the ghosts'
    /// anchors need not know about the zoom.
    /// The phase rails' buttons, as the rails' state has them.
    fn rail_profile(&self) -> crate::rails::Profile {
        crate::rails::profile(
            crate::rails::Surface::Mixer,
            crate::app::MIX,
            self.rails.phase,
            self.rails
                .scene
                .or(self.links.planner.as_ref().and_then(|p| p.scene)),
            self.rails.settings,
            self.rails.audience,
            "drums",
        )
    }

    /// The rails' frame, as last painted (CSS px).
    fn rail_frame(&self) -> crate::rails::Frame {
        crate::rails::Frame::new(self.width.get(), self.extent.get())
    }

    /// Whether `(x, y)` is on the rails rather than the strips (mixing).
    fn in_rails(&self, x: f64, y: f64) -> bool {
        self.links.mix.get() && !self.rail_frame().content().contains((x, y))
    }

    /// The rail button at `(x, y)`, if any.
    fn rail_at(&self, x: f64, y: f64) -> Option<crate::rails::Action> {
        if !self.links.mix.get() {
            return None;
        }
        let frame = self.rail_frame();
        let profile = self.rail_profile();
        let side = |items: &[crate::rails::Item<'_>], right: bool| {
            items
                .iter()
                .enumerate()
                .find(|(i, _)| frame.slot(*i, right).is_some_and(|r| r.contains((x, y))))
                .map(|(_, item)| item.act)
        };
        side(&profile.left, false)
            .or_else(|| side(&profile.right, true))
            .or_else(|| {
                profile
                    .top
                    .iter()
                    .enumerate()
                    .find(|(i, _)| frame.top_slot(*i).is_some_and(|r| r.contains((x, y))))
                    .map(|(_, item)| item.act)
            })
    }

    /// Do what a rail button says.
    /// Take up a Mix phase chosen elsewhere (the bottom bar's switcher),
    /// as a press on its rail would.
    fn follow_phase(&mut self) {
        // Only a mixer in the window's shell shares the window's phase:
        // one standing alone (a test, a bench) keeps its own.
        if self.links.switch.is_none() {
            return;
        }
        if self.phase_seen == u64::MAX {
            crate::app::choose_phase(self.rails.phase.slug());
            self.phase_seen = crate::app::phase().1;
            return;
        }
        let (chosen, now) = crate::app::phase();
        if now == self.phase_seen {
            return;
        }
        self.phase_seen = now;
        let chosen = chosen.and_then(fts_session::mix_phases::MixPhase::from_slug);
        if let Some(phase) = chosen
            && phase != self.rails.phase
        {
            self.rail_act(crate::rails::Action::Phase(phase));
        }
    }

    fn rail_act(&mut self, act: crate::rails::Action) {
        use crate::app::Audience;
        use crate::rails::Action;
        // A rail of the mixer's own view: `MIXER_VIEW`, taken up at once
        // (and by any other mixer on its next frame).
        let view = match act {
            Action::Scene(slug) => Some(crate::mixer_actions::MixerView {
                scene: Some(slug.to_owned()),
                ..Default::default()
            }),
            Action::FocusSelected => Some(crate::mixer_actions::MixerView {
                focus_selected: Some(!self.rails.settings.focus_selected),
                ..Default::default()
            }),
            Action::TakeFocusWidth => Some(crate::mixer_actions::MixerView {
                take_focus_width: Some(!self.rails.settings.take_focus_width),
                ..Default::default()
            }),
            Action::Audience => Some(crate::mixer_actions::MixerView {
                audience: Some(
                    if self.rails.audience == Audience::Engineer {
                        "player"
                    } else {
                        "engineer"
                    }
                    .to_owned(),
                ),
                ..Default::default()
            }),
            _ => None,
        };
        if let Some(view) = view {
            self.press_view(&view);
            return;
        }
        match act {
            Action::Scene(_)
            | Action::FocusSelected
            | Action::TakeFocusWidth
            | Action::Audience => {}
            // Every rack opens at that phase's section.
            Action::Phase(phase) => {
                // The window's one choice of phase: the bottom bar's
                // switcher shows it.
                if self.links.switch.is_some() {
                    let press = crate::mcp::Press {
                        id: "SESSION_PHASE",
                        args: serde_json::json!({ "mode": "Mix", "phase": phase.slug() }),
                        preview: Edit::Done(0),
                    };
                    crate::mcp::run(&press);
                    self.phase_seen = crate::app::phase().1;
                }
                self.rails.phase = phase;
                self.folds.focus(phase);
                for analyser in self.spectra.values_mut() {
                    analyser.invalidate();
                }
            }
            // Not the mixer's: the mode is switched in the bottom bar, the
            // takes are the arrangement's.
            Action::Mode(_) | Action::FoldedTakes => {}
        }
        // Built again with what changed.
        self.mixer = None;
    }

    /// `MIXER_VIEW` run with `view`, and taken up here at once.
    fn press_view(&mut self, view: &crate::mixer_actions::MixerView) {
        let args = facet_json::to_string(view)
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok());
        if let Some(args) = args {
            let _ = crate::press::press(crate::mixer_actions::MIXER_VIEW.id, &args);
        }
        self.take_view();
    }

    /// What `MIXER_VIEW` asked since the last look, shown.
    fn take_view(&mut self) {
        use crate::app::Audience;
        for view in
            crate::view_actions::since(&crate::mixer_actions::MIXER_ASKS, &mut self.view_seen)
        {
            if let Some(scene) = view
                .scene
                .as_deref()
                .and_then(crate::app::scene)
                .map(|s| s.slug)
            {
                self.rails.scene = Some(scene);
                self.mix_seen = None;
            }
            if let Some(on) = view.focus_selected {
                self.rails.settings.focus_selected = on;
                self.mix_seen = None;
            }
            if let Some(on) = view.take_focus_width {
                self.rails.settings.take_focus_width = on;
                self.mix_seen = None;
            }
            if let Some(audience) = view.audience.as_deref() {
                self.rails.audience = if audience.eq_ignore_ascii_case("player") {
                    Audience::Player
                } else {
                    Audience::Engineer
                };
            }
            if let Some(at) = view.scroll {
                let most = (self.content_w.get() - self.width.get()).max(0.0);
                self.scroll.set(at.clamp(0.0, most));
                self.fling = None;
            }
            if let Some(at) = view.rack_scroll {
                self.rack_scroll = at.clamp(0.0, self.rack_span());
            }
            if let Some(fold) = view.rack_fold
                && let Some(phase) = fts_session::mix_phases::MixPhase::from_slug(&fold.phase)
            {
                self.folds.toggle(&fold.track, phase);
                for analyser in self.spectra.values_mut() {
                    analyser.invalidate();
                }
                self.invalidate_rack(&fold.track);
            }
            // Built again with what changed.
            self.mixer = None;
            self.dirty.set(true);
        }
    }

    /// Where the strips are scrolled to, said (`MIXER_VIEW {scroll}`): a
    /// scroll gesture ended.
    fn commit_scroll(&mut self) {
        let view = crate::mixer_actions::MixerView {
            scroll: Some(self.scroll.get()),
            ..Default::default()
        };
        self.press_view(&view);
    }

    fn content(&self, x: f64, y: f64) -> (f64, f64) {
        let zoom = self.zoom();
        let (left, top) = self.origin.get();
        ((x - left + self.scroll.get()) / zoom, (y - top) / zoom)
    }

    /// Whether a finger at `(x, y)` is on `row`'s fader cap, give or take
    /// a fingertip's worth: in touch mode the fader moves only by its
    /// cap, so a finger landing on the groove can scroll instead.
    fn on_cap(&self, row: usize, x: f64, y: f64) -> bool {
        let (content_x, content_y) = self.content(x, y);
        let (Some(mixer), Some(track)) = (self.mixer.as_ref(), self.track_at(row)) else {
            return false;
        };
        let (Some((left, _, _)), Some(strip)) = (mixer.strip_box(row), mixer.strip(row)) else {
            return false;
        };
        strip.cap(track.volume).is_some_and(|cap| {
            cap.inflate(6.0, 8.0)
                .contains((content_x - left, content_y))
        })
    }

    /// Send each drag's latest value on, if it has moved since the last.
    /// A turn's fader or pan knob taken hold of (`held`) or let go — for
    /// the automation that records while it is held.
    fn touch(&self, turn: &Turn, held: bool) {
        let which = crate::engine::Touched::of(
            turn.spot.control == Control::Volume,
            turn.spot.control == Control::Pan,
        );
        if let Some(which) = which {
            self.send(Edit::Touch(turn.was.guid.clone(), which, held));
        }
    }

    /// A fader or pan drag over: where it ended committed through
    /// `TRACK_SET`, the action a model or the CLI sets a fader with. The
    /// drag itself was a live preview (`flush_drag`, the automation
    /// writer's edits); this is the one step the Action List sees.
    fn commit_turn(&self, turn: &Turn) {
        let Some(track) = self.tracks.iter().find(|t| t.guid == turn.was.guid) else {
            return;
        };
        let args = match turn.spot.control {
            Control::Volume => serde_json::json!({
                "track": track.guid,
                "volume_db": 20.0 * track.volume.max(1e-9).log10(),
            }),
            Control::Pan => serde_json::json!({ "track": track.guid, "pan": track.pan }),
            _ => return,
        };
        crate::mcp::run(&crate::mcp::Press {
            id: "TRACK_SET",
            args,
            preview: Edit::Done(0),
        });
    }

    fn flush_drag(&mut self) {
        for (_, edit) in std::mem::take(&mut self.dragged) {
            self.send(edit);
        }
    }

    /// A strip control pressed: shown here at once, and its action run
    /// ([`crate::mcp::run`]) — which does the work, for the engine and the
    /// arrangement alike.
    fn press(&mut self, press: crate::mcp::Press) {
        predict(&mut self.tracks, &press.preview);
        predict(&mut self.targets, &press.preview);
        self.show_targets();
        crate::mcp::run(&press);
    }

    /// Ask for another frame, where the host needs asking.
    fn redraw(&self) {
        if let Some(redraw) = &self.redraw {
            redraw();
        }
    }

    /// A thrown scroll's next step, if one is moving: carried along and
    /// stopped at either end.
    fn carry_fling(&mut self) {
        let Some(fling) = self.fling.as_mut() else {
            return;
        };
        let ((dx, _), going) = fling.step();
        let most = (self.content_w.get() - self.width.get()).max(0.0);
        let to = (self.scroll.get() + dx).clamp(0.0, most);
        let stopped = !going || to <= 0.0 || to >= most;
        self.scroll.set(to);
        if stopped {
            self.fling = None;
            self.commit_scroll();
        } else {
            self.dirty.set(true);
            self.redraw();
        }
    }

    /// The Signal rows of the strip under a content point: its row, its
    /// track and what the point presses there.
    fn signal_hit_at(&self, x: f64, y: f64) -> Option<(usize, String, crate::signal::view::Hit)> {
        let (cx, cy) = self.content(x, y);
        let mixer = self.mixer.as_ref()?;
        let row = mixer.strip_at(cx)?;
        let (left, _, _) = mixer.strip_box(row)?;
        let rack = mixer.strip(row)?.rack_rect()?;
        if !rack.contains((cx - left, cy)) {
            return None;
        }
        let track = self.track_at(row)?.guid.clone();
        let inserts = crate::signal::closeup::lock(&self.signal);
        let strip = inserts.strip(&track)?;
        let at = crate::tone::Panel::of(rack, left).up(self.rack_scroll);
        let rows = crate::signal::view::rows(&strip.chain, strip.fold);
        let laid = crate::signal::view::layout(&rows, at);
        let hit = crate::signal::view::hit(&laid, cx, cy)?;
        Some((row, track, hit))
    }

    /// A press on a strip's Signal rows: its header opens or shuts it
    /// (`SIGNAL_INSERTS`), a switch flips (`SIGNAL_RUN`), a make-up is taken
    /// hold of, a block opens close up (`SESSION_CLOSEUP {block}`). `false`
    /// where the press is not Signal's.
    fn signal_pressed_at(&mut self, id: BlitzPointerId, x: f64, y: f64) -> bool {
        use crate::signal::view::Hit;
        let Some((row, track, hit)) = self.signal_hit_at(x, y) else {
            return false;
        };
        let hold = match &hit {
            Hit::Fold => {
                let args = serde_json::json!({ "track": track });
                let _ = crate::press::press(crate::signal::INSERTS.id, &args);
                self.follow_signal();
                None
            }
            Hit::PhaseBypass(_) | Hit::BlockBypass(_) => {
                let mut inserts = crate::signal::closeup::lock(&self.signal);
                let edit = inserts
                    .strip(&track)
                    .and_then(|s| crate::signal::view::toggle_edit(&s.chain, &hit));
                if let Some(edit) = edit
                    && let Err(e) = inserts.run(&track, &edit)
                {
                    tracing::warn!(signal.action = edit.action().0, error = %e, "a Signal switch was refused");
                }
                None
            }
            Hit::Makeup(phase) => {
                let inserts = crate::signal::closeup::lock(&self.signal);
                let was = inserts
                    .strip(&track)
                    .and_then(|s| s.chain.phase(phase.as_str()))
                    .map_or(0.0, |p| p.makeup_db);
                let (_, cy) = self.content(x, y);
                Some(Hold::SignalMakeup {
                    track: track.clone(),
                    phase: phase.clone(),
                    from_y: cy,
                    was,
                })
            }
            Hit::Open(block) => {
                let name = self.track_at(row).map(|t| t.name.clone());
                let args =
                    serde_json::json!({ "track": track, "block": block.as_str(), "name": name });
                let _ = crate::press::press(crate::mixer_actions::CLOSEUP.id, &args);
                if let Some(closeups) = &self.closeups {
                    closeups.take_ask();
                }
                None
            }
        };
        let hold = hold.unwrap_or(Hold::Press {
            spot: None,
            from: (x, y),
            select: None,
        });
        self.holds.push((id, hold));
        self.dirty.set(true);
        self.redraw();
        true
    }

    /// Take up `SIGNAL_INSERTS`, re-read the chains whose revision moved
    /// and read the meters when due: once a frame, before painting.
    fn follow_signal(&mut self) {
        let mut inserts = crate::signal::closeup::lock(&self.signal);
        crate::signal::take_asks(&mut inserts, &mut self.signal_asks_seen);
        let followed = inserts.follow();
        for (track, e) in &followed.failed {
            tracing::warn!(signal.track = track.as_str(), error = %e, "a Signal chain could not be read");
        }
        inserts.read_meters(web_time::Instant::now());
        self.signal_seen.set(inserts.generation());
    }

    /// The rack grip under a widget point, with its strip and track.
    fn rack_grip_at(&self, x: f64, y: f64) -> Option<(usize, crate::tone::Grip, String)> {
        let (cx, cy) = self.content(x, y);
        let mixer = self.mixer.as_ref()?;
        let row = mixer.strip_at(cx)?;
        let (left, _, _) = mixer.strip_box(row)?;
        let panel =
            crate::tone::Panel::of(mixer.strip(row)?.rack_rect()?, left).up(self.rack_scroll);
        let track = self.track_at(row)?;
        let store = self.tone.store.borrow();
        let tone = store.get(&track.guid)?;
        let grip = crate::tone::grip_at(
            tone.panels(rack_panels()),
            tone,
            panel,
            self.folds.of(&track.guid),
            cx,
            cy,
        )?;
        Some((row, grip, track.guid.clone()))
    }

    /// Whether a widget point is in a strip's rack box.
    fn in_rack(&self, x: f64, y: f64) -> bool {
        let (cx, cy) = self.content(x, y);
        let Some(mixer) = self.mixer.as_ref() else {
            return false;
        };
        let Some(row) = mixer.strip_at(cx) else {
            return false;
        };
        let (Some((left, _, _)), Some(strip)) = (mixer.strip_box(row), mixer.strip(row)) else {
            return false;
        };
        strip
            .rack_rect()
            .is_some_and(|r| r.contains((cx - left, cy)))
    }

    /// A press in a rack: a switch flips at once, a second press on a
    /// grip puts it back to its default, a modified press on an EQ band
    /// bypasses or reshapes it, and any other grip is taken hold of. A
    /// finger on the rack away from every grip scrolls the chain.
    /// `false` where the press is not the rack's.
    fn rack_pressed_at(
        &mut self,
        id: BlitzPointerId,
        finger: bool,
        x: f64,
        y: f64,
        mods: Mods,
    ) -> bool {
        use crate::tone::Grip;
        let Some((row, grip, guid)) = self.rack_grip_at(x, y) else {
            if finger && self.in_rack(x, y) {
                let (_, cy) = self.content(x, y);
                let was = self.rack_scroll;
                self.holds.push((id, Hold::RackScroll { from_y: cy, was }));
                return true;
            }
            return false;
        };
        let now = web_time::Instant::now();
        let again = self.rack_pressed.is_some_and(|(r, g, at)| {
            r == row && g == grip && now.duration_since(at) < crate::gesture::DOUBLE
        });
        self.rack_pressed = Some((row, grip, now));
        if grip.is_switch() {
            if let Grip::Zoom(which) = grip {
                // The panel, full screen (`SESSION_CLOSEUP`): the shell
                // shows it, here at once.
                if let Some(track) = self.track_at(row) {
                    let args = serde_json::json!({
                        "track": guid,
                        "panel": which.slug(),
                        "name": track.name,
                    });
                    let _ = crate::press::press(crate::mixer_actions::CLOSEUP.id, &args);
                    if let Some(closeups) = &self.closeups {
                        closeups.take_ask();
                    }
                    self.redraw();
                }
            } else if let Grip::Phase(phase) = grip {
                // What the mixer is showing, not a setting of the track:
                // `MIXER_VIEW {rack_fold}`.
                let view = crate::mixer_actions::MixerView {
                    rack_fold: Some(crate::mixer_actions::RackFold {
                        track: guid.clone(),
                        phase: phase.slug().to_owned(),
                    }),
                    ..Default::default()
                };
                self.press_view(&view);
            } else if let Some(tone) = self.tone.store.borrow_mut().edit(&guid) {
                crate::tone::toggle(tone, grip);
            }
            self.invalidate_rack(&guid);
            self.holds.push((
                id,
                Hold::Press {
                    spot: None,
                    from: (x, y),
                    select: None,
                },
            ));
            return true;
        }
        // Locked: a rack's controls stay where they are — the drag scrolls
        // the chain, as a finger on the rack's background does. Its
        // switches still switch (above), and a close-up edits whatever
        // the lock says: going into one is the decision to.
        if crate::options::LOCKING.get() {
            let hold = if finger && self.in_rack(x, y) {
                let (_, cy) = self.content(x, y);
                Hold::RackScroll {
                    from_y: cy,
                    was: self.rack_scroll,
                }
            } else {
                Hold::Press {
                    spot: None,
                    from: (x, y),
                    select: None,
                }
            };
            self.holds.push((id, hold));
            return true;
        }
        if again {
            if let Some(tone) = self.tone.store.borrow_mut().edit(&guid) {
                crate::tone::reset(tone, grip);
            }
            self.invalidate_rack(&guid);
            self.holds.push((
                id,
                Hold::Press {
                    spot: None,
                    from: (x, y),
                    select: None,
                },
            ));
            return true;
        }
        if let Grip::Band(which, index) = grip
            && self
                .tone
                .store
                .borrow_mut()
                .edit(&guid)
                .is_some_and(|tone| crate::tone::dot_click(tone, which, index, mods))
        {
            self.invalidate_rack(&guid);
            self.holds.push((
                id,
                Hold::Press {
                    spot: None,
                    from: (x, y),
                    select: None,
                },
            ));
            return true;
        }
        let last = self.content(x, y);
        self.holds.push((id, Hold::Rack { row, grip, last }));
        self.invalidate_rack(&guid);
        true
    }

    /// Move a rack grip by a travel in layout pixels.
    fn drag_rack(&mut self, row: usize, grip: crate::tone::Grip, dx: f64, dy: f64) {
        let Some(mixer) = self.mixer.as_ref() else {
            return;
        };
        let Some((left, _, _)) = mixer.strip_box(row) else {
            return;
        };
        let Some(rack) = mixer.strip(row).and_then(|s| s.rack_rect()) else {
            return;
        };
        let panel = crate::tone::Panel::of(rack, left).up(self.rack_scroll);
        let Some(guid) = self.track_at(row).map(|t| t.guid.clone()) else {
            return;
        };
        let folded = self.folds.of(&guid);
        if let Some(tone) = self.tone.store.borrow_mut().edit(&guid) {
            let panels = tone.panels(rack_panels());
            crate::tone::drag(tone, grip, panels, panel, folded, Mods::default(), dx, dy);
        }
        self.invalidate_rack(&guid);
    }

    /// How far the racks can scroll: the longest chain's overrun of its
    /// box, since the scroll is shared.
    fn rack_span(&self) -> f64 {
        let Some(mixer) = self.mixer.as_ref() else {
            return 0.0;
        };
        self.rows
            .iter()
            .map(|(track, _)| {
                crate::tone::scroll_span(
                    self.tone
                        .store
                        .borrow()
                        .get(&track.guid)
                        .map_or(rack_panels(), |t| t.panels(rack_panels())),
                    mixer.rack_h,
                    self.folds.of(&track.guid),
                )
            })
            .fold(0.0_f64, f64::max)
    }

    /// Throw a track's cached rack away: its settings moved. Said to the
    /// shared settings too, so a close-up of the same panel follows —
    /// and taken as seen here, since this strip is already redrawn.
    fn invalidate_rack(&mut self, guid: &str) {
        if let Some(analyser) = self.spectra.get_mut(guid) {
            analyser.invalidate();
        }
        self.tone.edited();
        self.tone_seen = self.tone.generation();
    }

    /// The same, for the tracks under some strips' grips.
    fn invalidate_rack_rows(&mut self, rows: &[Option<(usize, crate::tone::Grip)>]) {
        let guids: Vec<String> = rows
            .iter()
            .flatten()
            .filter_map(|(row, _)| self.track_at(*row).map(|t| t.guid.clone()))
            .collect();
        for guid in guids {
            self.invalidate_rack(&guid);
        }
    }

    /// Scroll so the strips under `x` in the overview band are in the
    /// middle of the panel.
    fn overview_to(&mut self, x: f64) {
        let (width, content) = (self.width.get().max(1.0), self.content_w.get());
        let most = (content - width).max(0.0);
        let at = (x / width).clamp(0.0, 1.0) * content - width / 2.0;
        self.scroll.set(at.clamp(0.0, most));
        self.fling = None;
    }

    /// The overview band: every strip at once, each a live meter over its
    /// track's number on its track's colour, and the part of the mixer on screen
    /// outlined — where a mixer wider than the screen is found, and, with
    /// a press, gone to. In CSS pixels, `width` by `band`.
    fn overview(
        &self,
        levels: &[daw_proto::TrackLevels],
        width: f64,
        band: f64,
        (mut apart, scale): (Option<&mut crate::strip_overlay::Apart>, f64),
    ) -> Scene {
        use anyrender::PaintScene as _;
        use vello::kurbo::Rect;
        use vello::peniko::{Color, Fill};
        let mut out = Scene::new();
        let fill = |out: &mut Scene, rect: Rect, color: Color| {
            out.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
        };
        fill(
            &mut out,
            Rect::new(0.0, 0.0, width, band),
            Color::from_rgb8(0x0c, 0x0d, 0x0f),
        );
        let count = self.rows.len();
        if count == 0 {
            return out;
        }
        #[expect(clippy::cast_precision_loss, reason = "a strip count")]
        let cell = width / count as f64;
        let numbered = cell >= 14.0;
        let meter_bottom = if numbered { band - 12.0 } else { band - 4.0 };
        for row in 0..count {
            #[expect(clippy::cast_precision_loss, reason = "a strip index")]
            let x = row as f64 * cell;
            let Some(track) = self.track_at(row) else {
                continue;
            };
            // The track's colour, the whole cell: the mixer's colours at a
            // glance — the drums' red, the keys' green — and where each
            // family starts and ends. Brighter along the foot.
            // The track's own colour, whatever the strips' wash is set to;
            // an uncoloured track the strips' own grey.
            let ground = if track.color.is_some() {
                crate::tcp::track_color(&self.palette, track)
            } else {
                self.palette.tcp_tint
            };
            fill(
                &mut out,
                Rect::new(x + 0.5, 1.0, x + cell - 0.5, band),
                ground.multiply_alpha(0.55),
            );
            fill(
                &mut out,
                Rect::new(x + 0.5, band - 2.0, x + cell - 0.5, band),
                ground,
            );
            // Its level, a sliver up the cell.
            let level = usize::try_from(track.index)
                .ok()
                .and_then(|i| levels.get(i))
                .map_or(0.0, |l| {
                    crate::engine::meter_fraction(l.peak_left.max(l.peak_right))
                });
            let meter_w = (cell * 0.3).clamp(2.0, 5.0);
            let mx = x + (cell - meter_w) / 2.0;
            let top = 3.0;
            let well = Color::from_rgba8(0x0c, 0x0d, 0x0f, 0xb0);
            let column = Rect::new(mx, top, mx + meter_w, meter_bottom);
            fill(&mut out, column, well);
            // A host moving the meters by values: the sliver fully lit
            // here, and what is over it where it is not lit (the band,
            // the cell's colour, the well, as they are drawn) raised by
            // the level.
            if let (Some(apart), Some(index)) = (
                apart.as_deref_mut(),
                usize::try_from(track.index)
                    .ok()
                    .filter(|i| *i < crate::engine::METER_TRACKS),
            ) {
                let level = crate::engine::meter_value(index, "pk");
                // The sliver lit, shown while there is a level (as the
                // window draws one only above 0).
                apart.layers.push(crate::overlay::ValueLayer {
                    scene: {
                        let mut lit = Scene::new();
                        fill(&mut lit, column, Color::from_rgb8(0x4a, 0xde, 0x80));
                        lit
                    },
                    clip: None,
                    moves: vec![crate::overlay::ValueMove::new(
                        crate::overlay::ValueProperty::Visible,
                        &level,
                        1.0,
                        0.5 - 1e-9,
                    )],
                    transform: Some(Affine::scale(scale)),
                    over: false,
                });
                // What is over it where it is not lit, a pixel past it each
                // side so its soft edges are covered: the band, the cell's
                // colour, and the well where it is.
                let wide = Rect::new(
                    (mx - 1.0).floor().max(x + 0.5),
                    (top - 1.0).floor(),
                    (mx + meter_w + 1.0).ceil().min(x + cell - 0.5),
                    meter_bottom,
                );
                let mut cover = Scene::new();
                fill(&mut cover, wide, Color::from_rgb8(0x0c, 0x0d, 0x0f));
                fill(&mut cover, wide, ground.multiply_alpha(0.55));
                fill(&mut cover, column, well);
                let span = meter_bottom - top;
                apart.layers.push(crate::overlay::ValueLayer {
                    scene: cover,
                    clip: Some(wide),
                    moves: vec![
                        crate::overlay::ValueMove::new(
                            crate::overlay::ValueProperty::Y,
                            &level,
                            -span,
                            0.0,
                        )
                        .clamped(-span, 0.0),
                    ],
                    transform: Some(Affine::scale(scale)),
                    over: false,
                });
            }
            let split = apart.is_some()
                && usize::try_from(track.index).is_ok_and(|i| i < crate::engine::METER_TRACKS);
            if !split && level > 0.0 {
                let lit_top = meter_bottom - (meter_bottom - top) * level.clamp(0.0, 1.0);
                fill(
                    &mut out,
                    Rect::new(mx, lit_top, mx + meter_w, meter_bottom),
                    Color::from_rgb8(0x4a, 0xde, 0x80),
                );
            }
            if numbered {
                // The track's own number, as its strip shows it: a mixer
                // with tracks hidden skips numbers.
                let number = (track.index + 1).to_string();
                let size = 8.0_f32;
                let text_w = self.font.width(&number, size);
                crate::tcp::glyphs(
                    &mut out,
                    &self.font,
                    self.palette.text,
                    &number,
                    x + (cell - text_w) / 2.0,
                    band - 4.0,
                    size,
                );
            }
        }
        // What is on screen.
        let content = self.content_w.get().max(1.0);
        let left = self.scroll.get() / content * width;
        let shown = (width / content * width).min(width);
        let window = Rect::new(left, 0.5, (left + shown).min(width), band - 0.5);
        // Over the meters, so with them apart it is a layer over their
        // parts, as it is over the band here.
        let mut over = Scene::new();
        fill(&mut over, window, Color::from_rgba8(0xff, 0xff, 0xff, 0x14));
        over.stroke(
            &vello::kurbo::Stroke::new(1.0),
            Affine::IDENTITY,
            Color::from_rgba8(0xff, 0xff, 0xff, 0x66),
            None,
            &window,
        );
        match apart {
            Some(apart) => apart.layers.push(crate::overlay::ValueLayer {
                scene: over,
                clip: None,
                moves: Vec::new(),
                transform: Some(Affine::scale(scale)),
                over: true,
            }),
            None => anyrender::PaintScene::append_scene(&mut out, over, Affine::IDENTITY),
        }
        out
    }

    /// An edit, to the engine: straight there when the mixer stands alone,
    /// otherwise by way of the arrangement, which applies it too.
    fn send(&self, edit: Edit) {
        match &self.applier {
            Some(applier) => applier.send(edit),
            None => self.links.from_mixer.borrow_mut().push(edit),
        }
    }

    fn track_at(&self, row: usize) -> Option<&daw_proto::Track> {
        self.map.live(&self.tracks, row)
    }

    fn took(&mut self, event: &UiEvent) -> bool {
        let at = |e: &blitz_traits::events::BlitzPointerEvent| {
            (f64::from(e.coords.client_x), f64::from(e.coords.client_y))
        };
        match event {
            UiEvent::PointerMove(e) => {
                let (x, y) = at(e);
                // A mouse let go outside the panel: its release went
                // elsewhere, so whatever it was holding is over. Without
                // this the next move over the mixer carried on a drag
                // nobody was making.
                if e.is_mouse() && e.buttons.is_empty() && self.hold_of(e.id).is_some() {
                    self.let_go(e.id);
                }
                if self.hold_of(e.id).is_some() {
                    return self.moved(e.id, x, y);
                }
                // A finger has no hover: it is only there while it
                // presses.
                if e.is_finger() {
                    return false;
                }
                let rail = self.rail_at(x, y);
                let rail_lit = rail != self.rails.hover;
                self.rails.hover = rail;
                if self.in_rails(x, y) {
                    return self.pointer.hover(None) || rail_lit;
                }
                let spot = self.spot_at(x, y);
                let grip = self.rack_grip_at(x, y).map(|(row, grip, _)| (row, grip));
                let lit = grip != self.rack_hover || rail_lit;
                if lit {
                    self.invalidate_rack_rows(&[self.rack_hover, grip]);
                    self.rack_hover = grip;
                }
                self.pointer.hover(spot) || lit
            }
            UiEvent::PointerDown(e) if e.button == MouseEventButton::Main => {
                let (x, y) = at(e);
                self.pressed(e.id, e.is_finger(), x, y, crate::closeup::mods_of(e));
                true
            }
            UiEvent::PointerUp(e) if e.button == MouseEventButton::Main => {
                let (x, y) = at(e);
                self.released(e.id, e.is_finger(), x, y);
                true
            }
            UiEvent::PointerCancel(e) => {
                self.let_go(e.id);
                true
            }
            // Beside an arrangement, its keymap reads them; alone, the
            // host does (`HostedMixer::key`).
            UiEvent::KeyDown(e) if !self.links.alone => {
                self.links.keys.borrow_mut().push(Key::Down(e.clone()));
                false
            }
            UiEvent::KeyUp(e) if !self.links.alone => {
                self.links.keys.borrow_mut().push(Key::Up(e.clone()));
                false
            }
            _ => false,
        }
    }

    fn hold_of(&self, id: BlitzPointerId) -> Option<usize> {
        self.holds.iter().position(|(held, _)| *held == id)
    }

    /// A press: a turn if it lands on a fader or a knob, otherwise a
    /// press waiting to be a click (or, by a finger, a scroll).
    ///
    /// A finger turns a fader only by its cap, and then one to one, so
    /// the cap stays under it. A mouse takes the whole column, as REAPER
    /// does, at the gearing the mixer always had.
    fn pressed(&mut self, id: BlitzPointerId, finger: bool, x: f64, y: f64, mods: Mods) {
        // A press catches a thrown scroll, as it does on a phone.
        self.fling = None;
        // A second press by the same pointer means its release was lost.
        self.let_go(id);
        // Mixing, the rails take what lands on them.
        if self.in_rails(x, y) {
            self.rails.pressed = self.rail_at(x, y);
            self.dirty.set(true);
            return;
        }
        // The rack sits over the strip's own controls, so it answers
        // first: a press lands on what it looks like it landed on.
        if y >= self.band.get() && self.signal_pressed_at(id, x, y) {
            return;
        }
        if y >= self.band.get() && self.rack_pressed_at(id, finger, x, y, mods) {
            return;
        }
        if y < self.band.get() {
            self.overview_to(x);
            self.holds.push((id, Hold::Overview));
            return;
        }
        let spot = self.spot_at(x, y);
        // Locked: faders and knobs stay where they are, and a drag from
        // one scrolls the strips.
        let locked = crate::options::LOCKING.get();
        let spot = spot.filter(|s| !(locked && s.control.is_continuous()));
        let turn = spot
            .filter(|spot| spot.control.is_continuous())
            .filter(|spot| {
                !finger || spot.control != Control::Volume || self.on_cap(spot.row, x, y)
            })
            .and_then(|spot| {
                let track = self.track_at(spot.row)?.clone();
                let mixer = self.mixer.as_ref()?;
                let height = mixer.strip_box(spot.row)?.2;
                let travel = match mixer.strip(spot.row) {
                    Some(strip) if finger && spot.control == Control::Volume => strip.travel(),
                    _ => height * 0.4,
                };
                Some(Turn {
                    spot,
                    from_y: y,
                    travel,
                    was: track,
                })
            });
        // Taken hold of: what Touch and Latch automation record by.
        if let Some(turn) = &turn {
            self.touch(turn, true);
        }
        let hold = match turn {
            Some(turn) => Hold::Turn(turn),
            // A finger on a groove is not a decision about the level.
            None => Hold::Press {
                spot: spot.filter(|s| !finger || s.control != Control::Volume),
                from: (x, y),
                select: Some(mods.shift || mods.cmd),
            },
        };
        let shown = match &hold {
            Hold::Turn(turn) => Some(turn.spot),
            Hold::Press { spot, .. } => *spot,
            Hold::Pan { .. }
            | Hold::Overview
            | Hold::Rack { .. }
            | Hold::RackScroll { .. }
            | Hold::SignalMakeup { .. } => None,
        };
        self.pointer.hover(shown);
        self.pointer.press();
        self.holds.push((id, hold));
    }

    fn moved(&mut self, id: BlitzPointerId, x: f64, y: f64) -> bool {
        let zoom = self.zoom();
        let Some(i) = self.hold_of(id) else {
            return false;
        };
        // In the strips' own units, for a rack grip's travel.
        let (cx, cy) = self.content(x, y);
        match &mut self.holds[i].1 {
            Hold::Overview => {
                self.overview_to(x);
                true
            }
            Hold::Rack { row, grip, last } => {
                let (row, grip) = (*row, *grip);
                let (dx, dy) = (cx - last.0, cy - last.1);
                *last = (cx, cy);
                self.drag_rack(row, grip, dx, dy);
                true
            }
            Hold::RackScroll { from_y, was } => {
                let (from_y, was) = (*from_y, *was);
                self.rack_scroll = (was - (cy - from_y)).clamp(0.0, self.rack_span());
                for analyser in self.spectra.values_mut() {
                    analyser.invalidate();
                }
                true
            }
            Hold::SignalMakeup {
                track,
                phase,
                from_y,
                was,
            } => {
                let to = crate::signal::view::dragged_makeup(*was, cy - *from_y);
                let (track, phase) = (track.clone(), phase.clone());
                // Shown as it moves; sent once, when let go.
                let mut inserts = crate::signal::closeup::lock(&self.signal);
                inserts.preview_makeup(&track, &phase, to);
                true
            }
            Hold::Turn(turn) => {
                let fraction = crate::gesture::drag_fraction((y - turn.from_y) / zoom, turn.travel);
                let spot = turn.spot;
                if let Some(edit) =
                    crate::mcp::drag(spot.control, &turn.was.guid, &turn.was, fraction)
                {
                    // Shown now, sent once a frame: a drag moves the
                    // pointer many times a frame, and every edit sent is
                    // an engine call and a re-record of the arrangement's
                    // controls.
                    predict(&mut self.tracks, &edit);
                    self.dragged.retain(|(held, _)| *held != spot);
                    self.dragged.push((spot, edit));
                }
                true
            }
            Hold::Press { from, .. } => {
                let (dx, dy) = (x - from.0, y - from.1);
                if id != BlitzPointerId::Mouse && dx.hypot(dy) > crate::touch::SLOP {
                    let from_x = from.0;
                    self.holds[i].1 = Hold::Pan {
                        from_x,
                        was: self.scroll.get(),
                        speed: crate::touch::Speed::default(),
                    };
                    self.pointer.release();
                    self.pointer.hover(None);
                    return self.moved(id, x, y);
                }
                false
            }
            Hold::Pan { from_x, was, speed } => {
                speed.moved((x, y));
                let most = (self.content_w.get() - self.width.get()).max(0.0);
                self.scroll.set((*was - (x - *from_x)).clamp(0.0, most));
                true
            }
        }
    }

    fn released(&mut self, id: BlitzPointerId, finger: bool, x: f64, y: f64) {
        // A rail button acts when it is let go of over itself.
        if let Some(act) = self.rails.pressed.take() {
            if self.rail_at(x, y) == Some(act) {
                self.rail_act(act);
            }
            self.dirty.set(true);
            return;
        }
        let Some(i) = self.hold_of(id) else {
            return;
        };
        let (_, hold) = self.holds.remove(i);
        // A rack's grip, switch or dot let go: what it moved committed
        // (`FX_SET`, `tone::Shared::commit`).
        self.tone.commit();
        let up = self.spot_at(x, y);
        match hold {
            Hold::Turn(turn) => {
                self.flush_drag();
                self.touch(&turn, false);
                self.commit_turn(&turn);
            }
            // Thrown: the strips carry on the way the finger was going.
            Hold::Pan { speed, .. } => {
                let (vx, _) = speed.velocity();
                self.fling = crate::touch::Fling::thrown((-vx, 0.0));
                // Not thrown: it ends here (a throw ends where it stops).
                if self.fling.is_none() {
                    self.commit_scroll();
                }
                self.redraw();
            }
            Hold::Overview => self.commit_scroll(),
            Hold::SignalMakeup {
                track,
                phase,
                from_y,
                was,
            } => {
                let (_, cy) = self.content(x, y);
                let db = crate::signal::view::dragged_makeup(was, cy - from_y);
                let edit = crate::signal::Edit::Makeup { phase, db };
                let mut inserts = crate::signal::closeup::lock(&self.signal);
                if let Err(e) = inserts.run(&track, &edit) {
                    tracing::warn!(signal.action = "SIGNAL_SET", error = %e, "a Signal make-up was refused");
                }
                drop(inserts);
                self.dirty.set(true);
            }
            Hold::RackScroll { .. } => {
                let view = crate::mixer_actions::MixerView {
                    rack_scroll: Some(self.rack_scroll),
                    ..Default::default()
                };
                self.press_view(&view);
            }
            Hold::Press {
                spot: Some(spot), ..
            } if up == Some(spot) => {
                if spot.control == Control::Folder
                    && let Some(track) = self.track_at(spot.row).filter(|t| t.is_folder)
                {
                    // Open or shut: the window's folds, planned again.
                    let open = self
                        .rows
                        .get(spot.row + 1)
                        .zip(self.rows.get(spot.row))
                        .is_some_and(|((_, next), (_, this))| next > this);
                    let press = crate::mcp::Press {
                        id: "SESSION_FOLD",
                        args: serde_json::json!({ "track": track.guid, "open": !open }),
                        preview: Edit::Done(0),
                    };
                    crate::mcp::run(&press);
                    // This process's folds too (see the arrangement's).
                    crate::folds::toggle(&track.guid, open);
                    self.redraw();
                } else if let Some(track) = self.track_at(spot.row).cloned() {
                    // The clip latch clears on a click while it is lit;
                    // otherwise the band is the fader under it.
                    let control = match spot.control {
                        Control::Clip if self.clips.clear(&track.guid) => None,
                        Control::Clip => Some(Control::Volume),
                        other => Some(other),
                    };
                    let press = control.and_then(|c| {
                        let target = self.acts_on(c, &track);
                        crate::mcp::click(c, &target.guid, &target, false)
                    });
                    if let Some(press) = press {
                        self.press(press);
                    }
                }
            }
            // A click on a strip's own ground — not one of its controls —
            // selects its track, as a click on its name does.
            Hold::Press {
                spot: None,
                from,
                select: Some(add),
            } if up.is_none() => {
                if let Some(row) = self
                    .strip_row_at(x, y)
                    .filter(|row| self.strip_row_at(from.0, from.1) == Some(*row))
                    && let Some(track) = self.track_at(row).cloned()
                {
                    self.press(crate::mcp::select(&track.guid, add));
                    self.dirty.set(true);
                }
            }
            Hold::Press { .. } | Hold::Rack { .. } => {}
        }
        self.pointer.release();
        self.pointer.hover(if finger { None } else { up });
    }

    /// A pointer's gesture ended without a release here: cancelled, or
    /// its release lost. What it had moved stays moved.
    fn let_go(&mut self, id: BlitzPointerId) {
        if let Some(i) = self.hold_of(id) {
            let (_, hold) = self.holds.remove(i);
            self.flush_drag();
            self.tone.commit();
            if let Hold::Turn(turn) = hold {
                self.commit_turn(&turn);
            }
            self.pointer.release();
        }
    }
}

/// The panels a rack shows: the Tone phase's — EQ, compression and
/// saturation, with the rest of the chain around them.
fn rack_panels() -> &'static [crate::tone::Which] {
    crate::tone::panels_for(fts_session::mix_phases::MixPhase::Tone)
}

/// An inspector's strips, out of the session's `rows`: the selected track
/// (the first, with none), and the folder it sits in — the strip it goes
/// out through — beside it. Each at the top level: the inspector is two
/// strips, not a piece of the tree.
fn inspect(rows: &[(daw_proto::Track, u32)]) -> Rows {
    let Some(at) = rows
        .iter()
        .position(|(t, _)| t.selected)
        .or((!rows.is_empty()).then_some(0))
    else {
        return Vec::new();
    };
    let slim = |track: &daw_proto::Track| daw_proto::Track {
        width: Some(INSPECTOR_STRIP),
        ..track.clone()
    };
    let (track, depth) = &rows[at];
    let mut out = vec![(slim(track), 0)];
    if *depth > 0
        && let Some((parent, _)) = rows[..at]
            .iter()
            .rev()
            .find(|(t, d)| *d < *depth && t.is_folder)
    {
        out.push((slim(parent), 0));
    }
    out
}

/// An inspector strip's width: the narrow strip — pan, mute and solo over
/// a fader with its meter, the name under it — as Logic's inspector is.
const INSPECTOR_STRIP: u32 = 60;

/// What an edit does to the track it names, predicted here rather than
/// waited for (the engine is in-process but not instant).
pub(crate) fn predict(tracks: &mut [daw_proto::Track], edit: &Edit) {
    // Selection is of the set, not one track: the chosen one on, and —
    // unless added to — every other off.
    match edit {
        Edit::Select(guid) => {
            for track in tracks.iter_mut() {
                track.selected = track.guid == *guid;
            }
            return;
        }
        Edit::AddToSelection(guid) => {
            for track in tracks.iter_mut().filter(|t| t.guid == *guid) {
                track.selected = true;
            }
            return;
        }
        _ => {}
    }
    let (guid, change): (&str, &dyn Fn(&mut daw_proto::Track)) = match edit {
        Edit::ToggleMute(guid) => (guid, &|t| t.muted = !t.muted),
        Edit::ToggleSolo(guid) => (guid, &|t| t.soloed = !t.soloed),
        Edit::ToggleArm(guid) => (guid, &|t| t.armed = !t.armed),
        Edit::SetPhase(guid, on) => (guid, &|t| t.phase_inverted = *on),
        Edit::SetVolume(guid, gain) => (guid, &|t| t.volume = *gain),
        Edit::SetPan(guid, pan) => (guid, &|t| t.pan = *pan),
        Edit::Rename(guid, name) => (guid, &|t| t.name.clone_from(name)),
        Edit::SetInputMonitor(guid, mode) => (guid, &|t| t.input_monitor = *mode),
        _ => return,
    };
    for track in tracks.iter_mut().filter(|t| t.guid == guid) {
        change(track);
    }
}

impl MixerWidget {
    /// One input event: what Blitz's `Widget::handle_event` calls, and what
    /// the web host calls.
    pub fn event(&mut self, event: &UiEvent) {
        let changed = self.took(event);
        self.dirty.set(changed);
    }

    /// The picture: what Blitz's `Widget::paint` returns, and what the web
    /// host draws into its canvas.
    /// The picture with the meters' moving parts apart
    /// ([`crate::strip_overlay::Apart`]): for a host that moves them by an
    /// engine's values, so a playing mixer changes none of the picture.
    /// Composed (the picture, the parts, what is over them) at the levels
    /// the values hold, it is [`Self::paint_scene`]'s picture.
    ///
    /// `buttons`: the engine's values carry the tracks' mute and solo
    /// ([`crate::engine::track_value`]), so their lit state goes apart too.
    pub fn paint_layers(
        &mut self,
        width: u32,
        height: u32,
        scale: f64,
        buttons: bool,
    ) -> (Scene, crate::strip_overlay::Apart) {
        self.apart = Some(crate::strip_overlay::Apart {
            buttons,
            ..crate::strip_overlay::Apart::default()
        });
        let picture = self.paint_scene(width, height, scale);
        (picture, self.apart.take().unwrap_or_default())
    }

    pub fn paint_scene(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.follow_phase();
        self.take_view();
        self.follow_signal();
        self.dirty.set(false);
        self.racks_seen.set(racks_shown());
        self.flush_drag();
        self.catch_up();
        self.carry_fling();
        // Blitz hands a widget its size in device pixels, and the pointer
        // in CSS pixels; the page hands both in CSS pixels (and `scale`
        // 1). Laid out in CSS pixels, so the hit test agrees with the
        // picture, then drawn at the device's scale. Drawing device pixels
        // as if they were CSS put a 2x screen's strips at half size and
        // every tap twice as far along as what it pressed.
        let scale = scale.max(f64::EPSILON);
        let (css_w, full_h) = (f64::from(width) / scale, f64::from(height) / scale);
        self.width.set(css_w);
        // The overview band over the strips: on a touchscreen, and
        // wherever the strips run past the panel.
        #[expect(clippy::cast_precision_loss, reason = "a strip count")]
        let across = self.rows.len() as f64 * (crate::mcp::STRIP_W + crate::mcp::STRIP_GAP);
        let band =
            if !self.links.fill && !self.links.inspector && (self.touch.get() || across > css_w) {
                OVERVIEW_H
            } else {
                0.0
            };
        // Mixing, the strips sit inside the phase rails, at their own
        // size — the frame the scene renders in (`rails::Frame`).
        let mix = self.links.mix.get();
        let recording = self.links.recording.get();
        let mixing = self.links.mixing.get();
        let band = if mix { 0.0 } else { band };
        self.band.set(band);
        let frame = crate::rails::Frame::new(css_w, full_h);
        let origin = if mix {
            (crate::rails::SIDE, crate::rails::TOP)
        } else {
            (0.0, band)
        };
        self.origin.set(origin);
        let (css_w, css_h) = if mix {
            (frame.content_width(), frame.content_height())
        } else {
            (css_w, (full_h - band).max(0.0))
        };
        self.extent.set(full_h);
        let zoom = if mix {
            1.0
        } else {
            self.zoom_for(css_w, css_h)
        };
        self.drawn_zoom.set(zoom);
        // Laid out at the size it has once zoomed, and drawn bigger.
        let (w, h) = (css_w / zoom, css_h / zoom);
        let mut out = Scene::new();
        if w < 1.0 || h < 1.0 {
            return out;
        }
        // The selected track's strip opens to show its rack: given the
        // working width, which turns the racks on, the mixer's own widths
        // open it further (`mcp::widths`). None selected, no racks — the
        // faders keep the whole height.
        // An edit elsewhere — a close-up of one of these panels — leaves
        // every cached rack stale.
        if self.tone.generation() != self.tone_seen {
            self.tone_seen = self.tone.generation();
            for analyser in self.spectra.values_mut() {
                analyser.invalidate();
            }
        }
        let tone = Rc::clone(&self.tone);
        let store = tone.store.borrow();
        // No rack in a filled mixer, nor in the inspector's slim strips,
        // nor anywhere while the racks are hidden (`racks_shown`).
        let racks = self.racks_seen.get();
        let open = if self.links.fill || self.links.inspector || !racks {
            None
        } else {
            self.tracks
                .iter()
                .find(|t| t.selected)
                .map(|t| t.guid.clone())
        };
        let stale = self.mixer.as_ref().is_none_or(|m| {
            (m.height - h).abs() > 0.5
                || m.touch != self.touch.get()
                || m.skin_version != crate::skin::version()
                || m.live == (mixing || recording)
                || m.recording != recording
                || m.mixing != mixing
        }) || open != self.built_open
            || racks != self.built_racks;
        if stale {
            self.built_open.clone_from(&open);
            self.built_racks = racks;
            // The theme may have changed, and with it the colours the
            // painters draw what it does not.
            self.palette = crate::skin::palette(&crate::palette::Palette::from_theme(
                &daw_ui::theming::Theme::dark(),
            ));
            for analyser in self.spectra.values_mut() {
                analyser.invalidate();
            }
            let project = daw_ui::studio::ProjectRef(std::sync::Arc::new(
                daw_ui::studio::project::Project::default(),
            ));
            let mut rows = self.rows.clone();
            // Mixing with nothing selected, the scene's own focus stands.
            let keep_scene = mix && open.is_none();
            for (track, _) in &mut rows {
                if keep_scene {
                    continue;
                }
                let selected = open.as_deref() == Some(track.guid.as_str());
                track.selected = selected;
                if selected {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a strip width"
                    )]
                    let working = crate::tone::WORKING as u32;
                    track.width = Some(track.width.unwrap_or(0).max(working));
                }
            }
            let rows = daw_ui::studio::RowsRef(std::sync::Arc::new(rows));
            // Laid out whole, recorded as it is drawn: below, a frame's
            // worth at a time.
            self.mixer = Some(crate::mcp::Mixer::lay_out(
                &self.palette,
                &self.font,
                &project,
                &rows,
                h,
                self.layout,
                // Mixing, every strip wide enough has its rack.
                if !racks {
                    &[]
                } else if mix {
                    crate::tone::panels_for(self.rails.phase)
                } else if open.is_some() {
                    rack_panels()
                } else {
                    &[]
                },
                open.is_some() && !mix,
                crate::settings::Settings {
                    touch_strips: self.touch.get(),
                    // Mixing or recording, the full strip.
                    live_strips: !(mixing || recording),
                    mixing,
                    recording,
                    focus_selected: self.rails.settings.focus_selected,
                    take_focus_width: self.rails.settings.take_focus_width,
                    ..crate::settings::Settings::default()
                },
                &store,
            ));
        }
        // The strips on screen (and a couple either side) not recorded
        // yet: as many as fit this frame's budget, the rest next frame.
        // The mixer opens at once and fills in over a few frames, rather
        // than holding the window while a hundred strips are recorded.
        if let Some(mixer) = self.mixer.as_mut() {
            let at = (self.scroll.get() / zoom).max(0.0);
            let near = mixer.visible((at - NEAR).max(0.0), NEAR.mul_add(2.0, w));
            let until = web_time::Instant::now().checked_add(RECORD_BUDGET);
            if mixer.record(near, &self.palette, &self.font, &store, until) {
                self.dirty.set(true);
            }
        }
        let Some(mixer) = self.mixer.as_ref() else {
            return out;
        };
        // CSS pixels, as the scroll is.
        self.content_w.set(mixer.content_width() * zoom);
        {
            let mut strips = self.strips.borrow_mut();
            strips.clear();
            for (row, (track, _)) in self.rows.iter().enumerate() {
                if let Some((left, width, _)) = mixer.strip_box(row) {
                    strips.push((track.guid.clone(), left * zoom, width * zoom));
                }
            }
        }
        let most = (mixer.content_width() - w).max(0.0) * zoom;
        let screen_scroll = self.scroll.get().clamp(0.0, most);
        self.scroll.set(screen_scroll);
        let scroll = screen_scroll / zoom;
        let held = self.simulated_at;
        let levels = match &mut self.simulated {
            // On screen only: what is not drawn is not fed.
            Some((since, feed)) => feed.feed(
                held.unwrap_or_else(|| since.elapsed().as_secs_f64()),
                &self.rows,
                mixer.visible(scroll, w),
                &store,
                &mut self.levels,
                &mut self.spectra,
            ),
            None => self
                .meters
                .as_ref()
                .map(crate::engine::Meters::levels)
                .unwrap_or_default(),
        };
        let at = Affine::scale(scale)
            * Affine::translate(origin)
            * Affine::scale(zoom)
            * Affine::translate((-scroll, 0.0));
        mixer.replay(&mut out, scroll, w, at);
        crate::strip_overlay::controls(
            &mut out,
            &self.palette,
            &self.font,
            mixer,
            &self.tracks,
            &self.map,
            &self.pointer,
            &levels,
            &self.clips,
            self.rack_scroll,
            &mut crate::strip_overlay::Racks {
                settings: &store,
                history: &mut self.levels,
                spectra: &mut self.spectra,
                lit: self
                    .holds
                    .iter()
                    .find_map(|(_, hold)| match hold {
                        Hold::Rack { row, grip, .. } => Some((*row, *grip)),
                        _ => None,
                    })
                    .or(self.rack_hover),
                panels: rack_panels(),
                folded: &self.folds,
                signal: Some(&mut crate::signal::closeup::lock(&self.signal)),
            },
            scroll,
            w,
            at,
            self.apart.as_mut(),
        );
        self.painted_levels.clone_from(&levels);
        if band > 0.0 {
            let mut apart = self.apart.take();
            let overview = self.overview(&levels, css_w, band, (apart.as_mut(), scale));
            self.apart = apart;
            anyrender::PaintScene::append_scene(&mut out, overview, Affine::scale(scale));
        }
        // Mixing: the phase rails around the strips, over them — the frame
        // the scene renders in.
        if mix {
            let profile = self.rail_profile();
            let mut rails = Scene::new();
            crate::rails::draw(
                &mut rails,
                &self.palette,
                &self.font,
                &mut crate::icons::Icons::none(),
                (self.rails.hover, self.rails.pressed),
                frame,
                &profile.left,
                &profile.right,
                &profile.top,
            );
            anyrender::PaintScene::append_scene(&mut out, rails, Affine::scale(scale));
        }
        out
    }
}

impl Widget for MixerWidget {
    fn handle_event(&mut self, event: &UiEvent) {
        self.event(event);
    }

    fn needs_redraw(&self) -> bool {
        // The meters, while the transport moves; a fold or a tone change
        // made off the UI thread.
        // A theme file saved since the last look: the mixer re-records.
        crate::skin::poll()
            || self
                .mixer
                .as_ref()
                .is_some_and(|m| m.skin_version != crate::skin::version())
            || self.dirty.get()
            || crate::engine::moving()
            // A simulated signal moves as a playing one does.
            || self.simulated.is_some()
            || crate::folds::generation() != self.folds_seen
            || self.tone.generation() != self.tone_seen
            // The racks shown or hidden.
            || racks_shown() != self.racks_seen.get()
            // A `MIXER_VIEW` asked of it.
            || crate::view_actions::pending(&crate::mixer_actions::MIXER_ASKS, self.view_seen)
            // A Signal chain moved, its meters are due, or `SIGNAL_INSERTS`.
            || crate::view_actions::pending(&crate::signal::INSERTS_ASKS, self.signal_asks_seen)
            || crate::signal::closeup::lock(&self.signal)
                .due(web_time::Instant::now(), self.signal_seen.get())
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.paint_scene(width, height, scale)
    }
}

//! The Arrangement panel's own behaviour, host-free.
//!
//! What the panel does with input that is not the widget's: the zoom
//! spring (`z`) and its drag and wheel, the middle-button hand, the wheel
//! scroll, the zoom requests the keys make, the which-key popup, the
//! mixer's toggle and the edits each way. It used to be written directly
//! against winit's events in the dioxus-native component; it is the same
//! logic in the browser, so it lives here and each host translates its own
//! events into a [`PanelEvent`]:
//!
//! - native (`studio::Arrangement`, dioxus-native): winit window events;
//! - web (`web_host`, dioxus-web): DOM events on the panel's canvas.
//!
//! The chrome drawn over the widget — the main toolbar, the popup, the
//! scrollbars — is ordinary DOM and shared too ([`PanelChrome`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;

use crate::arrange_camera::ArrangeCamera;
use crate::studio::{PPS, StudioSession};

/// The scrollbar's thickness: a hairline that says where the view is,
/// not a control to aim at — a finger or a wheel scrolls the lanes
/// themselves, and a thumb still takes a drag.
pub const BAR: f64 = 5.0;
/// One wheel notch, for a wheel that counts in lines.
pub const WHEEL_LINE: f64 = 40.0;

/// A mouse button, as the panel cares about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Other,
}

/// What a host tells the panel. Positions are in the host's window (or
/// page) coordinates, the same ones the panel's rectangle is read in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelEvent {
    /// The `z` key, by its physical position (Shift+z is a zoom too).
    ZKey {
        pressed: bool,
        repeat: bool,
    },
    Modifiers(crate::mousemap::Mods),
    Pointer {
        x: f64,
        y: f64,
    },
    Button {
        button: Button,
        pressed: bool,
    },
    /// A wheel, already in pixels (a line-counting wheel times
    /// [`WHEEL_LINE`]).
    Wheel {
        dx: f64,
        dy: f64,
    },
    /// Two fingers pinching: the zoom across grows by `by` (0.1 is ten
    /// percent wider), about where they are.
    Pinch {
        by: f64,
    },
    /// Once a frame, with where the play cursor is.
    Frame {
        play_at: f64,
    },
}

/// What the pointer and the keyboard are doing to the view.
#[derive(Default)]
struct Input {
    /// Whether the zoom spring (`z`) is held.
    zooming: bool,
    shift: bool,
    pointer: (f64, f64),
    drag: Option<Drag>,
    /// The middle button, held down over this panel: the hand, whether
    /// or not the pointer has moved yet.
    panning: bool,
}

impl Input {
    /// The tool up, if any: the hand while the middle button is held (it
    /// was pressed on purpose, mid-zoom or not), else the zoom spring.
    fn tool(&self) -> crate::tool::Tool {
        if self.panning {
            crate::tool::Tool::Pan
        } else if self.zooming {
            crate::tool::Tool::Zoom
        } else {
            crate::tool::Tool::Map
        }
    }
}

/// A drag in flight.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    /// The hand: the session moves under the pointer.
    Pan,
    /// The zoom tool, sprung from a held `z`: it zooms about where it was
    /// pressed, since the pointer moves while it zooms and a view that
    /// chased it would slide as it grew.
    Zoom(crate::arrange_camera::ZoomGesture),
}

/// The scroll that keeps the point under the pointer where it is across a
/// zoom from `was` to `to`: zooming goes where the pointer points, rather
/// than toward the top-left corner.
///
/// `at` is the pointer along the axis, measured from the frame's own
/// origin (the lanes' left edge, or the top below the ruler); `scroll` is
/// the scroll in zoomed pixels before the zoom. The session point under
/// the pointer is `(scroll + at) / was` either side of the zoom, so the
/// new scroll is that point times `to`, less `at`.
#[must_use]
pub fn zoom_about(at: f64, scroll: f64, was: f64, to: f64) -> f64 {
    if was <= 0.0 {
        return scroll;
    }
    (scroll + at) / was * to - at
}

/// A wheel or drag distance as a zoom factor: the zoom tool's own, so a
/// benchmark that sweeps a drag zooms by what a hand would.
#[must_use]
pub fn factor(pixels: f64) -> f64 {
    timeline::gestures::zoom_factor(pixels)
}

thread_local! {
    /// The panel on screen's edit sender ([`ArrangementPanel::send_edits`]),
    /// for [`send_pending`].
    static SENDER: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Send what the panel's widgets have queued to the engine now, frame or
/// no frame.
///
/// The window sends at the start of each frame; a window that draws none
/// (hidden, minimized, on another desktop) never would, and an edit made
/// by an action over MCP would wait in it — the shell's loop calls this
/// between frames. On the UI thread, as the panel is.
pub fn send_pending() {
    SENDER.with(|s| {
        if let Some(send) = s.borrow().as_ref() {
            send();
        }
    });
}

/// Where edits go: the engine, however this host reaches it.
pub type Engine = Rc<dyn Fn(crate::engine::Edit)>;

/// The panel: its view (signals the chrome renders from), and what it
/// shares with the widget.
#[derive(Clone)]
pub struct ArrangementPanel {
    pub scroll: Signal<f64>,
    pub down: Signal<f64>,
    pub zoom: Signal<(f64, f64)>,
    /// The panel's rectangle in the window, read back from the layout, in
    /// CSS pixels. What the panel works in is this over [`Self::ui`]
    /// ([`ArrangementPanel::units`]).
    pub rect: Signal<(f64, f64, f64, f64)>,
    /// How much bigger the arrangement is drawn than it is laid out:
    /// touch mode's [`crate::touch::ARRANGE_ZOOM`], or 1. Shared with the
    /// widget, which draws at it.
    pub ui: Rc<Cell<f64>>,
    /// The size the widget was last painted at, in the panel's units: when
    /// it is not [`Self::units`]' size the rectangle is stale (a dock
    /// opened or hidden below it) and is measured at once.
    painted: Rc<Cell<(f64, f64)>>,
    /// A measurement on its way, so a resize asks for one at a time.
    measuring: Rc<Cell<bool>>,
    span_x: f64,
    span_y: Signal<f64>,
    pub which_shown: Signal<Option<crate::which_key::WhichKey>>,
    content_h: Rc<Cell<f64>>,
    pointing: crate::tool::Shared,
    which: crate::which_key::Shared,
    zooms: crate::zoom::Requests,
    history: Rc<RefCell<crate::zoom::History>>,
    view: crate::widget::Shared,
    edits: Rc<RefCell<Vec<crate::engine::Edit>>>,
    input: Rc<RefCell<Input>>,
    engine: Engine,
    mixer: Option<crate::mixer_panel::Links>,
    /// The shell's pins, when there is a shell: what a quantize panel
    /// asked for from the Action List turns.
    pins: Option<crate::shell::Pins>,
    /// The panel's shape, toggled by the toolbar and read by the widget.
    pub compact: Rc<Cell<bool>>,
    /// Whether the window is in Record mode (from its mode, each render).
    pub recording: Rc<Cell<bool>>,
    /// The same, as a signal: what re-renders the chrome over the panel
    /// (the toolbar's width, the scrollbar's left end) when it changes.
    pub shape: Signal<bool>,
    /// Whether the view follows the play cursor (the toolbar's toggle),
    /// and where the cursor was last frame — following acts on the
    /// cursor MOVING, so a view scrolled away from a stopped song stays
    /// where it was put.
    pub follow: Rc<Cell<bool>>,
    followed_at: Rc<Cell<f64>>,
    /// The last jump asked of the transport this view has seen
    /// ([`crate::engine::jumps`]), and how many frames are left for the
    /// cursor to land: a jump is followed even while stopped.
    jumps_seen: Rc<Cell<u64>>,
    landing: Rc<Cell<u32>>,
    /// The panel's outer node (measured) and the widget's (focused).
    pub mounted: Rc<RefCell<Option<Rc<MountedData>>>>,
    pub focus_node: Rc<RefCell<Option<Rc<MountedData>>>>,
    measured: Rc<Cell<Option<web_time::Instant>>>,
    /// The generated Click track, for the toolbar's metronome.
    click: (Option<String>, bool),
    /// The song this panel shows and whether it is docked beside more:
    /// where its view is kept between mounts ([`Kept`]).
    song: std::sync::Weak<daw_ui::studio::project::Project>,
    docked: bool,
    /// The view has been put where it is meant to be — fitted to the song,
    /// or taken up from where it was — and so is worth keeping.
    settled: Rc<Cell<bool>>,
    /// Ask the window for a frame: the rectangle measured, the fit it was
    /// waiting for is due now, not at the next touch.
    redraw: Option<Rc<dyn Fn()>>,
}

/// Where an arrangement was looked at from when it was last on screen,
/// kept per song and per shape (docked beside more, or a view of its
/// own): switching back to it opens there on its first frame, rather
/// than at zoom one and fitted a moment later.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Kept {
    scroll: f64,
    down: f64,
    zoom: (f64, f64),
    rect: (f64, f64, f64, f64),
}

type Song = std::sync::Weak<daw_ui::studio::project::Project>;

thread_local! {
    /// By the song's project (held weakly: a song closed is forgotten,
    /// and its allocation is never another song's while it is here).
    static KEPT: RefCell<Vec<(Song, bool, Kept)>> = const { RefCell::new(Vec::new()) };
}

/// Where `song`'s arrangement was left, if it has been shown.
fn kept(song: &Song, docked: bool) -> Option<Kept> {
    KEPT.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|(each, ..)| each.strong_count() > 0);
        all.iter()
            .find(|(each, d, _)| *d == docked && each.ptr_eq(song))
            .map(|(.., view)| *view)
    })
}

/// Keep where `song`'s arrangement is now.
fn keep(song: &Song, docked: bool, view: Kept) {
    KEPT.with(|all| {
        let mut all = all.borrow_mut();
        match all
            .iter_mut()
            .find(|(each, d, _)| *d == docked && each.ptr_eq(song))
        {
            Some(slot) => slot.2 = view,
            None => all.push((song.clone(), docked, view)),
        }
    });
}

impl PartialEq for ArrangementPanel {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.input, &other.input)
    }
}

/// A handle on the panel for a host that drives its view itself — a
/// benchmark sweeping a gesture, a window animating one — rather than
/// through input. Provided above the panel; [`use_arrangement_panel`]
/// fills it with the panel it builds. The app provides none.
#[derive(Clone, Default)]
pub struct Slot(pub Rc<RefCell<Option<ArrangementPanel>>>);

impl PartialEq for Slot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Build the panel and its widget, once. `host` wraps the widget the way
/// the host mounts it (dioxus-native's `CustomWidgetAttr`, or the web
/// canvas's handle), and what it returns comes back with the panel.
pub fn use_arrangement_panel<H: Clone + 'static>(
    cursor: Option<crate::tool::Sink>,
    engine: Engine,
    host: impl FnOnce(crate::widget::ArrangementWidget) -> H,
) -> (ArrangementPanel, H) {
    let session: StudioSession = use_context();
    // The docked mixer's, when the view has one beside this panel.
    let mixer: Option<crate::mixer_panel::Links> = try_use_context();
    let pins: Option<crate::shell::Pins> = try_use_context();
    // The rows' height at zoom 1: showing or hiding tracks (the visibility
    // manager) changes it, and the widget reports the new one here.
    let content_h = use_hook(|| {
        let layout = crate::layout::Layout::from_env();
        Rc::new(Cell::new(
            session
                .rows
                .iter()
                .map(|(track, _)| layout.slot_of(track))
                .sum::<f64>(),
        ))
    });
    let pointing = use_hook(|| crate::tool::Pointing::shared(cursor));
    let which = use_hook(crate::which_key::Shared::default);
    let zooms = use_hook(crate::zoom::Requests::default);
    // The panel's shape, shared with the toolbar that toggles it. The
    // view decides where it starts: compact where the arrangement is
    // docked beside more (the Overview) or the screen is small (a phone),
    // full in the DAW view.
    let docked = mixer.as_ref().is_some_and(|links| links.docked);
    let small = crate::compact::use_form().compact();
    let touch = crate::touch::use_touch();
    // A finger's screen starts compact too: at touch size the full panel
    // would take half an iPad's width from the lanes.
    let compact = use_hook(|| Rc::new(Cell::new(docked || small || touch)));
    // The window's mode, read every render (so a change re-renders):
    // the track panel's arm only in Record mode. No mode to read — a
    // panel outside the window's shell — and it stays as it was.
    let recording = use_hook(|| Rc::new(Cell::new(true)));
    let mode = try_use_context::<Signal<session::modes::Mode>>().map(|mode| mode());
    recording.set(mode.is_none_or(|mode| mode == session::modes::Mode::Record));
    // And Mix mode: every item in its track's colour.
    let mixing = use_hook(|| Rc::new(Cell::new(false)));
    mixing.set(mode == Some(session::modes::Mode::Mix));
    // And Edit mode — and Record mode, where takes are comped as they
    // come in: every cut on a folder's row, what the drum detection found
    // drawn over the kit, and the comp strip to pick takes on.
    let editing = use_hook(|| Rc::new(Cell::new(false)));
    editing.set(matches!(
        mode,
        Some(session::modes::Mode::Edit | session::modes::Mode::Record)
    ));
    crate::folds::show_cuts(editing.get());
    let ui = use_hook(|| Rc::new(Cell::new(crate::touch::arrange_zoom(touch, 0.0))));
    let painted = use_hook(|| Rc::new(Cell::new((0.0_f64, 0.0_f64))));
    crate::ruler::set_slim(touch);
    let shape = use_signal(|| compact.get());
    // On by default: the view shows where the song is while it plays —
    // and only then (`follow_cursor`): stopped, it stays where it was.
    let follow = use_hook(|| Rc::new(Cell::new(true)));
    let followed_at = use_hook(|| Rc::new(Cell::new(0.0_f64)));
    let jumps_seen = use_hook(|| Rc::new(Cell::new(crate::engine::jumps())));
    let landing = use_hook(|| Rc::new(Cell::new(0_u32)));
    let history = use_hook(|| Rc::new(RefCell::new(crate::zoom::History::default())));
    let which_shown = use_signal(|| None::<crate::which_key::WhichKey>);
    let focus_node = use_hook(|| {
        mixer.as_ref().map_or_else(
            || Rc::new(RefCell::new(None::<Rc<MountedData>>)),
            |links| Rc::clone(&links.arrange_node),
        )
    });

    // Where this song's arrangement was left, if it has been on screen:
    // it opens there, and is not fitted again.
    let song = use_hook(|| std::sync::Arc::downgrade(&session.planner.raw));
    let was = use_hook(|| kept(&song, docked));

    // The widget, built once. Its view — scroll, zoom, where the play
    // cursor is — is a plain cell it reads every paint, because the paint
    // runs outside the Dioxus runtime.
    let (hosted, view, edits) = use_hook(|| {
        let opening = was.map_or(crate::widget::View::OPENING, |at| crate::widget::View {
            scroll_x: at.scroll,
            scroll_y: at.down,
            zoom_x: at.zoom.0,
            zoom_y: at.zoom.1,
            play_at: 0.0,
        });
        let view: crate::widget::Shared = Rc::new(RefCell::new(opening));
        // Shown before (back from the mixer, say): the song as it was left
        // and the folders as the window has them now, not the rows the
        // song opened with.
        let left = crate::widget::rows_as_left(&session.planner);
        let (project, rows) = left
            .as_ref()
            .map_or((&session.project, &session.rows), |(p, r)| (p, r));
        let built = crate::widget::ArrangementWidget::for_session(
            project,
            rows,
            &session.previews,
            compact.get(),
            Rc::clone(&view),
            std::env::var("FTS_BLITZ_FPS").is_ok_and(|v| v != "0"),
        )
        .with_pointing(Rc::clone(&pointing))
        .with_view_links(Rc::clone(&which), Rc::clone(&zooms))
        .with_planner(session.planner.clone(), Rc::clone(&content_h))
        .with_compact(Rc::clone(&compact))
        .with_recording(Rc::clone(&recording))
        .with_mixing(Rc::clone(&mixing))
        .with_editing(Rc::clone(&editing))
        .with_touch(touch)
        .with_ui(Rc::clone(&ui))
        .with_redraw(crate::touch::redraw_hook())
        .with_painted(Rc::clone(&painted))
        .with_fit_on_open(was.is_none());
        let built = match &mixer {
            Some(links) => built.with_mixer(crate::widget::MixerLinks {
                toggle: Rc::clone(&links.toggle),
                rows: Rc::clone(&links.rows),
                echo: Rc::clone(&links.to_arrange),
                keys: Rc::clone(&links.keys),
            }),
            None => built,
        };
        let edits = built.edits();
        (host(built), view, edits)
    });

    let scroll = use_signal(|| was.map_or(0.0, |at| at.scroll));
    let down = use_signal(|| was.map_or(0.0, |at| at.down));
    let zoom = use_signal(|| was.map_or((1.0, 1.0), |at| at.zoom));
    let rect = use_signal(|| was.map_or((0.0, 0.0, 0.0, 0.0), |at| at.rect));
    let settled = use_hook(|| Rc::new(Cell::new(was.is_some())));
    let redraw = use_hook(crate::touch::redraw_hook);
    let span_y = use_signal(|| content_h.get());
    let input = use_hook(|| Rc::new(RefCell::new(Input::default())));
    let mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let measured = use_hook(|| Rc::new(Cell::new(None::<web_time::Instant>)));
    let measuring = use_hook(|| Rc::new(Cell::new(false)));
    let click = session
        .project
        .tracks
        .iter()
        .find(|t| t.name.trim() == "Click")
        .map_or((None, true), |t| (Some(t.guid.clone()), t.muted));

    let panel = ArrangementPanel {
        scroll,
        down,
        zoom,
        rect,
        ui,
        painted,
        measuring,
        span_x: (session.project.length_secs * PPS).max(1.0),
        span_y,
        which_shown,
        content_h,
        pointing,
        which,
        zooms,
        history,
        view,
        edits,
        input,
        engine,
        mixer,
        pins,
        compact,
        recording,
        shape,
        follow,
        followed_at,
        jumps_seen,
        landing,
        mounted,
        focus_node,
        measured,
        click,
        song,
        docked,
        settled,
        redraw,
    };
    let slot: Option<Slot> = try_use_context();
    if let Some(slot) = slot
        && slot.0.borrow().is_none()
    {
        *slot.0.borrow_mut() = Some(panel.clone());
    }
    // The shell sends its edits too, between frames ([`send_pending`]):
    // an edit made with no frame coming (an action over MCP, the window
    // hidden) still reaches the engine. Registered as the panel is built,
    // not as it draws — a window drawing nothing would never register it.
    let sender = panel.clone();
    SENDER.with(|s| *s.borrow_mut() = Some(Box::new(move || sender.send_edits())));
    (panel, hosted)
}

/// The arrangement panel's tree on dioxus-native: the widget's node, and
/// the chrome over it, filling the positioned tile it is in.
///
/// What `studio::Arrangement` renders around the widget once it has
/// wired winit's events to the panel — and what a document with no winit
/// window renders (`bin/blitz_shot`'s pictures and its benchmark), which
/// has no events to wire. One tree, whoever hosts it.
#[cfg(feature = "native")]
#[component]
pub fn NativeTree(panel: ArrangementPanel, widget: dioxus_native_dom::CustomWidgetAttr) -> Element {
    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    let surface = colors.surface.clone();
    let mounted = Rc::clone(&panel.mounted);
    let focus_node = Rc::clone(&panel.focus_node);
    rsx! {
        div {
            // Filling the positioned tile it is in. Absolute rather than
            // `height:100%`: a percentage of a flex item's height does not
            // resolve in Blitz, and the panel came out zero tall.
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{surface};",
            onmounted: move |event| {
                // Where the arrangement is in the window: the pointer is
                // its own to publish (and to draw) while it is over it.
                crate::ghosts::region_mounted(crate::ghosts::ARRANGEMENT, event.data());
                *mounted.borrow_mut() = Some(event.data());
            },
            object {
                style: "position:absolute; left:0; top:0; width:100%; height:100%;",
                // `<object>` is not in Blitz's default-focusable list, and
                // the arrangement takes the keyboard (a rename is a field
                // inside it).
                tabindex: "0",
                data: widget.clone(),
                // The keyboard is the arrangement's from the start: with
                // nothing focused, every shortcut waited for a first click.
                onmounted: move |event| {
                    let node = event.data();
                    *focus_node.borrow_mut() = Some(Rc::clone(&node));
                    async move {
                        let _ = node.set_focus(true).await;
                    }
                },
            }
            PanelChrome { panel: panel.clone() }
        }
    }
}

impl ArrangementPanel {
    /// The panel's rectangle in the units it lays out in: CSS pixels over
    /// the touch zoom. Everything the panel works out — the frame, the
    /// zoom limits, where the pointer is — is in these, as the widget is.
    #[must_use]
    pub fn units(&self) -> (f64, f64, f64, f64) {
        let r = *self.rect.peek();
        let ui = self.ui.get().max(f64::EPSILON);
        // Not measured yet: as big as the widget was painted, which it
        // knows from its first paint — the fit a song opens with is taken
        // in on the second frame, not when a measurement comes back.
        let (w, h) = if r.2 > 0.0 && r.3 > 0.0 {
            (r.2 / ui, r.3 / ui)
        } else {
            self.painted.get()
        };
        (r.0 / ui, r.1 / ui, w, h)
    }

    /// The lanes' frame: the panel less the track column and the ruler on
    /// one side, and the scrollbars on the other.
    #[must_use]
    pub fn frame(&self, r: (f64, f64, f64, f64)) -> (f64, f64) {
        (
            (r.2 - self.tcp().width() - BAR).max(1.0),
            (r.3 - crate::ruler::ruler_h() - BAR).max(1.0),
        )
    }

    /// The view as the arrangement's camera, in the panel's rectangle.
    fn camera(&self, r: (f64, f64, f64, f64)) -> ArrangeCamera {
        ArrangeCamera {
            frame: self.frame(r),
            span: (self.span_x, *self.span_y.peek()),
            zoom: *self.zoom.peek(),
            scroll: *self.scroll.peek(),
            down: *self.down.peek(),
        }
    }

    /// Put the view where `camera` is.
    fn put(&self, camera: ArrangeCamera) {
        let (mut zoom, mut scroll, mut down) = (self.zoom, self.scroll, self.down);
        if *zoom.peek() != camera.zoom {
            zoom.set(camera.zoom);
        }
        scroll.set(camera.scroll);
        down.set(camera.down);
    }

    /// A window point in the lanes' frame.
    fn in_frame(&self, r: (f64, f64, f64, f64), at: (f64, f64)) -> (f64, f64) {
        (
            at.0 - r.0 - self.tcp().width(),
            at.1 - r.1 - crate::ruler::ruler_h(),
        )
    }

    /// How far the view can scroll, at a zoom.
    #[must_use]
    pub fn extent(&self, r: (f64, f64, f64, f64), zx: f64, zy: f64) -> (f64, f64) {
        self.camera(r).extent(zx, zy)
    }

    /// Out no further than the whole session filling the frame, and never
    /// above [`crate::arrange_camera::ZOOM_IN`].
    fn limits(&self, r: (f64, f64, f64, f64)) -> ((f64, f64), (f64, f64)) {
        self.camera(r).limits()
    }

    /// Whether a window point is inside the panel.
    fn inside(&self, p: (f64, f64)) -> bool {
        let r = self.units();
        p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3
    }

    /// A tool going up or down, or a modifier changing what a drag would
    /// do, changes the pointer's shape with the pointer standing still,
    /// when the widget hears nothing. So the panel re-applies it.
    fn reshape(&self, input: &Input) {
        let mut pointing = self.pointing.borrow_mut();
        pointing.tool = input.tool();
        pointing.inside = self.inside(input.pointer);
        pointing.apply();
    }

    /// One event from the host.
    pub fn handle(&self, event: PanelEvent) {
        let r = self.units();
        // The host's CSS pixels, in the panel's units.
        let ui = self.ui.get().max(f64::EPSILON);
        let event = match event {
            PanelEvent::Pointer { x, y } => PanelEvent::Pointer {
                x: x / ui,
                y: y / ui,
            },
            PanelEvent::Wheel { dx, dy } => PanelEvent::Wheel {
                dx: dx / ui,
                dy: dy / ui,
            },
            other => other,
        };
        match event {
            PanelEvent::ZKey { pressed, repeat } => {
                let mut input = self.input.borrow_mut();
                if pressed && !repeat {
                    // A fresh hold: not yet used as the tool.
                    self.pointing.borrow_mut().tool_used = false;
                }
                input.zooming = pressed;
                self.reshape(&input);
            }
            PanelEvent::Modifiers(mods) => {
                let mut input = self.input.borrow_mut();
                input.shift = mods.shift;
                self.pointing.borrow_mut().mods = mods;
                self.reshape(&input);
            }
            PanelEvent::Pointer { x, y } => {
                let at = (x, y);
                let mut input = self.input.borrow_mut();
                let moved = (at.0 - input.pointer.0, at.1 - input.pointer.1);
                input.pointer = at;
                let frame_at = self.in_frame(r, at);
                match input.drag.as_mut() {
                    Some(Drag::Pan) => {
                        let mut camera = self.camera(r);
                        camera.pan(moved);
                        self.put(camera);
                    }
                    Some(Drag::Zoom(gesture)) => {
                        self.pointing.borrow_mut().tool_used = true;
                        // Down zooms in: the rows come toward you as you
                        // pull, as in REAPER's zoom tool.
                        let mut camera = self.camera(r);
                        camera.zoom_to(gesture, frame_at);
                        self.put(camera);
                    }
                    None => {}
                }
                // Over the toolbar or a scrollbar the widget hears nothing,
                // and a tool's shape still has to hold there.
                if input.tool() != crate::tool::Tool::Map || !self.inside(at) {
                    self.reshape(&input);
                }
            }
            PanelEvent::Button { button, pressed } => {
                let mut input = self.input.borrow_mut();
                let over = self.inside(input.pointer);
                if button == Button::Middle {
                    input.panning = pressed && over;
                }
                // A drag starts only over this panel; it ends wherever the
                // button comes up.
                input.drag = match (pressed, button) {
                    (true, Button::Middle) if over => Some(Drag::Pan),
                    (true, Button::Left) if input.zooming && over => Some(Drag::Zoom(
                        self.camera(r).zoom_press(self.in_frame(r, input.pointer)),
                    )),
                    _ => None,
                };
                self.reshape(&input);
            }
            PanelEvent::Pinch { by } => {
                // Time, as a pinch zooms a timeline everywhere else; the
                // rows keep the height the view opened at.
                let input = self.input.borrow();
                if !self.inside(input.pointer) {
                    return;
                }
                let mut camera = self.camera(r);
                camera.pinch(by, self.in_frame(r, input.pointer));
                self.put(camera);
            }
            PanelEvent::Wheel { dx, dy } => {
                let input = self.input.borrow();
                if !self.inside(input.pointer) {
                    return;
                }
                if input.zooming {
                    self.pointing.borrow_mut().tool_used = true;
                }
                // The Editor's wheel too (`timeline::gestures::wheel`). Either
                // axis of the wheel counts for a zoom: macOS turns a
                // shifted wheel sideways, so the notch arrives in `dx`.
                // winit's deltas are the other way round to a browser's.
                let mut camera = self.camera(r);
                camera.wheel(
                    dx,
                    dy,
                    input.zooming,
                    input.shift,
                    self.in_frame(r, input.pointer),
                );
                self.put(camera);
            }
            PanelEvent::Frame { play_at } => self.frame_tick(play_at),
        }
    }

    /// Once a frame: the widget's view, the zooms the keys asked for, the
    /// popup, the edits each way, and the panel's rectangle.
    fn frame_tick(&self, play_at: f64) {
        let r = self.units();
        let (scroll, down, zoom) = (self.scroll, self.down, self.zoom);
        self.follow_cursor(r, play_at);
        // Four numbers, written every time because a missed write is a
        // frame drawn in the wrong place.
        *self.view.borrow_mut() = crate::widget::View {
            scroll_x: *scroll.peek(),
            scroll_y: *down.peek(),
            zoom_x: zoom.peek().0,
            zoom_y: zoom.peek().1,
            play_at,
        };
        // The zooms the keys asked for: the widget has no way to set a
        // signal, so it leaves them here. Held until the panel has been
        // measured — a frame fitted to a zero-sized rectangle is no fit
        // (the fit a song opens with is asked on its first paint).
        let asked: Vec<_> = if r.2 > 0.0 && r.3 > 0.0 {
            self.zooms.borrow_mut().drain(..).collect()
        } else {
            Vec::new()
        };
        for request in asked {
            // A swipe on the track panel: its shape, as the toolbar's
            // switch sets it.
            if let crate::zoom::Request::Shape { compact } = request {
                self.compact.set(compact);
                let mut shape = self.shape;
                if *shape.peek() != compact {
                    shape.set(compact);
                }
                continue;
            }
            let (zx, zy) = *zoom.peek();
            let now = crate::zoom::Target {
                zoom_x: zx,
                zoom_y: zy,
                scroll_x: *scroll.peek(),
                scroll_y: *down.peek(),
            };
            let (fw, fh) = self.frame(r);
            let ((x0, x1), (y0, y1)) = self.limits(r);
            let at = crate::zoom::Frame {
                width: fw,
                height: fh,
                pps: PPS,
                limits_x: (x0, x1),
                limits_y: (y0, y1),
            };
            let framed = || match request {
                crate::zoom::Request::Frame { time, rows, .. } => {
                    crate::zoom::frame(now, at, time, rows)
                }
                crate::zoom::Request::ScrollBy { dx, dy } => crate::zoom::Target {
                    scroll_x: now.scroll_x + dx,
                    scroll_y: now.scroll_y + dy,
                    ..now
                },
                crate::zoom::Request::Open { time, rows, floor } => {
                    let camera = opening_camera(
                        ((fw, fh), (self.span_x, *self.span_y.peek())),
                        now,
                        (time, rows, floor),
                    );
                    crate::zoom::Target {
                        zoom_x: camera.zoom.0,
                        zoom_y: camera.zoom.1,
                        scroll_x: camera.scroll,
                        scroll_y: camera.down,
                    }
                }
                crate::zoom::Request::Pinch { sx, sy, at, dx, dy } => {
                    let zoom_x = (zx * sx).clamp(x0, x1);
                    let zoom_y = (zy * sy).clamp(y0, y1);
                    crate::zoom::Target {
                        zoom_x,
                        zoom_y,
                        scroll_x: zoom_about(at.0, now.scroll_x, zx, zoom_x) + dx,
                        scroll_y: zoom_about(at.1, now.scroll_y, zy, zoom_y) + dy,
                    }
                }
                crate::zoom::Request::Scale { vertical, by } => {
                    // About the middle of the lanes.
                    let mut to = now;
                    if vertical {
                        to.zoom_y = (zy * by).clamp(y0, y1);
                        to.scroll_y = zoom_about(fh / 2.0, now.scroll_y, zy, to.zoom_y);
                    } else {
                        to.zoom_x = (zx * by).clamp(x0, x1);
                        to.scroll_x = zoom_about(fw / 2.0, now.scroll_x, zx, to.zoom_x);
                    }
                    to
                }
                _ => now,
            };
            self.settled.set(true);
            let Some(to) = self.history.borrow_mut().go(now, request, framed) else {
                continue;
            };
            let mut camera = self.camera(r);
            camera.zoom = (to.zoom_x, to.zoom_y);
            camera.scroll = to.scroll_x;
            camera.down = to.scroll_y;
            camera.clamp();
            self.put(camera);
            // And to the widget now, for the paint this frame is about to
            // make: written only at the top, a zoom asked for without an
            // event (a model's) waited for a frame nothing asked for.
            *self.view.borrow_mut() = crate::widget::View {
                scroll_x: *scroll.peek(),
                scroll_y: *down.peek(),
                zoom_x: zoom.peek().0,
                zoom_y: zoom.peek().1,
                play_at,
            };
            // The view cell was written before this; the frame showing
            // the new one is the next.
            if let Some(redraw) = &self.redraw {
                redraw();
            }
        }
        if self.settled.get() && r.2 > 0.0 && r.3 > 0.0 {
            keep(
                &self.song,
                self.docked,
                Kept {
                    scroll: *scroll.peek(),
                    down: *down.peek(),
                    zoom: *zoom.peek(),
                    rect: *self.rect.peek(),
                },
            );
        }
        // Rows shown or hidden: the scroll range follows.
        let mut span_y = self.span_y;
        if self.content_h.get() != *span_y.peek() {
            span_y.set(self.content_h.get());
        }
        // A held `z` used as the tool is the tool, not a prefix: no popup
        // over the zoom.
        let wanted = if self.pointing.borrow().tool_used {
            None
        } else {
            self.which.borrow().clone()
        };
        let mut which_shown = self.which_shown;
        if *which_shown.peek() != wanted {
            which_shown.set(wanted);
        }
        // The Quantize panel, asked for on the Action List.
        #[cfg(feature = "native")]
        if crate::quantize_panel::take_asked()
            && let Some(pins) = self.pins
        {
            let mut shown = pins.quantize;
            let was = *shown.peek();
            shown.set(!was);
        }
        if let Some(links) = &self.mixer {
            // `x`, from either widget.
            if links.toggle.take() {
                if links.switch.is_some() {
                    // The window's keys pressed the mixer's pane already
                    // (`keys::use_window_transport_keys`).
                } else {
                    let mut open = links.open;
                    let was = *open.peek();
                    open.set(!was);
                    // Closing it: the keyboard comes back here, not to a
                    // panel that is no longer on screen.
                    if was && let Some(node) = self.focus_node.borrow().clone() {
                        spawn(async move {
                            let _ = node.set_focus(true).await;
                        });
                    }
                }
            }
        }
        // Whatever the widget asked to have done, to the engine.
        self.send_edits();
        // Where this panel is, re-read a few times a second: a divider or a
        // window resize moves it without telling anything here.
        // And at once when the widget was painted at another size than
        // the one measured: a dock opened or hidden below it, with no
        // frame to come that would re-measure it later.
        let stale = self
            .measured
            .get()
            .is_none_or(|at| at.elapsed() > std::time::Duration::from_millis(200))
            || self.resized();
        if stale
            && !self.measuring.get()
            && let Some(node) = self.mounted.borrow().clone()
        {
            self.measured.set(Some(web_time::Instant::now()));
            self.measure(node);
        }
    }

    /// Read where the panel is from its node, and keep it in [`Self::rect`].
    ///
    /// Read NOW, between frames, where dioxus-native answers at once:
    /// from a spawned task it is asked while the document is busy
    /// rendering, refuses, and the rectangle stayed whatever it was — the
    /// scrollbars left where a dock hidden since had put them. A host
    /// that answers later (the browser) is asked again from a task.
    fn measure(&self, node: Rc<MountedData>) {
        let polled = {
            let mut asked = std::pin::pin!(node.get_client_rect());
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            std::future::Future::poll(asked.as_mut(), &mut cx)
        };
        let (rect, redraw) = (self.rect, self.redraw.clone());
        match polled {
            std::task::Poll::Ready(Ok(got)) => take_rect(rect, redraw.as_ref(), &got),
            // Busy: asked again on the next frame.
            std::task::Poll::Ready(Err(_)) => self.measured.set(None),
            std::task::Poll::Pending => {
                self.measuring.set(true);
                let measuring = Rc::clone(&self.measuring);
                spawn(async move {
                    let got = node.get_client_rect().await;
                    measuring.set(false);
                    if let Ok(got) = got {
                        take_rect(rect, redraw.as_ref(), &got);
                    }
                });
            }
        }
    }

    /// Whether the widget was last painted at another size than the panel
    /// was measured at (by more than rounding).
    fn resized(&self) -> bool {
        let (w, h) = self.painted.get();
        let r = self.units();
        w > 0.0 && h > 0.0 && ((w - r.2).abs() > 1.5 || (h - r.3).abs() > 1.5)
    }

    /// The track panel's shape, as the widget is drawing it.
    #[must_use]
    pub fn tcp(&self) -> crate::tcp::Tcp {
        crate::tcp::Tcp {
            compact: self.compact.get(),
            // The arm only in Record mode; with no mixer to say which
            // mode it is, as it always was.
            recording: self.recording.get(),
            ..crate::tcp::Tcp::FULL
        }
    }

    /// Page the view to the play cursor when following and the cursor has
    /// moved out of it: played past the right edge, or jumped (a click on
    /// the progress bar, a marker, a song picked) to somewhere off screen.
    ///
    /// A page, not a scroll: the cursor lands near the left edge and the
    /// view holds still while it crosses the page, which is readable at any
    /// zoom where a view sliding under the cursor is not.
    fn follow_cursor(&self, r: (f64, f64, f64, f64), play_at: f64) {
        let moved = (self.followed_at.replace(play_at) - play_at).abs() > 1e-6;
        // A jump asked for (a tap on a progress-strip section, Home, End):
        // the cursor's next move is it landing, followed playing or not.
        let jumps = crate::engine::jumps();
        if self.jumps_seen.replace(jumps) != jumps {
            self.landing.set(LANDING_FRAMES);
        }
        let landing = self.landing.get();
        let jumped = landing > 0 && moved;
        self.landing
            .set(if jumped { 0 } else { landing.saturating_sub(1) });
        // Otherwise only while the song plays: stopped, the cursor going
        // back to where play began leaves the view where it was — on what
        // was playing when it stopped.
        if !self.follow.get() || !moved || !(jumped || crate::engine::moving()) {
            return;
        }
        let (zoom_x, zoom_y) = *self.zoom.peek();
        let pps = PPS * zoom_x;
        let (width, _) = self.frame(r);
        // On a page of bars (`[` / `]`) at its zoom: the view goes in steps
        // with the song, the next page as it reaches it, not a scroll.
        if let Some(page) = crate::bar_step::page()
            && on_the_page(page, width, pps)
        {
            if play_at >= page.to || play_at < page.from {
                crate::bar_step::turn_to(play_at);
            }
            return;
        }
        // Read into a local first: a `peek()` in the `if let` below would
        // hold its borrow for the whole block, and `set` inside it then
        // panics with the signal already borrowed.
        let across = *self.scroll.peek();
        let travel = self.extent(r, zoom_x, zoom_y).0;
        if let Some(to) = page_to(play_at * pps, across, width, travel) {
            let mut scroll = self.scroll;
            scroll.set(to);
        }
    }

    /// What the widgets asked to have done, to the engine: the arrangement's
    /// edits and the docked mixer's, each also to the other widget (never
    /// back to the one that made it, or a toggle would flip twice).
    pub fn send_edits(&self) {
        let mut pending: Vec<_> = self.edits.borrow_mut().drain(..).collect();
        if let Some(links) = &self.mixer {
            links.to_mixer.borrow_mut().extend(pending.iter().cloned());
            let from_mixer: Vec<_> = links.from_mixer.borrow_mut().drain(..).collect();
            links
                .to_arrange
                .borrow_mut()
                .extend(from_mixer.iter().cloned());
            pending.extend(from_mixer);
        }
        for edit in pending {
            (self.engine)(edit);
        }
    }

    /// The toolbar's edits: the same queue the widget's go on.
    #[must_use]
    pub fn edits(&self) -> crate::studio::Edits {
        crate::studio::Edits(Rc::clone(&self.edits))
    }
}

/// Whether the view is still showing `page` at its own zoom — not zoomed
/// or framed elsewhere since — so following the song turns its pages.
fn on_the_page(page: crate::bar_step::Page, width: f64, pps: f64) -> bool {
    let (a, b) = page.shown;
    let fits = width / pps.max(f64::EPSILON);
    (fits - (b - a)).abs() <= (b - a) * PAGE_ZOOM_SLACK
}

/// How far the view's span may be from a page's for it to be that page's
/// zoom still, of the span.
const PAGE_ZOOM_SLACK: f64 = 0.02;

/// How many frames a jump has to land in before it is forgotten (about
/// two seconds): the transport answers a seek in a frame or two, and a
/// jump to where the cursor already was never moves it at all, so an old
/// one must not page the view at the next stop.
const LANDING_FRAMES: u32 = 120;

/// The view a song opens on, fitted: the camera over a lanes `frame` and
/// session `span`, from `now`, filled with `time` across and `rows` down
/// no further out than `floor` ([`crate::zoom::Request::Open`]).
///
/// One sum for the two who make it: the widget, on its first paint, so
/// the first frame drawn is the fitted one; and the panel, taking the same
/// request into its view a frame later — where it lands exactly where the
/// widget already drew it.
#[must_use]
pub fn opening_camera(
    (frame, span): ((f64, f64), (f64, f64)),
    now: crate::zoom::Target,
    (time, rows, floor): (Option<(f64, f64)>, Option<(f64, f64)>, (f64, f64)),
) -> ArrangeCamera {
    let mut camera = ArrangeCamera {
        frame,
        span,
        zoom: (now.zoom_x, now.zoom_y),
        scroll: now.scroll_x,
        down: now.scroll_y,
    };
    let ((x0, x1), (y0, y1)) = camera.limits();
    let raised = |(lo, hi): (f64, f64), least: f64| (lo.max(least).min(hi), hi);
    let at = crate::zoom::Frame {
        width: frame.0,
        height: frame.1,
        pps: PPS,
        limits_x: raised((x0, x1), floor.0),
        limits_y: raised((y0, y1), floor.1),
    };
    let to = crate::zoom::frame(now, at, time, rows);
    camera.zoom = (to.zoom_x, to.zoom_y);
    camera.scroll = to.scroll_x;
    camera.down = to.scroll_y;
    camera.clamp();
    camera
}

/// Keep `got` as the panel's rectangle, and when it moved, ask for the
/// frame what waited on it is due in (the fit a song opens with, the
/// scrollbars at the panel's new edges).
fn take_rect(
    mut rect: Signal<(f64, f64, f64, f64)>,
    redraw: Option<&Rc<dyn Fn()>>,
    got: &dioxus::html::geometry::PixelsRect,
) {
    let next = (got.origin.x, got.origin.y, got.size.width, got.size.height);
    if next != *rect.peek() {
        rect.set(next);
        if let Some(redraw) = redraw {
            redraw();
        }
    }
}

/// What is drawn over the widget: the main toolbar, the which-key popup,
/// and the two scrollbars. Ordinary DOM, the same under either host.
#[component]
pub fn PanelChrome(panel: ArrangementPanel) -> Element {
    // Read as a signal, so a resize re-renders the chrome; in the panel's
    // units, and placed in CSS pixels by `ui`.
    let css = (panel.rect)();
    let touch = crate::touch::use_touch();
    let ui = crate::touch::arrange_zoom(touch, css.2);
    panel.ui.set(ui);
    crate::ruler::set_slim(touch);
    let r = (css.0 / ui, css.1 / ui, css.2 / ui, css.3 / ui);
    let (fw, fh) = panel.frame(r);
    let (zx, zy) = (panel.zoom)();
    let travel = panel.extent(r, zx, zy);
    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    let ruler = crate::ruler::ruler_h();
    let (mut scroll, mut down) = (panel.scroll, panel.down);
    let (click, click_muted) = panel.click.clone();
    // Read as a signal so a toggle re-renders what is sized to the panel.
    let tcp_w = crate::tcp::Tcp {
        compact: (panel.shape)(),
        ..crate::tcp::Tcp::FULL
    }
    .width();
    rsx! {
        // The main toolbar, in the corner left of the ruler's lane names.
        crate::toolbar::MainToolbar {
            click,
            click_muted,
            edits: panel.edits(),
            // The toolbar sits over the panel, so it is as wide as
            // whichever shape the panel is in.
            width: (tcp_w - crate::ruler::LABEL_W) * ui,
            compact: Rc::clone(&panel.compact),
            shape: panel.shape,
            follow: Rc::clone(&panel.follow),
            touch,
            mixer: panel.mixer.as_ref().map(|links| links.open),
        }
        crate::which_key::Panel { showing: (panel.which_shown)(), colors: colors.clone() }
        crate::studio::ScrollBar {
            across: true,
            at: scroll(),
            travel: travel.0,
            window: fw,
            left: tcp_w * ui,
            top: (r.3 - BAR).max(0.0) * ui,
            // At the panel's foot, wherever that is now: the bottom dock
            // opening, closing or resizing moves it with the panel.
            bottom: Some(0.0),
            length: fw * ui,
            thick: BAR * ui,
            colors: colors.clone(),
            on_move: move |to: f64| scroll.set(to.clamp(0.0, travel.0)),
        }
        crate::studio::ScrollBar {
            across: false,
            at: down(),
            travel: travel.1,
            window: fh,
            left: (r.2 - BAR).max(0.0) * ui,
            top: ruler * ui,
            length: fh * ui,
            thick: BAR * ui,
            colors,
            on_move: move |to: f64| down.set(to.clamp(0.0, travel.1)),
        }
    }
}

/// Where to page the view to keep the play cursor in sight
/// ([`timeline::scroll::page_to`]).
use timeline::scroll::page_to;

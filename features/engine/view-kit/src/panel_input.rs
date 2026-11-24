//! What a host tells a panel (the arrangement's, the chart's): the
//! pointer, the buttons, the wheel and the frame, in the host's window
//! coordinates, and where the panel's edits go. Shared by every host
//! (dioxus-native's window events, a browser canvas, an extension pane).

use std::cell::RefCell;
use std::rc::Rc;

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

thread_local! {
    /// The panel on screen's edit sender (the arrangement panel's `send_edits`),
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

/// Provided above the panel by a host that wants it to open compact (the
/// track panel at its narrow width) although no mixer is docked beside it
/// in this process: an Architect pane whose mixer is another extension's
/// pane below it. Studio's own window provides none.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StartCompact(pub bool);

/// Make `send` what [`send_pending`] runs: the panel on screen's edits,
/// sent. Registered as a panel is built.
pub fn set_sender(send: Box<dyn Fn()>) {
    SENDER.with(|s| *s.borrow_mut() = Some(send));
}

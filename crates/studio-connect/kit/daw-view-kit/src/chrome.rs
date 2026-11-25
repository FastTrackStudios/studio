//! The window's chrome the views share with the app around them: which
//! view the window shows ([`View`]), the panes pinned under the
//! arrangement ([`Pane`], [`MixerToggle`], [`FxToggle`], [`EditorPane`]),
//! what stays on screen over every view ([`Pins`]), and the colours and
//! sizes the bars and popups are drawn with.
//!
//! The app (studio-panels' `shell`) provides these as contexts; the views
//! (the arrangement, the mixer, the toolbar, the keys) read them with
//! `try_use_context`, and do without when the app gives none.

use dioxus::prelude::*;

pub const RULE: &str = "#2a2c31";
pub const TEXT: &str = "#e5e7eb";
pub const DIM: &str = "#8b9099";

/// The views the bars switch between.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Setup,
    Performance,
    /// The arrangement, and the mixer docked under it when it is open.
    Daw,
    Overview,
    /// The chart alone, the whole window.
    Chart,
    /// The song's lyrics, the whole window.
    Lyrics,
    /// The mixer alone, the whole window.
    Mixer,
    /// The selected track's effects as devices, the whole window
    /// (the mixer's FX view).
    Fx,
    /// The audio and MIDI editor (the expression editor, brought in
    /// next): a place held for it.
    Editor,
}

impl View {
    pub const ALL: [Self; 9] = [
        Self::Performance,
        Self::Overview,
        Self::Chart,
        Self::Lyrics,
        Self::Daw,
        Self::Mixer,
        Self::Fx,
        Self::Editor,
        Self::Setup,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Setup => "Setup",
            Self::Performance => "Performance",
            Self::Daw => "Arrangement",
            Self::Overview => "Overview",
            Self::Chart => "Chart",
            Self::Lyrics => "Lyrics",
            Self::Mixer => "Mixer",
            Self::Fx => "FX",
            Self::Editor => "Editor",
        }
    }

    /// The word under the view's icon in the bottom bar: the name, cut to
    /// a verb where the name is long.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Performance => "Perform",
            Self::Daw => "Arrange",
            other => other.name(),
        }
    }
}

/// The Editor view, until the audio and MIDI editor (the expression
/// editor) is brought in: a place held for it, saying so.
#[component]
pub fn EditorComing() -> Element {
    rsx! {
        div {
            style: "position:absolute; inset:0; display:flex; flex-direction:column; align-items:center; \
                    justify-content:center; gap:10px; color:{DIM}; font-family:system-ui, sans-serif;",
            lucide_dioxus::AudioWaveform { size: 40, color: "currentColor" }
            span { style: "font-size:16px; font-weight:650; color:{TEXT};", "Editor" }
            span {
                style: "font-size:13px; max-width:420px; text-align:center; line-height:1.5;",
                "The audio and MIDI editor comes here: notes, audio, automation, \
                 edited up close for the selected item."
            }
        }
    }
}

/// What stays on screen over every view — the song's progress along the
/// top, the performance transport along the foot — each a setting a
/// switch on the Setup page turns on;
/// a context the shell provides.
#[derive(Clone, Copy, PartialEq)]
pub struct Pins {
    pub progress: Signal<bool>,
    pub transport: Signal<bool>,
    /// Locked: drags scroll, and move nothing (`crate::options::LOCKING`,
    /// which this mirrors so the buttons that show it re-render).
    pub lock: Signal<bool>,
    /// The inspector down the arrangement's right: the selected track's
    /// strip, and its folder's.
    pub inspector: Signal<bool>,
    /// The navigator down the window's left (`studio_panels::shell::NavigatorColumn`).
    pub navigator: Signal<bool>,
    /// The browser down the window's left: what to load on the selected
    /// track (`b`, `studio_panels::browser::BrowserColumn`).
    pub browser: Signal<bool>,
    /// The Quantize panel over the arrangement's right
    /// (`crate::quantize_panel`).
    pub quantize: Signal<bool>,
}

/// A pane that is either a view of its own or pinned under others — the
/// mixer, the FX. One way in for each: its bottom-bar button, its key,
/// its toolbar switch, all [`Pane::press`].
///
/// Not docked (the default), pressing it is its view, the whole window,
/// and pressing it again is the view it came from. Docked — a right-click
/// on its button, on a screen wide enough — it is pinned under the views
/// it docks in (`hosts`), opened and closed there.
#[derive(Clone, Copy, PartialEq)]
pub struct Pane {
    pub view: Signal<View>,
    /// Its own view.
    pub own: View,
    /// The views it is pinned under, docked; the first is where pressing it
    /// from anywhere else goes.
    pub hosts: &'static [View],
    /// Pinned under the hosts rather than a view of its own.
    pub docked: Signal<bool>,
    /// Whether the pinned pane is showing.
    pub open: Signal<bool>,
    /// The view its own view was opened from, to go back to.
    back: Signal<View>,
}

impl Pane {
    fn new(view: Signal<View>, own: View, hosts: &'static [View]) -> Self {
        Self {
            view,
            own,
            hosts,
            docked: Signal::new(false),
            open: Signal::new(false),
            back: Signal::new(View::Daw),
        }
    }

    /// Open it, or close it.
    pub fn press(self) {
        let (mut view, mut open, mut back) = (self.view, self.open, self.back);
        let now = *view.peek();
        if *self.docked.peek() {
            if self.hosts.contains(&now) {
                open.toggle();
            } else {
                view.set(self.hosts.first().copied().unwrap_or(View::Daw));
                open.set(true);
            }
        } else if now == self.own {
            view.set(*back.peek());
        } else {
            back.set(now);
            view.set(self.own);
        }
    }

    /// Its own view, full size, whether or not it is docked — or, from
    /// there, back to where it was opened from.
    pub fn press_full(self) {
        let (mut view, mut back) = (self.view, self.back);
        let now = *view.peek();
        if now == self.own {
            view.set(*back.peek());
        } else {
            back.set(now);
            view.set(self.own);
        }
    }

    /// Whether it is showing, however it is shown.
    #[must_use]
    pub fn showing(self) -> bool {
        if (self.docked)() {
            self.hosts.contains(&(self.view)()) && (self.open)()
        } else {
            (self.view)() == self.own
        }
    }

    /// Pinned or a view of its own, keeping it showing where it moves to.
    pub fn set_docked(self, on: bool) {
        let (mut docked, mut view, mut open) = (self.docked, self.view, self.open);
        if *docked.peek() == on {
            return;
        }
        let showing = self.showing();
        docked.set(on);
        if showing {
            if on {
                view.set(self.hosts.first().copied().unwrap_or(View::Daw));
                open.set(true);
            } else {
                view.set(self.own);
            }
        }
    }
}

/// The mixer's [`Pane`]: pinned under the arrangement.
#[derive(Clone, Copy, PartialEq)]
pub struct MixerToggle(pub Pane);

impl std::ops::Deref for MixerToggle {
    type Target = Pane;
    fn deref(&self) -> &Pane {
        &self.0
    }
}

/// The FX's [`Pane`]: pinned under the arrangement or the mixer, either.
#[derive(Clone, Copy, PartialEq)]
pub struct FxToggle(pub Pane);

impl std::ops::Deref for FxToggle {
    type Target = Pane;
    fn deref(&self) -> &Pane {
        &self.0
    }
}

/// The Editor's [`Pane`]: pinned under the arrangement — the selected
/// track's notes or audio, up close, under where it sits in the song.
#[derive(Clone, Copy, PartialEq)]
pub struct EditorPane(pub Pane);

impl std::ops::Deref for EditorPane {
    type Target = Pane;
    fn deref(&self) -> &Pane {
        &self.0
    }
}

/// Provide the window's [`MixerToggle`], [`FxToggle`] and [`EditorPane`]
/// over `view`, for a shell.
pub fn use_mixer_toggle(view: Signal<View>) -> MixerToggle {
    // The FX starts docked: on a desktop it is a strip under whatever you
    // are working in, and a view of its own when you ask (right-click).
    use_context_provider(|| {
        let pane = Pane::new(view, View::Fx, &[View::Daw, View::Mixer]);
        let mut docked = pane.docked;
        docked.set(true);
        FxToggle(pane)
    });
    // The Editor starts docked too, under the arrangement: you edit a
    // part where it is in the song.
    use_context_provider(|| {
        let pane = Pane::new(view, View::Editor, &[View::Daw]);
        let mut docked = pane.docked;
        docked.set(true);
        EditorPane(pane)
    });
    use_context_provider(|| MixerToggle(Pane::new(view, View::Mixer, &[View::Daw])))
}

/// How wide a window must be for the mixer to be pinned under the
/// arrangement: a desktop's, not a tablet's.
pub const DOCK_W: f64 = 1200.0;

impl Pins {
    #[must_use]
    pub fn new() -> Self {
        Self {
            // The song's progress over every view from the start.
            progress: Signal::new(true),
            transport: Signal::new(false),
            // Locked where the pointer is a finger, where a drag meant
            // to scroll is the common case; unlocked at a desk, where
            // a mouse drag is meant to edit.
            lock: Signal::new(!cfg!(test) && crate::touch::detected()),
            inspector: Signal::new(true),
            navigator: Signal::new(false),
            browser: Signal::new(false),
            quantize: Signal::new(false),
        }
    }
}

impl Default for Pins {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide the window's [`Pins`], for a shell: and the lock, wherever it
/// is switched (the Setup page, a view's own lock button), is what
/// [`crate::options::LOCKING`] says.
pub fn use_pins() -> Pins {
    let pins = use_context_provider(Pins::new);
    let lock = pins.lock;
    use_effect(move || crate::options::LOCKING.set(lock()));
    pins
}

/// A press-outside-closes layer under a popup: the whole screen, from
/// wherever the popup is. Past the screen's edges on every side, because
/// it is placed from its container, not the viewport (Blitz has no
/// `fixed`, so it is `absolute`) — a layer at 0,0 of a picker in the
/// bottom-right corner covered nothing a finger could press. A popup is
/// never under a clipping box (it would be clipped itself), so neither
/// is its scrim, in a browser either.
pub const SCRIM: &str = "position:absolute; top:-100vh; left:-100vw; width:300vw; height:300vh;";

/// How tall the bottom bar is: an icon with its word under it, for a
/// finger.
pub const BOTTOM_H: f64 = 54.0;

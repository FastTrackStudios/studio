//! Studio's views in Architect: which there are, what opens each, and
//! which is lit (studio#409).
//!
//! The owner's list (2026-11-11): the Arrangement (with its track control
//! panel), the Mixer, the Chart (engraved), the Chart Editor (the Keyflow
//! source), the Progress Bar and the Editor (the MIDI editor). Each is a
//! button of the Views toolbar at the bottom of the window, as the Studio
//! iPad app's bottom bar has them (`extension.toml`'s `[[toolbars]]`). A
//! button is an action ([`StudioView::action`]): it opens or focuses its
//! view as a pane of the workspace and is lit while that view is focused
//! ([`checked`]); one whose view is not there yet stays on the bar and
//! refuses with the reason ([`Opens::Missing`]).
//!
//! The model is pure: `ext` (feature `extension`) runs it against the app.

#[cfg(feature = "extension")]
pub mod ext;

/// What a view button opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    /// A view an extension has: opened (or focused) in the workspace.
    View {
        extension: &'static str,
        view: &'static str,
    },
    /// Not a view there yet: why, as the button's action says when run.
    Missing(&'static str),
}

/// A view's button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StudioView {
    /// The action the button runs, a toggle lit while the view is
    /// focused.
    pub action: &'static str,
    pub label: &'static str,
    /// Its icon: one of the names Architect's toolbars draw (its shell's
    /// `named_icon`, a fixed set), the nearest to the Lucide icon Session's
    /// bottom bar draws the view with.
    pub icon: &'static str,
    pub opens: Opens,
}

impl StudioView {
    /// The workspace's name for its view (`studio-setlists/setlist`), when
    /// it has one.
    #[must_use]
    pub fn view_kind(&self) -> Option<String> {
        match self.opens {
            Opens::View { extension, view } => Some(format!("{extension}/{view}")),
            Opens::Missing(_) => None,
        }
    }
}

/// Why the Arrangement, the Mixer and the Editor are not views yet: they
/// draw the song open in Studio's engine.
pub const NEEDS_ENGINE: &str = "needs Studio's engine (the Full layer): Session's panel draws the \
     project open in the engine, which this Architect build does not run. Not a view of the \
     Studio extension yet (studio#409).";

/// Why the Chart, the Chart Editor and the Progress Bar are not views yet:
/// their panels are the song workspace's.
pub const WITH_SONG_WORKSPACE: &str = "is a panel of the song workspace (studio#408), not a view \
     of its own yet: open a song to see it.";

/// The owner's six, in the bar's order.
pub const VIEWS: [StudioView; 6] = [
    StudioView {
        action: "STUDIO_VIEWS_ARRANGEMENT",
        label: "Arrangement",
        icon: "layout-dashboard",
        opens: Opens::Missing(NEEDS_ENGINE),
    },
    StudioView {
        action: "STUDIO_VIEWS_MIXER",
        label: "Mixer",
        icon: "sliders-horizontal",
        opens: Opens::Missing(NEEDS_ENGINE),
    },
    StudioView {
        action: "STUDIO_VIEWS_CHART",
        label: "Chart",
        icon: "file",
        opens: Opens::Missing(WITH_SONG_WORKSPACE),
    },
    StudioView {
        action: "STUDIO_VIEWS_CHART_EDITOR",
        label: "Chart Editor",
        icon: "code",
        opens: Opens::Missing(WITH_SONG_WORKSPACE),
    },
    StudioView {
        action: "STUDIO_VIEWS_PROGRESS",
        label: "Progress Bar",
        icon: "clock",
        opens: Opens::Missing(WITH_SONG_WORKSPACE),
    },
    StudioView {
        action: "STUDIO_VIEWS_EDITOR",
        label: "Editor",
        icon: "music",
        opens: Opens::Missing(NEEDS_ENGINE),
    },
];

/// The views Studio has in Architect today, after a separator on the bar:
/// the setlists extension's.
pub const SETLISTS: [StudioView; 2] = [
    StudioView {
        action: "STUDIO_VIEWS_SONG",
        label: "Song",
        icon: "file-text",
        opens: Opens::View {
            extension: "studio-setlists",
            view: "song",
        },
    },
    StudioView {
        action: "STUDIO_VIEWS_SETLISTS",
        label: "Setlists",
        icon: "list",
        opens: Opens::View {
            extension: "studio-setlists",
            view: "setlist",
        },
    },
];

/// Every button, in the bar's order.
pub fn all() -> impl Iterator<Item = &'static StudioView> {
    VIEWS.iter().chain(SETLISTS.iter())
}

/// The button that runs `action`.
#[must_use]
pub fn by_action(action: &str) -> Option<&'static StudioView> {
    all().find(|v| v.action == action)
}

/// The button lit while `focused_view` (the workspace's focused view,
/// `<extension>/<view>`) is focused.
#[must_use]
pub fn lit(focused_view: &str) -> Option<&'static StudioView> {
    all().find(|v| v.view_kind().as_deref() == Some(focused_view))
}

/// Whether focusing `focused_view` moves the lit button. A toolbar's own
/// pane (`toolbar:<extension>/<id>`) takes the focus when one of its
/// buttons is pressed: the view it opened is still the one in front.
#[must_use]
pub fn moves_lit(focused_view: &str) -> bool {
    !focused_view.starts_with("toolbar:")
}

/// Every toggle's state with `focused_view` focused: the one lit on, the
/// rest off.
#[must_use]
pub fn checked(focused_view: &str) -> Vec<(&'static str, bool)> {
    let on = lit(focused_view).map(|v| v.action);
    all().map(|v| (v.action, Some(v.action) == on)).collect()
}

//! Studio's views in Architect: which there are, what opens each, and
//! which is lit (studio#409).
//!
//! The owner's list (2026-11-11): the Arrangement (with its track control
//! panel), the Mixer, the Chart (engraved), the Chart Editor (the Keyflow
//! source), the Progress Bar and the Editor (the MIDI editor). Each is a
//! button of the Views toolbar at the bottom of the window, in the order,
//! with the words, the Studio iPad app's bottom bar has them
//! (`extension.toml`'s `[[toolbars]]`); these six and no others. Two views
//! have toolbars of their own: the Arrangement's tools on its left
//! ([`ARRANGEMENT_TOOLS`]) and the Chart Editor's options at its bottom
//! ([`CHART_OPTIONS`], [`chart`]). A button is an action ([`StudioView::action`]): it
//! opens or focuses its view as a pane of the workspace and is lit while
//! that view is focused ([`checked`]); one whose view is not there yet
//! stays on the bar and refuses with the reason ([`Opens::Missing`]). None
//! is there yet: each is its own issue (studio#409's PR lists them).
//!
//! The model is pure: `ext` (feature `extension`) runs it against the app.

pub mod chart;
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
///
/// `issue` names what makes it a view; empty once it is one.
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
    /// The issue that makes it a view (`studio#433`), while it is not one.
    pub issue: &'static str,
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
     Studio extension yet.";

/// Why the Chart, the Chart Editor and the Progress Bar are not views yet:
/// their panels are the song workspace's.
pub const WITH_SONG_WORKSPACE: &str = "is a panel of the song workspace (studio#408), not a view \
     of its own yet.";

/// The owner's six, in the bar's order.
///
/// The Studio iPad app's bottom bar's order (`session_ui::shell::BottomBar`:
/// Perform, Chart, Lyrics, Arrange, Mixer, FX, Editor) for the four it has,
/// with its words under them ("Arrange"); the song's two other panels after
/// the Chart.
pub const VIEWS: [StudioView; 6] = [
    StudioView {
        action: "STUDIO_VIEWS_CHART",
        label: "Chart",
        icon: "file",
        opens: Opens::Missing(WITH_SONG_WORKSPACE),
        issue: "studio#435",
    },
    StudioView {
        action: "STUDIO_VIEWS_CHART_EDITOR",
        label: "Chart Editor",
        icon: "code",
        opens: Opens::Missing(WITH_SONG_WORKSPACE),
        issue: "studio#436",
    },
    StudioView {
        action: "STUDIO_VIEWS_PROGRESS",
        label: "Progress",
        icon: "clock",
        opens: Opens::Missing(WITH_SONG_WORKSPACE),
        issue: "studio#437",
    },
    StudioView {
        action: "STUDIO_VIEWS_ARRANGEMENT",
        label: "Arrange",
        icon: "layout-dashboard",
        opens: Opens::Missing(NEEDS_ENGINE),
        issue: "studio#433",
    },
    StudioView {
        action: "STUDIO_VIEWS_MIXER",
        label: "Mixer",
        icon: "sliders-horizontal",
        opens: Opens::Missing(NEEDS_ENGINE),
        issue: "studio#434",
    },
    StudioView {
        action: "STUDIO_VIEWS_EDITOR",
        label: "Editor",
        icon: "music",
        opens: Opens::Missing(NEEDS_ENGINE),
        issue: "studio#438",
    },
];

/// A tool of a view's own toolbar: an action and its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub action: &'static str,
    pub label: &'static str,
}

/// The Arrangement's tools: its toolbar on its left side.
///
/// `extension.toml`'s `arrangement`: what Session's arrangement rail
/// carries (`session_ui::rails::profile`, `Surface::Arrange`), the mix
/// phases (`session::mix_phases::MixPhase::ALL`, in order), then the
/// arrangement's switch, Takes. The scenes the rail also lists are the
/// current mode's for the instrument in front: not a fixed list, so not
/// buttons here (a widget of the Arrangement view's, once it is one).
pub const ARRANGEMENT_TOOLS: [Tool; 9] = [
    Tool {
        action: "STUDIO_VIEWS_PHASE_RESCUE",
        label: "Rescue",
    },
    Tool {
        action: "STUDIO_VIEWS_PHASE_BALANCE",
        label: "Balance",
    },
    Tool {
        action: "STUDIO_VIEWS_PHASE_TONE",
        label: "Tone",
    },
    Tool {
        action: "STUDIO_VIEWS_PHASE_POLISH",
        label: "Polish",
    },
    Tool {
        action: "STUDIO_VIEWS_PHASE_RELATIONAL",
        label: "Relational",
    },
    Tool {
        action: "STUDIO_VIEWS_PHASE_DEPTH",
        label: "Depth",
    },
    Tool {
        action: "STUDIO_VIEWS_PHASE_CREATIVE",
        label: "Creative",
    },
    Tool {
        action: "STUDIO_VIEWS_PHASE_OVERVIEW",
        label: "Overview",
    },
    Tool {
        action: "STUDIO_VIEWS_FOLDED_TAKES",
        label: "Takes",
    },
];

/// The Chart Editor's options, its toolbar at its bottom
/// (`extension.toml`'s `chart-editor`): Keyflow's transposer ([`chart`]).
pub const CHART_OPTIONS: [Tool; 5] = [
    Tool {
        action: "STUDIO_VIEWS_CHART_TRANSPOSE_DOWN",
        label: "−1",
    },
    Tool {
        action: "STUDIO_VIEWS_CHART_TRANSPOSE_UP",
        label: "+1",
    },
    Tool {
        action: "STUDIO_VIEWS_CHART_NOTATION_LETTERS",
        label: "Letters",
    },
    Tool {
        action: "STUDIO_VIEWS_CHART_NOTATION_NASHVILLE",
        label: "Nashville",
    },
    Tool {
        action: "STUDIO_VIEWS_CHART_NOTATION_ROMAN",
        label: "Roman",
    },
];

/// Every button, in the bar's order.
pub fn all() -> impl Iterator<Item = &'static StudioView> {
    VIEWS.iter()
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
    lit_among(&VIEWS, focused_view)
}

/// The one of `views` lit while `focused_view` is focused.
#[must_use]
pub fn lit_among<'a>(views: &'a [StudioView], focused_view: &str) -> Option<&'a StudioView> {
    views
        .iter()
        .find(|v| v.view_kind().as_deref() == Some(focused_view))
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
    checked_among(&VIEWS, focused_view)
}

/// [`checked`] over `views`.
#[must_use]
pub fn checked_among(views: &[StudioView], focused_view: &str) -> Vec<(&'static str, bool)> {
    let on = lit_among(views, focused_view).map(|v| v.action);
    views
        .iter()
        .map(|v| (v.action, Some(v.action) == on))
        .collect()
}

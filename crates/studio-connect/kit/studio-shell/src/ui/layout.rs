//! The shell's layout: which regions are open, how big they are, and which
//! panel each one shows. Pure data, no Dioxus, so it is tested without a
//! renderer and persisted through `utils::prefs`.

use facet::Facet;

/// The five places a panel can live (docs/design/unified-shell/spec.md §1).
///
/// The app bar is not a region: it never collapses and holds no panels.
/// Sidebars run the full height under the app bar; the drawers live inside
/// the main column, so opening one never moves the sidebars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum Region {
    /// What you choose from: the setlist, a browser, a library.
    Left,
    /// What you inspect: the inspector, song info, the chart outline.
    Right,
    /// The view itself. Never collapses.
    Main,
    /// Slim and glanceable: song facts, progress, NOW / NEXT.
    Top,
    /// What you operate: transport and practice.
    Bottom,
}

impl Region {
    /// Every region, in the order the frame lays them out.
    pub const ALL: [Self; 5] = [Self::Left, Self::Top, Self::Main, Self::Bottom, Self::Right];

    /// The regions that can be opened and closed (all but [`Region::Main`]).
    pub const EDGES: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];

    /// A sidebar has a width; a drawer takes the height of its content.
    #[must_use]
    pub const fn is_sidebar(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    /// The size a region opens at the first time, in logical px: a width for
    /// sidebars, a cap on the height for drawers.
    #[must_use]
    pub const fn default_size(self) -> u32 {
        match self {
            Self::Left => 280,
            Self::Right => 320,
            Self::Main => 0,
            Self::Top => 144,
            Self::Bottom => 340,
        }
    }

    /// The smallest and largest size the region may be set to (spec §1).
    #[must_use]
    pub const fn limits(self) -> (u32, u32) {
        match self {
            Self::Left => (220, 420),
            Self::Right => (260, 480),
            Self::Main => (0, u32::MAX),
            Self::Top => (48, 144),
            Self::Bottom => (64, 480),
        }
    }

    /// The short name used in persisted keys and debug output.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Main => "main",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// One edge region's saved state.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct RegionLayout {
    /// Shown (pinned) or collapsed to its rail / handle.
    pub open: bool,
    /// Width for a sidebar, height cap for a drawer, in logical px.
    pub size: u32,
    /// The panel showing when the region holds more than one, by id.
    /// Left out when unset: the prefs format writes `None` as a bare `@`
    /// it then cannot read back, which lost every saved layout.
    #[facet(default, skip_serializing_if = Option::is_none)]
    pub active: Option<String>,
}

impl RegionLayout {
    /// The layout a region starts with: open, at its default size.
    #[must_use]
    pub const fn initial(region: Region) -> Self {
        Self {
            open: true,
            size: region.default_size(),
            active: None,
        }
    }
}

/// A panel a person moved out of the region it lives in by default
/// (comments to the bottom, tasks to the left).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Placement {
    pub panel: String,
    pub region: Region,
}

/// The whole shell's layout, one per scope (an app, or a workspace inside it).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ShellLayout {
    pub left: RegionLayout,
    pub right: RegionLayout,
    pub top: RegionLayout,
    pub bottom: RegionLayout,
    /// The panels moved out of their home region. Missing from a layout
    /// saved before panels could move: every panel at home.
    #[facet(default)]
    pub moved: Vec<Placement>,
}

impl Default for ShellLayout {
    fn default() -> Self {
        Self {
            left: RegionLayout::initial(Region::Left),
            right: RegionLayout::initial(Region::Right),
            top: RegionLayout::initial(Region::Top),
            bottom: RegionLayout::initial(Region::Bottom),
            moved: Vec::new(),
        }
    }
}

impl ShellLayout {
    /// The saved state of an edge region; `None` for [`Region::Main`].
    #[must_use]
    pub const fn region(&self, region: Region) -> Option<&RegionLayout> {
        match region {
            Region::Left => Some(&self.left),
            Region::Right => Some(&self.right),
            Region::Top => Some(&self.top),
            Region::Bottom => Some(&self.bottom),
            Region::Main => None,
        }
    }

    const fn region_mut(&mut self, region: Region) -> Option<&mut RegionLayout> {
        match region {
            Region::Left => Some(&mut self.left),
            Region::Right => Some(&mut self.right),
            Region::Top => Some(&mut self.top),
            Region::Bottom => Some(&mut self.bottom),
            Region::Main => None,
        }
    }

    /// Whether a region is showing. Main always is.
    #[must_use]
    pub fn is_open(&self, region: Region) -> bool {
        self.region(region).is_none_or(|r| r.open)
    }

    /// Open or close a region. Returns whether anything changed, so the
    /// caller only persists (and redraws) on a real change.
    pub fn set_open(&mut self, region: Region, open: bool) -> bool {
        match self.region_mut(region) {
            Some(r) if r.open != open => {
                r.open = open;
                true
            }
            _ => false,
        }
    }

    /// Flip a region open/closed. Main cannot be closed; returns whether
    /// anything changed.
    pub fn toggle(&mut self, region: Region) -> bool {
        let open = self.is_open(region);
        self.set_open(region, !open)
    }

    /// The region's size (see [`RegionLayout::size`]); 0 for Main.
    #[must_use]
    pub fn size(&self, region: Region) -> u32 {
        self.region(region).map_or(0, |r| r.size)
    }

    /// Set a region's size, clamped to [`Region::limits`]. Returns whether
    /// anything changed.
    pub fn resize(&mut self, region: Region, size: u32) -> bool {
        let (lo, hi) = region.limits();
        let size = size.clamp(lo, hi);
        match self.region_mut(region) {
            Some(r) if r.size != size => {
                r.size = size;
                true
            }
            _ => false,
        }
    }

    /// The id of the panel a region shows, if one was picked.
    #[must_use]
    pub fn active(&self, region: Region) -> Option<&str> {
        self.region(region).and_then(|r| r.active.as_deref())
    }

    /// Pick the panel a region shows. Returns whether anything changed.
    pub fn set_active(&mut self, region: Region, id: &str) -> bool {
        match self.region_mut(region) {
            Some(r) if r.active.as_deref() != Some(id) => {
                r.active = Some(id.to_string());
                true
            }
            _ => false,
        }
    }

    /// Where a panel is: where it was moved to, else `home` (the region it
    /// was registered in).
    #[must_use]
    pub fn placed(&self, panel: &str, home: Region) -> Region {
        self.moved
            .iter()
            .find(|m| m.panel == panel)
            .map_or(home, |m| m.region)
    }

    /// Move a panel to `to` (its `home` forgets the move), and show it
    /// there, opened. Main holds the page, not panels: a move there is
    /// refused. Returns whether anything changed.
    pub fn move_panel(&mut self, panel: &str, home: Region, to: Region) -> bool {
        if to == Region::Main || self.placed(panel, home) == to {
            return false;
        }
        self.moved.retain(|m| m.panel != panel);
        if to != home {
            self.moved.push(Placement {
                panel: panel.to_string(),
                region: to,
            });
        }
        self.set_active(to, panel);
        self.set_open(to, true);
        true
    }

    /// Bring a stored layout back inside the limits this build enforces, so
    /// a hand-edited or older preference can't make a region unusable.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        for region in Region::EDGES {
            let (lo, hi) = region.limits();
            if let Some(r) = self.region_mut(region) {
                r.size = r.size.clamp(lo, hi);
            }
        }
        self
    }
}

/// The preference key a scope's layout is stored under.
///
/// Versioned in the key as well as in the stored value (`utils::prefs`
/// stamps a version): a future layout shape gets a new key instead of
/// half-reading this one (spec §2.3, migration).
#[must_use]
pub fn pref_key(scope: &str) -> String {
    format!("shell.layout.v1.{scope}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_with_a_moved_panel_reads_back_as_it_was_saved() {
        let mut layout = ShellLayout::default();
        layout.move_panel("session.comments", Region::Right, Region::Bottom);
        let text = facet_styx::to_string(&layout).unwrap();
        let back: Result<ShellLayout, _> = facet_styx::from_str(&text);
        assert_eq!(back.as_ref().ok(), Some(&layout), "{text}\n{back:?}");
    }

    #[test]
    fn a_moved_panel_lives_where_it_was_moved_and_moving_home_forgets_it() {
        let mut layout = ShellLayout::default();
        assert_eq!(layout.placed("talk", Region::Right), Region::Right);
        layout.set_open(Region::Bottom, false);
        assert!(layout.move_panel("talk", Region::Right, Region::Bottom));
        assert_eq!(layout.placed("talk", Region::Right), Region::Bottom);
        assert!(
            layout.is_open(Region::Bottom),
            "a move opens where it lands"
        );
        assert_eq!(layout.active(Region::Bottom), Some("talk"));
        assert!(
            !layout.move_panel("talk", Region::Right, Region::Bottom),
            "already there"
        );
        assert!(
            !layout.move_panel("talk", Region::Right, Region::Main),
            "main holds the page"
        );
        assert!(layout.move_panel("talk", Region::Right, Region::Right));
        assert!(layout.moved.is_empty(), "home again: nothing to remember");
    }
}

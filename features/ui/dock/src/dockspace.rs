//! A window's docking arrangement: the centre, four edge docks and any
//! floating windows, each a tree of splits ending in stacks of panels.
//!
//! The model behind REAPER-style docking (docs/design/docking.md). Unlike
//! [`DockLayout`](crate::DockLayout), whose tabs hold a panel *kind*, a
//! stack here holds panel *instances* ([`Panel`]): two arrangement views at
//! different zooms, or one MIDI and one audio editor, are two instances of
//! one kind, each with its own saved view state.
//!
//! ```text
//!            +--------------------- top ----------------------+
//!            | left |            centre             |  right  |
//!            +-------------------- bottom --------------------+
//!   floats:  [ window: its own tree ] [ window: its own tree ]
//! ```
//!
//! Every area is a [`Node`] tree: a [`Node::Split`] in two, sized by a
//! ratio or by a fixed number of pixels on one side (a toolbar against the
//! arrangement), or a [`Stack`] of panels shown one at a time as tabs.
//! Pure data: no Dioxus, no I/O; serialized with facet.

use facet::Facet;
use uuid::Uuid;

macro_rules! dock_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
        pub struct $name(Uuid);

        impl $name {
            /// A new random id.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// The id a fixed UUID names (built-in layouts, tests).
            #[must_use]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

dock_id!(
    /// One panel instance: an arrangement view, a mixer, a toolbar.
    InstanceId
);
dock_id!(
    /// One stack of tabbed panels.
    StackId
);
dock_id!(
    /// One split, so a divider drag can name what it resizes.
    SplitId
);
dock_id!(
    /// One floating window.
    FloatId
);
dock_id!(
    /// Panels that move together: the track control panel and the
    /// arrangement it heads share their vertical scroll and track set.
    LinkId
);

/// What a panel is: `"arrangement"`, `"mixer"`, `"toolbar"`, …
///
/// Open-ended, so a feature registers its own kinds without touching this
/// crate. A layout naming a kind the running app does not know keeps it
/// (shown as a placeholder) rather than losing the user's arrangement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
pub struct PanelKind(pub String);

impl PanelKind {
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self(kind.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for PanelKind {
    fn from(kind: &str) -> Self {
        Self::new(kind)
    }
}

/// One panel instance and what it remembers.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Panel {
    pub id: InstanceId,
    pub kind: PanelKind,
    /// The tab's label when the user named it ("Drums edit"); else the
    /// kind's own title.
    pub title: Option<String>,
    /// The group it scrolls and selects with, if any.
    pub link: Option<LinkId>,
    /// The panel's own view state (zoom, scroll, which item an editor
    /// follows), opaque here: the panel writes it, a screenset keeps it.
    pub state: String,
}

impl Panel {
    #[must_use]
    pub fn new(kind: impl Into<PanelKind>) -> Self {
        Self {
            id: InstanceId::new(),
            kind: kind.into(),
            title: None,
            link: None,
            state: String::new(),
        }
    }

    #[must_use]
    pub fn titled(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub const fn linked(mut self, link: LinkId) -> Self {
        self.link = Some(link);
        self
    }
}

/// Which way a split divides its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum Axis {
    /// Side by side: first on the left.
    Row,
    /// One above the other: first on top.
    Column,
}

/// How a split shares its length between its two halves.
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
#[repr(u8)]
pub enum Share {
    /// The first half's fraction of the length, 0–1.
    Ratio(f32),
    /// The first half is this many logical px; the second takes the rest.
    First(f32),
    /// The second half is this many logical px; the first takes the rest.
    Second(f32),
}

impl Share {
    /// The smallest fraction either half keeps.
    pub const MIN_RATIO: f32 = 0.05;

    /// The first half's length out of `length`, each half kept at least
    /// `min` long where the length allows.
    #[must_use]
    pub fn first_of(self, length: f32, min: f32) -> f32 {
        let first = match self {
            Self::Ratio(r) => length * r,
            Self::First(px) => px,
            Self::Second(px) => length - px,
        };
        let min = min.min(length / 2.0);
        first.clamp(min, (length - min).max(min))
    }

    /// This share, kept within what can be shown.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let px = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        match self {
            Self::Ratio(r) if r.is_finite() => {
                Self::Ratio(r.clamp(Self::MIN_RATIO, 1.0 - Self::MIN_RATIO))
            }
            Self::Ratio(_) => Self::Ratio(0.5),
            Self::First(v) => Self::First(px(v)),
            Self::Second(v) => Self::Second(px(v)),
        }
    }
}

/// How a stack draws its header.
///
/// Bare by default: panels are moved by holding the arrange key (or
/// turning Arrange on) and dragging the panel itself, so most need no
/// header. Tabs are a per-stack choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(u8)]
pub enum Chrome {
    /// A tab strip over the panel (one tab still shows its title).
    Tabs,
    /// No header: the panel alone.
    #[default]
    Bare,
}

/// Panels sharing one place, shown one at a time.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Stack {
    pub id: StackId,
    pub tabs: Vec<InstanceId>,
    pub active: usize,
    pub chrome: Chrome,
}

impl Stack {
    #[must_use]
    pub fn of(tabs: Vec<InstanceId>) -> Self {
        Self {
            id: StackId::new(),
            tabs,
            active: 0,
            chrome: Chrome::default(),
        }
    }

    /// This stack drawn with `chrome`.
    #[must_use]
    pub const fn with_chrome(mut self, chrome: Chrome) -> Self {
        self.chrome = chrome;
        self
    }

    /// The panel on show.
    #[must_use]
    pub fn active_tab(&self) -> Option<InstanceId> {
        self.tabs
            .get(self.active)
            .or_else(|| self.tabs.last())
            .copied()
    }
}

/// A tree of splits ending in stacks.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(C)]
pub enum Node {
    Split {
        id: SplitId,
        axis: Axis,
        share: Share,
        first: Box<Node>,
        second: Box<Node>,
    },
    Stack(Stack),
}

impl Node {
    /// A stack of one panel.
    #[must_use]
    pub fn one(id: InstanceId) -> Self {
        Self::Stack(Stack::of(vec![id]))
    }

    /// `first` and `second` side by side.
    #[must_use]
    pub fn row(first: Self, second: Self, share: Share) -> Self {
        Self::split(Axis::Row, first, second, share)
    }

    /// `first` above `second`.
    #[must_use]
    pub fn column(first: Self, second: Self, share: Share) -> Self {
        Self::split(Axis::Column, first, second, share)
    }

    #[must_use]
    pub fn split(axis: Axis, first: Self, second: Self, share: Share) -> Self {
        Self::Split {
            id: SplitId::new(),
            axis,
            share: share.sanitized(),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// This node with its stacks drawn bare (no tab strip).
    #[must_use]
    pub fn bare(mut self) -> Self {
        self.for_each_stack_mut(&mut |s| s.chrome = Chrome::Bare);
        self
    }

    /// Every stack, first to last.
    pub fn stacks(&self) -> Vec<&Stack> {
        let mut out = Vec::new();
        self.collect_stacks(&mut out);
        out
    }

    fn collect_stacks<'a>(&'a self, out: &mut Vec<&'a Stack>) {
        match self {
            Self::Stack(s) => out.push(s),
            Self::Split { first, second, .. } => {
                first.collect_stacks(out);
                second.collect_stacks(out);
            }
        }
    }

    fn for_each_stack_mut(&mut self, f: &mut impl FnMut(&mut Stack)) {
        match self {
            Self::Stack(s) => f(s),
            Self::Split { first, second, .. } => {
                first.for_each_stack_mut(f);
                second.for_each_stack_mut(f);
            }
        }
    }

    fn stack_mut(&mut self, id: StackId) -> Option<&mut Stack> {
        match self {
            Self::Stack(s) => (s.id == id).then_some(s),
            Self::Split { first, second, .. } => {
                first.stack_mut(id).or_else(|| second.stack_mut(id))
            }
        }
    }

    fn share_of(&self, id: SplitId) -> Option<Share> {
        match self {
            Self::Stack(_) => None,
            Self::Split {
                id: own,
                share,
                first,
                second,
                ..
            } => {
                if *own == id {
                    Some(*share)
                } else {
                    first.share_of(id).or_else(|| second.share_of(id))
                }
            }
        }
    }

    fn share_mut(&mut self, id: SplitId) -> Option<&mut Share> {
        match self {
            Self::Stack(_) => None,
            Self::Split {
                id: own,
                share,
                first,
                second,
                ..
            } => {
                if *own == id {
                    Some(share)
                } else {
                    first.share_mut(id).or_else(|| second.share_mut(id))
                }
            }
        }
    }

    /// Replaces the stack `id` with what `f` makes of it. True once found.
    fn replace_stack(&mut self, id: StackId, f: &mut Option<impl FnOnce(Stack) -> Self>) -> bool {
        match self {
            Self::Stack(s) if s.id == id => {
                let Some(f) = f.take() else { return false };
                let stack = std::mem::replace(s, Stack::of(Vec::new()));
                *self = f(stack);
                true
            }
            Self::Stack(_) => false,
            Self::Split { first, second, .. } => {
                first.replace_stack(id, f) || second.replace_stack(id, f)
            }
        }
    }

    /// This tree with `id` taken out of every stack, empty stacks gone and
    /// a split left with one half replaced by that half. `None` when
    /// nothing is left.
    fn without(self, id: InstanceId) -> Option<Self> {
        self.retain(&mut |tab| tab != id)
    }

    /// This tree keeping only the tabs `keep` accepts, pruned as
    /// [`Node::without`] prunes.
    fn retain(self, keep: &mut impl FnMut(InstanceId) -> bool) -> Option<Self> {
        match self {
            Self::Stack(mut s) => {
                let shown = s.active_tab();
                s.tabs.retain(|t| keep(*t));
                if s.tabs.is_empty() {
                    return None;
                }
                s.active = shown
                    .and_then(|t| s.tabs.iter().position(|x| *x == t))
                    .unwrap_or_else(|| s.active.min(s.tabs.len() - 1));
                Some(Self::Stack(s))
            }
            Self::Split {
                id,
                axis,
                share,
                first,
                second,
            } => match (first.retain(keep), second.retain(keep)) {
                (Some(a), Some(b)) => Some(Self::Split {
                    id,
                    axis,
                    share,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (one, None) | (None, one) => one,
            },
        }
    }
}

/// One of the four edges a dock sits along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
#[repr(u8)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];

    /// The axis a dock on this side splits the window along.
    #[must_use]
    pub const fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Row,
            Self::Top | Self::Bottom => Axis::Column,
        }
    }

    /// True where the docked thing goes first (left of, or above, the rest).
    #[must_use]
    pub const fn leads(self) -> bool {
        matches!(self, Self::Left | Self::Top)
    }
}

/// How much of an edge dock shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(u8)]
pub enum Shown {
    /// Full size, taking room from the centre.
    #[default]
    Open,
    /// A rail of its panels' icons; a tap peeks one over the centre.
    Collapsed,
    /// Gone until reopened.
    Hidden,
}

/// The dock along one edge of the window.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Edge {
    pub root: Option<Node>,
    /// Width (left, right) or height (top, bottom), logical px.
    pub size: f32,
    pub shown: Shown,
}

impl Edge {
    #[must_use]
    pub const fn empty(size: f32) -> Self {
        Self {
            root: None,
            size,
            shown: Shown::Open,
        }
    }

    /// True when the dock takes room: open, with something in it.
    #[must_use]
    pub fn takes_room(&self) -> bool {
        self.root.is_some() && self.shown == Shown::Open
    }
}

/// The four edge docks.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Edges {
    pub left: Edge,
    pub right: Edge,
    pub top: Edge,
    pub bottom: Edge,
    /// Which docks own the corners: the side docks run the window's full
    /// height (REAPER's default) or the top and bottom run its full width.
    pub corners: Corners,
}

impl Default for Edges {
    fn default() -> Self {
        Self {
            left: Edge::empty(280.0),
            right: Edge::empty(320.0),
            top: Edge::empty(160.0),
            bottom: Edge::empty(300.0),
            corners: Corners::Sides,
        }
    }
}

impl Edges {
    #[must_use]
    pub const fn get(&self, side: Side) -> &Edge {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
            Side::Top => &self.top,
            Side::Bottom => &self.bottom,
        }
    }

    pub fn get_mut(&mut self, side: Side) -> &mut Edge {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
            Side::Top => &mut self.top,
            Side::Bottom => &mut self.bottom,
        }
    }
}

/// Which pair of edge docks runs into the corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(u8)]
pub enum Corners {
    /// Left and right run full height; top and bottom sit between them.
    #[default]
    Sides,
    /// Top and bottom run full width; left and right sit between them.
    Ends,
}

/// A window's place and size, logical px.
#[derive(Debug, Clone, Copy, PartialEq, Default, Facet)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

/// A floating window with its own tree of splits.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Float {
    pub id: FloatId,
    pub root: Node,
    pub bounds: Rect,
    pub on_top: bool,
}

/// Where a panel goes.
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
#[repr(u8)]
pub enum Place {
    /// Into the centre: a tab of its first stack, or the centre itself
    /// when it is empty.
    Center,
    /// Into the dock on `side`: a tab of its first stack, or the dock's
    /// first panel.
    Edge(Side),
    /// Into a new floating window at `bounds`.
    Float(Rect),
    /// Beside or onto a stack wherever it is.
    Stack(StackId, Zone),
}

/// Where on a stack a dropped panel lands.
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
#[repr(u8)]
pub enum Zone {
    /// As another tab.
    Tab,
    /// In a new stack split off on `side`, sized by `share` (of the new
    /// pair, first half first).
    Split(Side, Share),
}

/// Which area a stack lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AreaRef {
    Center,
    Edge(Side),
    Float(FloatId),
}

/// What a dockspace operation could not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockError {
    /// No panel has that id.
    NoPanel(InstanceId),
    /// No stack has that id.
    NoStack(StackId),
    /// No split has that id.
    NoSplit(SplitId),
    /// No floating window has that id.
    NoFloat(FloatId),
}

impl std::fmt::Display for DockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPanel(id) => write!(f, "no panel {id}"),
            Self::NoStack(id) => write!(f, "no stack {id}"),
            Self::NoSplit(id) => write!(f, "no split {id}"),
            Self::NoFloat(id) => write!(f, "no floating window {id}"),
        }
    }
}

impl std::error::Error for DockError {}

/// A window's whole docking arrangement.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Dockspace {
    pub center: Option<Node>,
    pub edges: Edges,
    pub floats: Vec<Float>,
    /// Every panel instance placed anywhere above.
    pub panels: Vec<Panel>,
    /// The panel filling the window for now (REAPER's maximise; MaxPane's
    /// solo). Every structural change ends it.
    pub maximized: Option<InstanceId>,
}

impl Default for Dockspace {
    fn default() -> Self {
        Self::empty()
    }
}

impl Dockspace {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            center: None,
            edges: Edges::default(),
            floats: Vec::new(),
            panels: Vec::new(),
            maximized: None,
        }
    }

    #[must_use]
    pub fn panel(&self, id: InstanceId) -> Option<&Panel> {
        self.panels.iter().find(|p| p.id == id)
    }

    pub fn panel_mut(&mut self, id: InstanceId) -> Option<&mut Panel> {
        self.panels.iter_mut().find(|p| p.id == id)
    }

    /// The panels of one kind, in the order they were added.
    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Panel> + 'a {
        self.panels.iter().filter(move |p| p.kind.as_str() == kind)
    }

    /// Every area's root, with which area it is.
    pub fn areas(&self) -> impl Iterator<Item = (AreaRef, &Node)> {
        let center = self.center.as_ref().map(|n| (AreaRef::Center, n));
        let edges = Side::ALL.into_iter().filter_map(|s| {
            self.edges
                .get(s)
                .root
                .as_ref()
                .map(|n| (AreaRef::Edge(s), n))
        });
        let floats = self.floats.iter().map(|f| (AreaRef::Float(f.id), &f.root));
        center.into_iter().chain(edges).chain(floats)
    }

    /// The stack showing `id`, and its area.
    #[must_use]
    pub fn locate(&self, id: InstanceId) -> Option<(AreaRef, StackId)> {
        self.areas().find_map(|(area, root)| {
            root.stacks()
                .into_iter()
                .find(|s| s.tabs.contains(&id))
                .map(|s| (area, s.id))
        })
    }

    /// The stack `id`.
    #[must_use]
    pub fn stack(&self, id: StackId) -> Option<&Stack> {
        self.areas()
            .find_map(|(_, root)| root.stacks().into_iter().find(|s| s.id == id))
    }

    /// Adds `panel` at `place`; returns its id.
    pub fn add(&mut self, panel: Panel, place: Place) -> Result<InstanceId, DockError> {
        let id = panel.id;
        if let Place::Stack(stack, _) = place {
            self.stack(stack).ok_or(DockError::NoStack(stack))?;
        }
        self.panels.push(panel);
        self.put(id, place);
        Ok(id)
    }

    /// Opens a new `kind` panel at `place`.
    pub fn open(
        &mut self,
        kind: impl Into<PanelKind>,
        place: Place,
    ) -> Result<InstanceId, DockError> {
        self.add(Panel::new(kind), place)
    }

    /// Moves `id` to `place`, out of wherever it was.
    pub fn move_panel(&mut self, id: InstanceId, place: Place) -> Result<(), DockError> {
        self.panel(id).ok_or(DockError::NoPanel(id))?;
        if let Place::Stack(stack, zone) = place {
            let target = self.stack(stack).ok_or(DockError::NoStack(stack))?;
            // Onto its own stack as a tab, or split off a stack of just
            // itself: nothing would change, and taking it out first would
            // remove the stack it is meant to land on.
            if target.tabs == [id] || (zone == Zone::Tab && target.tabs.contains(&id)) {
                return Ok(());
            }
        }
        self.take_out(id);
        if let Place::Stack(stack, _) = place {
            if self.stack(stack).is_none() {
                // Its stack went with it; the move still lands somewhere.
                self.put(id, Place::Center);
                return Ok(());
            }
        }
        self.put(id, place);
        Ok(())
    }

    /// Closes `id`: out of its stack and forgotten.
    pub fn close(&mut self, id: InstanceId) -> Result<Panel, DockError> {
        let at = self
            .panels
            .iter()
            .position(|p| p.id == id)
            .ok_or(DockError::NoPanel(id))?;
        self.take_out(id);
        Ok(self.panels.remove(at))
    }

    /// Brings `id` to the front of its stack.
    pub fn activate(&mut self, id: InstanceId) -> Result<(), DockError> {
        let (_, stack) = self.locate(id).ok_or(DockError::NoPanel(id))?;
        let s = self.stack_mut(stack).ok_or(DockError::NoStack(stack))?;
        s.active = s.tabs.iter().position(|t| *t == id).unwrap_or(0);
        if let Some(side) = self.edge_of(id) {
            let edge = self.edges.get_mut(side);
            if edge.shown == Shown::Hidden {
                edge.shown = Shown::Open;
            }
        }
        Ok(())
    }

    /// Sets how the split `id` shares its length.
    /// How split `id` is divided, wherever it is.
    #[must_use]
    pub fn share_of(&self, id: SplitId) -> Option<Share> {
        self.areas().find_map(|(_, root)| root.share_of(id))
    }

    pub fn set_share(&mut self, id: SplitId, share: Share) -> Result<(), DockError> {
        let slot = self
            .roots_mut()
            .find_map(|root| root.share_mut(id))
            .ok_or(DockError::NoSplit(id))?;
        *slot = share.sanitized();
        Ok(())
    }

    /// Sets the edge dock's width or height.
    pub fn set_edge_size(&mut self, side: Side, px: f32) {
        if px.is_finite() {
            self.edges.get_mut(side).size = px.max(0.0);
        }
    }

    /// Shows or hides the stack's tab strip.
    pub fn set_chrome(&mut self, stack: StackId, chrome: Chrome) -> Result<(), DockError> {
        self.stack_mut(stack)
            .ok_or(DockError::NoStack(stack))?
            .chrome = chrome;
        Ok(())
    }

    /// Opens, collapses or hides the edge dock.
    pub fn set_shown(&mut self, side: Side, shown: Shown) {
        self.edges.get_mut(side).shown = shown;
    }

    /// True when panel `id` is on screen: not in a hidden edge dock.
    #[must_use]
    pub fn on_screen(&self, id: InstanceId) -> bool {
        self.edge_of(id)
            .is_none_or(|side| self.edges.get(side).shown != Shown::Hidden)
    }

    /// Maximises `id`, or ends it when it is already.
    pub fn toggle_maximized(&mut self, id: InstanceId) -> Result<(), DockError> {
        self.panel(id).ok_or(DockError::NoPanel(id))?;
        self.maximized = (self.maximized != Some(id)).then_some(id);
        Ok(())
    }

    /// Moves the floating window.
    pub fn set_float_bounds(&mut self, id: FloatId, bounds: Rect) -> Result<(), DockError> {
        let float = self
            .floats
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or(DockError::NoFloat(id))?;
        float.bounds = bounds;
        Ok(())
    }

    /// Puts `id` beside `host` on `side`, `px` thick, linked to it: the
    /// track control panel against the arrangement, a toolbar against the
    /// arrangement. They share a [`LinkId`] (the host's, or a new one).
    pub fn attach(
        &mut self,
        id: InstanceId,
        host: InstanceId,
        side: Side,
        px: f32,
    ) -> Result<(), DockError> {
        let (_, stack) = self.locate(host).ok_or(DockError::NoPanel(host))?;
        let link = self.panel(host).and_then(|p| p.link).unwrap_or_default();
        let share = if side.leads() {
            Share::First(px)
        } else {
            Share::Second(px)
        };
        self.move_panel(id, Place::Stack(stack, Zone::Split(side, share)))?;
        for panel in [host, id] {
            if let Some(p) = self.panel_mut(panel) {
                p.link = Some(link);
            }
        }
        Ok(())
    }

    /// Makes the layout whole again after loading one from disk: every
    /// stack's tabs name a panel, no panel is in two places, no panel is
    /// left placed nowhere, every share is in range. Returns what it fixed;
    /// empty when the layout was whole.
    pub fn repair(&mut self) -> Vec<Repair> {
        let mut fixed = Vec::new();
        let known: Vec<InstanceId> = self.panels.iter().map(|p| p.id).collect();
        let mut seen = Vec::new();
        let mut keep = |tab: InstanceId| {
            if !known.contains(&tab) {
                fixed.push(Repair::DanglingTab(tab));
                false
            } else if seen.contains(&tab) {
                fixed.push(Repair::Duplicate(tab));
                false
            } else {
                seen.push(tab);
                true
            }
        };
        self.center = self.center.take().and_then(|n| n.retain(&mut keep));
        for side in Side::ALL {
            let edge = self.edges.get_mut(side);
            edge.root = edge.root.take().and_then(|n| n.retain(&mut keep));
        }
        self.floats = std::mem::take(&mut self.floats)
            .into_iter()
            .filter_map(|mut f| {
                f.root = f.root.retain(&mut keep)?;
                Some(f)
            })
            .collect();
        let orphans: Vec<InstanceId> = known
            .iter()
            .copied()
            .filter(|id| !seen.contains(id))
            .collect();
        self.panels.retain(|p| !orphans.contains(&p.id));
        fixed.extend(orphans.into_iter().map(Repair::Orphan));
        for root in self.roots_mut() {
            sanitize_shares(root);
        }
        if self.maximized.is_some_and(|id| self.panel(id).is_none()) {
            self.maximized = None;
        }
        fixed
    }

    fn put(&mut self, id: InstanceId, place: Place) {
        self.maximized = None;
        match place {
            Place::Center => push_tab(&mut self.center, id),
            Place::Edge(side) => {
                let edge = self.edges.get_mut(side);
                push_tab(&mut edge.root, id);
                if edge.shown == Shown::Hidden {
                    edge.shown = Shown::Open;
                }
            }
            Place::Float(bounds) => self.floats.push(Float {
                id: FloatId::new(),
                root: Node::one(id),
                bounds,
                on_top: false,
            }),
            Place::Stack(stack, Zone::Tab) => {
                if let Some(s) = self.stack_mut(stack) {
                    s.tabs.push(id);
                    s.active = s.tabs.len() - 1;
                }
            }
            Place::Stack(stack, Zone::Split(side, share)) => {
                let mut f = Some(move |old: Stack| {
                    let new = Node::one(id);
                    let old = Node::Stack(old);
                    let (first, second) = if side.leads() { (new, old) } else { (old, new) };
                    Node::split(side.axis(), first, second, share)
                });
                for root in self.roots_mut() {
                    if root.replace_stack(stack, &mut f) {
                        break;
                    }
                }
            }
        }
    }

    /// Takes `id` out of whatever stack shows it, pruning what empties.
    fn take_out(&mut self, id: InstanceId) {
        self.maximized = None;
        self.center = self.center.take().and_then(|n| n.without(id));
        for side in Side::ALL {
            let edge = self.edges.get_mut(side);
            edge.root = edge.root.take().and_then(|n| n.without(id));
        }
        self.floats = std::mem::take(&mut self.floats)
            .into_iter()
            .filter_map(|mut f| {
                f.root = f.root.without(id)?;
                Some(f)
            })
            .collect();
    }

    /// The edge dock `id` is in, if it is in one.
    #[must_use]
    pub fn edge_of(&self, id: InstanceId) -> Option<Side> {
        match self.locate(id)? {
            (AreaRef::Edge(side), _) => Some(side),
            _ => None,
        }
    }

    fn stack_mut(&mut self, id: StackId) -> Option<&mut Stack> {
        self.roots_mut().find_map(|root| root.stack_mut(id))
    }

    fn roots_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        let Self {
            center,
            edges,
            floats,
            ..
        } = self;
        let Edges {
            left,
            right,
            top,
            bottom,
            ..
        } = edges;
        center
            .iter_mut()
            .chain(
                [left, right, top, bottom]
                    .into_iter()
                    .filter_map(|e| e.root.as_mut()),
            )
            .chain(floats.iter_mut().map(|f| &mut f.root))
    }
}

/// One thing [`Dockspace::repair`] fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repair {
    /// A tab named a panel that does not exist; the tab went.
    DanglingTab(InstanceId),
    /// A panel was in two stacks; the later one lost it.
    Duplicate(InstanceId),
    /// A panel was placed nowhere; it went.
    Orphan(InstanceId),
}

fn push_tab(root: &mut Option<Node>, id: InstanceId) {
    match root {
        None => *root = Some(Node::one(id)),
        Some(node) => {
            let mut first = None;
            node.for_each_stack_mut(&mut |s| {
                if first.is_none() {
                    first = Some(s.id);
                }
            });
            if let Some(stack) = first.and_then(|id| node.stack_mut(id)) {
                stack.tabs.push(id);
                stack.active = stack.tabs.len() - 1;
            }
        }
    }
}

fn sanitize_shares(node: &mut Node) {
    if let Node::Split {
        share,
        first,
        second,
        ..
    } = node
    {
        *share = share.sanitized();
        sanitize_shares(first);
        sanitize_shares(second);
    }
}

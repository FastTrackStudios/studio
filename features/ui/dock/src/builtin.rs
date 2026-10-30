//! Studio's panel kinds and the screensets it ships with.
//!
//! Signal and Session are not two apps: they are two screensets of these
//! panels (docs/design/docking.md). Each set has a layout for every
//! [`FormClass`], so a phone held either way and an iPad held either way
//! each get a layout made for them, not a desk layout squeezed.
//!
//! Pure data: the kinds are strings the app's catalog draws.

use uuid::Uuid;

use crate::dockspace::{Chrome, Dockspace, Node, Panel, Share, Shown, Side};
use crate::screenset::{FormClass, Screenset, ScreensetId, Screensets};

/// The panel kinds, as their [`PanelKind`](crate::dockspace::PanelKind)
/// strings.
pub mod kind {
    pub const ARRANGEMENT: &str = "arrangement";
    pub const TCP: &str = "tcp";
    pub const MIXER: &str = "mixer";
    pub const CHART: &str = "chart";
    pub const CHART_EDITOR: &str = "chart-editor";
    pub const LYRICS: &str = "lyrics";
    pub const LYRIC_EDITOR: &str = "lyric-editor";
    pub const FX_ROW: &str = "fx-row";
    pub const EDITOR: &str = "editor";
    pub const TRANSPORT: &str = "transport";
    pub const TOOLBAR: &str = "toolbar";
    pub const BROWSER: &str = "browser";
    pub const SETLIST: &str = "setlist";
    pub const RIG: &str = "rig";
    pub const INSPECTOR: &str = "inspector";
    /// The song's progress, its sections as a bar (Session's
    /// `PinnedProgress`, Signal's NOW / NEXT strip).
    pub const PROGRESS: &str = "progress";
}

/// The built-in sets' ids, fixed so the active one survives a restart.
pub mod id {
    use super::{ScreensetId, Uuid};

    pub const PERFORMANCE: ScreensetId = ScreensetId::from_uuid(Uuid::from_u128(0x9e4f));
    pub const OVERVIEW: ScreensetId = ScreensetId::from_uuid(Uuid::from_u128(0x0e4e));
    pub const SESSION: ScreensetId = ScreensetId::from_uuid(Uuid::from_u128(0x5e55_1011));
    pub const SIGNAL: ScreensetId = ScreensetId::from_uuid(Uuid::from_u128(0x5197_a1));
    pub const MIX: ScreensetId = ScreensetId::from_uuid(Uuid::from_u128(0x0313));
    pub const EDIT: ScreensetId = ScreensetId::from_uuid(Uuid::from_u128(0xed17));
    pub const CHARTS: ScreensetId = ScreensetId::from_uuid(Uuid::from_u128(0xc4a7));
}

/// A toolbar's height along the top, px.
const TOOLBAR_H: f32 = 40.0;
/// The transport band's height, px (spec §6.4: 64 under a finger).
const TRANSPORT_H: f32 = 56.0;
const TRANSPORT_TOUCH_H: f32 = 64.0;

/// Every built-in set, in the order of their number keys: Session's
/// views first (as its view switch had them), then Charts and Signal.
/// Session is active.
#[must_use]
pub fn screensets() -> Screensets {
    let mut sets = Screensets::new(vec![
        performance(),
        overview(),
        session(),
        mix(),
        edit(),
        charts(),
        signal(),
    ]);
    sets.active = Some(id::SESSION);
    sets
}

/// Builds one layout: panels added as their nodes are made.
struct Lay(Dockspace);

impl Lay {
    fn new() -> Self {
        Self(Dockspace::empty())
    }

    /// A stack of one new `kind` panel.
    fn one(&mut self, kind: &str) -> Node {
        let panel = Panel::new(kind);
        let node = Node::one(panel.id);
        self.0.panels.push(panel);
        node
    }

    /// A stack of new panels as tabs, the first on show. Its tab strip
    /// shows: with more than one panel in it, the strip is how the others
    /// are found.
    fn tabs(&mut self, kinds: &[&str]) -> Node {
        let ids = kinds
            .iter()
            .map(|k| {
                let p = Panel::new(*k);
                let id = p.id;
                self.0.panels.push(p);
                id
            })
            .collect();
        Node::Stack(crate::dockspace::Stack::of(ids).with_chrome(Chrome::Tabs))
    }

    /// A panel of its own, header-less like every single panel: kept as
    /// its own name where the layout means a toolbar or a band.
    fn bare(&mut self, kind: &str) -> Node {
        self.one(kind)
    }

    /// A track view: the arrangement, its track headers drawn in its own
    /// canvas so they never tear from the lanes ("rendered together when
    /// in the same spot"). `_tcp` is the headers' width, for when the
    /// arrangement takes it as view state.
    fn tracks(&mut self, _tcp: f32) -> Node {
        self.one(kind::ARRANGEMENT)
    }

    /// `top` over the transport band.
    fn over_transport(&mut self, top: Node, touch: bool) -> Node {
        let h = if touch {
            TRANSPORT_TOUCH_H
        } else {
            TRANSPORT_H
        };
        let transport = self.bare(kind::TRANSPORT);
        Node::column(top, transport, Share::Second(h))
    }

    fn center(mut self, node: Node) -> Self {
        self.0.center = Some(node);
        self
    }

    fn edge(mut self, side: Side, size: f32, node: Node) -> Self {
        let edge = self.0.edges.get_mut(side);
        edge.root = Some(node);
        edge.size = size;
        self
    }

    fn collapsed(mut self, side: Side) -> Self {
        self.0.edges.get_mut(side).shown = Shown::Collapsed;
        self
    }

    fn done(self) -> Dockspace {
        self.0
    }
}

fn set(id: ScreensetId, name: &str, slot: u8, variants: [(FormClass, Dockspace); 5]) -> Screenset {
    let mut set = Screenset::new(name).in_slot(slot);
    set.id = id;
    set.builtin = true;
    for (class, mut space) in variants {
        if !class.is_phone() {
            with_toolbar(&mut space);
        }
        set.set_variant(class, space);
    }
    set
}

/// `space` with a toolbar along its top, where it has none: the screensets
/// stay one click away in every desk and tablet layout. (A phone reaches
/// them from its dock bar's menu.)
fn with_toolbar(space: &mut Dockspace) {
    if space.of_kind(kind::TOOLBAR).next().is_some() {
        return;
    }
    let toolbar = Panel::new(kind::TOOLBAR);
    let bar = Node::one(toolbar.id);
    space.panels.push(toolbar);
    let top = &mut space.edges.top;
    top.root = Some(match top.root.take() {
        Some(below) => Node::column(bar, below, Share::First(TOOLBAR_H)),
        None => bar,
    });
    if top.size < TOOLBAR_H {
        top.size = TOOLBAR_H;
    }
    if top.root.as_ref().is_some_and(|r| r.stacks().len() == 1) {
        top.size = TOOLBAR_H;
    }
}

/// A progress strip `h` px over `main`.
fn under_progress(l: &mut Lay, main: Node, h: f32) -> Node {
    let progress = l.bare(kind::PROGRESS);
    Node::column(progress, main, Share::First(h))
}

/// Playing the song: the chart and the lyrics, the progress over them and
/// the transport under them (Session's Performance view).
fn performance() -> Screenset {
    use kind::*;
    use FormClass::*;
    let wide = |touch: bool, lyrics_share: f32| {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let lyrics = l.one(LYRICS);
        let main = Node::row(chart, lyrics, Share::Ratio(1.0 - lyrics_share));
        let main = under_progress(&mut l, main, 48.0);
        let main = l.over_transport(main, touch);
        l.center(main).done()
    };
    let tablet_tall = {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let lyrics = l.one(LYRICS);
        let main = Node::column(chart, lyrics, Share::Ratio(0.62));
        let main = under_progress(&mut l, main, 56.0);
        let main = l.over_transport(main, true);
        l.center(main).done()
    };
    // A phone has room for one: the chart, the lyrics a tap away.
    let phone = |progress_h: f32| {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let main = under_progress(&mut l, chart, progress_h);
        let main = l.over_transport(main, true);
        let lyrics = l.one(LYRICS);
        l.center(main).edge(Side::Right, 0.0, lyrics).done()
    };
    set(
        id::PERFORMANCE,
        "Performance",
        1,
        [
            (Desk, wide(false, 0.38)),
            (TabletLandscape, wide(true, 0.4)),
            (TabletPortrait, tablet_tall),
            (PhoneLandscape, phone(40.0)),
            (PhonePortrait, phone(48.0)),
        ],
    )
}

/// The whole song at once: the chart over its lyrics beside the
/// arrangement, the chart editor a column to their left (Session's
/// Overview).
fn overview() -> Screenset {
    use kind::*;
    use FormClass::*;
    let wide = |touch: bool, editor: bool| {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let lyrics = l.one(LYRICS);
        let words = Node::column(chart, lyrics, Share::Ratio(0.62));
        let tracks = l.tracks(if touch { 150.0 } else { 180.0 });
        let main = Node::row(words, tracks, Share::Ratio(0.36));
        let main = if editor {
            let ed = l.one(CHART_EDITOR);
            Node::row(ed, main, Share::First(380.0))
        } else {
            main
        };
        let main = under_progress(&mut l, main, 48.0);
        let main = l.over_transport(main, touch);
        l.center(main).done()
    };
    let tablet_tall = {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let lyrics = l.one(LYRICS);
        let words = Node::row(chart, lyrics, Share::Ratio(0.6));
        let tracks = l.tracks(140.0);
        let main = Node::column(words, tracks, Share::Ratio(0.5));
        let main = under_progress(&mut l, main, 56.0);
        let main = l.over_transport(main, true);
        let ed = l.one(CHART_EDITOR);
        l.center(main)
            .edge(Side::Left, 360.0, ed)
            .collapsed(Side::Left)
            .done()
    };
    let phone = |portrait: bool| {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let tracks = l.tracks(if portrait { 88.0 } else { 110.0 });
        let main = if portrait {
            Node::column(chart, tracks, Share::Ratio(0.5))
        } else {
            Node::row(chart, tracks, Share::Ratio(0.42))
        };
        let main = under_progress(&mut l, main, 40.0);
        let main = l.over_transport(main, true);
        let lyrics = l.one(LYRICS);
        let ed = l.one(CHART_EDITOR);
        l.center(main)
            .edge(Side::Right, 0.0, lyrics)
            .edge(Side::Left, 0.0, ed)
            .done()
    };
    set(
        id::OVERVIEW,
        "Overview",
        2,
        [
            (Desk, wide(false, true)),
            (TabletLandscape, wide(true, false)),
            (TabletPortrait, tablet_tall),
            (PhoneLandscape, phone(false)),
            (PhonePortrait, phone(true)),
        ],
    )
}

/// The full DAW: tracks in the middle, the mixer and editors below.
fn session() -> Screenset {
    use kind::*;
    use FormClass::*;
    let desk = {
        let mut l = Lay::new();
        let tracks = l.tracks(220.0);
        let toolbar = l.bare(TOOLBAR);
        let browser = l.tabs(&[BROWSER, SETLIST]);
        let below = l.tabs(&[MIXER, EDITOR, FX_ROW]);
        let below = l.over_transport(below, false);
        l.center(tracks)
            .edge(Side::Top, TOOLBAR_H, toolbar)
            .edge(Side::Left, 260.0, browser)
            .edge(Side::Bottom, 340.0, below)
            .done()
    };
    let tablet_wide = {
        let mut l = Lay::new();
        let tracks = l.tracks(180.0);
        let toolbar = l.bare(TOOLBAR);
        let browser = l.tabs(&[BROWSER, SETLIST]);
        let below = l.tabs(&[MIXER, EDITOR, FX_ROW]);
        let below = l.over_transport(below, true);
        l.center(tracks)
            .edge(Side::Top, TOOLBAR_H, toolbar)
            .edge(Side::Left, 260.0, browser)
            .collapsed(Side::Left)
            .edge(Side::Bottom, 320.0, below)
            .done()
    };
    let tablet_tall = {
        let mut l = Lay::new();
        let tracks = l.tracks(150.0);
        let toolbar = l.bare(TOOLBAR);
        let below = l.tabs(&[MIXER, EDITOR, FX_ROW]);
        let main = Node::column(tracks, below, Share::Ratio(0.58));
        let main = l.over_transport(main, true);
        let browser = l.tabs(&[BROWSER, SETLIST]);
        l.center(main)
            .edge(Side::Top, TOOLBAR_H, toolbar)
            .edge(Side::Left, 300.0, browser)
            .collapsed(Side::Left)
            .done()
    };
    let phone_wide = {
        let mut l = Lay::new();
        let tracks = l.tracks(120.0);
        let main = l.over_transport(tracks, true);
        let below = l.tabs(&[MIXER, EDITOR, FX_ROW]);
        let browser = l.tabs(&[BROWSER, SETLIST]);
        let chart = l.one(CHART);
        l.center(main)
            .edge(Side::Bottom, 0.0, below)
            .edge(Side::Left, 0.0, browser)
            .edge(Side::Right, 0.0, chart)
            .done()
    };
    let phone_tall = {
        let mut l = Lay::new();
        let tracks = l.tracks(96.0);
        let main = l.over_transport(tracks, true);
        let below = l.tabs(&[MIXER, EDITOR, FX_ROW]);
        let browser = l.tabs(&[BROWSER, SETLIST]);
        let chart = l.one(CHART);
        l.center(main)
            .edge(Side::Bottom, 0.0, below)
            .edge(Side::Left, 0.0, browser)
            .edge(Side::Right, 0.0, chart)
            .done()
    };
    set(
        id::SESSION,
        "Session",
        3,
        [
            (Desk, desk),
            (TabletLandscape, tablet_wide),
            (TabletPortrait, tablet_tall),
            (PhoneLandscape, phone_wide),
            (PhonePortrait, phone_tall),
        ],
    )
}

/// The live rig: the rig in the middle (it draws its own chain and
/// footswitches), the chart beside it, the transport under it.
fn signal() -> Screenset {
    use kind::*;
    use FormClass::*;
    let wide = |touch: bool, chart_w: f32, setlist: bool| {
        let mut l = Lay::new();
        let rig = l.one(RIG);
        let main = l.over_transport(rig, touch);
        let chart = l.tabs(&[CHART, LYRICS]);
        let toolbar = l.bare(TOOLBAR);
        let mut l =
            l.center(main)
                .edge(Side::Top, TOOLBAR_H, toolbar)
                .edge(Side::Right, chart_w, chart);
        if setlist {
            let list = l.one(SETLIST);
            l = l.edge(Side::Left, 260.0, list);
        }
        l.done()
    };
    let tablet_tall = {
        let mut l = Lay::new();
        let rig = l.one(RIG);
        let chart = l.tabs(&[CHART, LYRICS]);
        let main = Node::column(rig, chart, Share::Ratio(0.6));
        let main = l.over_transport(main, true);
        let list = l.one(SETLIST);
        l.center(main)
            .edge(Side::Left, 300.0, list)
            .collapsed(Side::Left)
            .done()
    };
    let phone = || {
        let mut l = Lay::new();
        let rig = l.one(RIG);
        let main = l.over_transport(rig, true);
        let chart = l.tabs(&[CHART, LYRICS]);
        let list = l.one(SETLIST);
        l.center(main)
            .edge(Side::Right, 0.0, chart)
            .edge(Side::Left, 0.0, list)
            .done()
    };
    set(
        id::SIGNAL,
        "Signal",
        7,
        [
            (Desk, wide(false, 380.0, true)),
            (TabletLandscape, wide(true, 340.0, false)),
            (TabletPortrait, tablet_tall),
            (PhoneLandscape, phone()),
            (PhonePortrait, phone()),
        ],
    )
}

/// The mixer in the middle, FX beside it.
fn mix() -> Screenset {
    use kind::*;
    use FormClass::*;
    let make = |touch: bool, fx: bool| {
        let mut l = Lay::new();
        let mixer = l.one(MIXER);
        let main = l.over_transport(mixer, touch);
        let mut l = l.center(main);
        if fx {
            let fx_row = l.tabs(&[FX_ROW, INSPECTOR]);
            l = l.edge(Side::Right, 340.0, fx_row);
        }
        l.done()
    };
    set(
        id::MIX,
        "Mix",
        4,
        [
            (Desk, make(false, true)),
            (TabletLandscape, make(true, true)),
            (TabletPortrait, make(true, false)),
            (PhoneLandscape, make(true, true)),
            (PhonePortrait, make(true, true)),
        ],
    )
}

/// Tracks over the editor: comping, quantizing, MIDI.
fn edit() -> Screenset {
    use kind::*;
    use FormClass::*;
    let make = |touch: bool, tcp: f32, ratio: f32| {
        let mut l = Lay::new();
        let tracks = l.tracks(tcp);
        let editor = l.one(EDITOR);
        let main = Node::column(tracks, editor, Share::Ratio(ratio));
        let main = l.over_transport(main, touch);
        l.center(main).done()
    };
    set(
        id::EDIT,
        "Edit",
        5,
        [
            (Desk, make(false, 220.0, 0.42)),
            (TabletLandscape, make(true, 160.0, 0.4)),
            (TabletPortrait, make(true, 140.0, 0.45)),
            (PhoneLandscape, make(true, 96.0, 0.35)),
            (PhonePortrait, make(true, 80.0, 0.4)),
        ],
    )
}

/// The chart and its editor; lyrics beside.
fn charts() -> Screenset {
    use kind::*;
    use FormClass::*;
    let wide = |touch: bool| {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let editor = l.one(CHART_EDITOR);
        let main = Node::row(chart, editor, Share::Ratio(0.55));
        let main = l.over_transport(main, touch);
        let lyrics = l.tabs(&[LYRICS, LYRIC_EDITOR]);
        l.center(main).edge(Side::Right, 320.0, lyrics).done()
    };
    let tall = |phone: bool| {
        let mut l = Lay::new();
        let chart = l.one(CHART);
        let main = if phone {
            chart
        } else {
            let editor = l.one(CHART_EDITOR);
            Node::column(chart, editor, Share::Ratio(0.6))
        };
        let main = l.over_transport(main, true);
        let lyrics = l.tabs(&[LYRICS, LYRIC_EDITOR]);
        let mut l = l.center(main).edge(Side::Right, 0.0, lyrics);
        if phone {
            let editor = l.one(CHART_EDITOR);
            l = l.edge(Side::Bottom, 0.0, editor);
        }
        l.done()
    };
    set(
        id::CHARTS,
        "Charts",
        6,
        [
            (Desk, wide(false)),
            (TabletLandscape, wide(true)),
            (TabletPortrait, tall(false)),
            (PhoneLandscape, tall(true)),
            (PhonePortrait, tall(true)),
        ],
    )
}

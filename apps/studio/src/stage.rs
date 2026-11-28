//! The session's **Stage**: three windows, the layout the launcher made
//! for a show over three displays (`extensions/launcher/src/layout.rs`,
//! `stage` and `stage_main`), ported onto app-shell's layout as it is:
//!
//! - **main**, "Studio — <set>": the song's progress bar across the top
//!   (as tall as it is), the arrangement with the mixer beside it on a
//!   wide display (the mixer 1070 points wide) or under it (0.62) when
//!   narrower;
//! - **lighting**, "Lighting — <set>": Ignition's visualizer (0.8) with its cue
//!   list to the right;
//! - **chart**: Keyflow's chart, its note-input toolbar on its pane (0.67),
//!   over the lyrics.
//!
//! A **studio project** (`# kind: studio` in its setlist file: an album
//! or a record being made, not a show) needs no lighting: its main window
//! is the progress bar over the arrangement alone, and the right display's
//! window, "Mixer — <set>", is the mixer.
//!
//! The window manager places them by title (Loft's rules on app id
//! `studio`).

use app_shell::{
    Axis, Child, Group, GroupId, Node, Pane, PaneId, Rect, Split, SplitId, Window, WindowId,
    Workspace,
};

pub const ARRANGEMENT: &str = "studio.arrangement/arrangement";
pub const MIXER: &str = "studio.mixer/mixer";
pub const PROGRESS: &str = "studio.navigator/progress";
pub const LYRICS: &str = "studio.lyrics/lyrics";
pub const CHART: &str = "keyflow/chart";
pub const SHOW: &str = "ignition/show";
pub const CUES: &str = "ignition/cues";

/// The lighting's window and the chart's.
pub const LIGHTING_WINDOW: &str = "lighting";
/// A studio project's mixer window, where a show has its lighting.
pub const MIXER_WINDOW: &str = "mixer";
pub const CHART_WINDOW: &str = "chart";
/// The launcher's own window during a session, titled "Launcher".
pub const LAUNCHER_WINDOW: &str = "launcher";
/// Its pane (the window's title; the app draws the window itself).
pub const LAUNCHER: &str = "studio.launcher";

/// The mixer's width beside the arrangement, points: its strips (the top
/// groups of a song).
const MIXER_WIDTH: f64 = 1070.0;
/// The progress bar's height with its foot, points.
const PROGRESS_HEIGHT: f64 = 50.0;
/// A window this much wider than tall puts the mixer beside the
/// arrangement (an ultrawide); narrower, under it.
pub const WIDE: f64 = 2.4;

/// A window's shape, points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub w: f64,
    pub h: f64,
}

impl Area {
    fn wide(self) -> bool {
        self.w / self.h >= WIDE
    }
}

impl Default for Area {
    /// The display the first Stage was laid out on.
    fn default() -> Self {
        Self {
            w: 5120.0,
            h: 1400.0,
        }
    }
}

/// What the stage's panes say: the set's name, the chart's state (the
/// song's session, `session:<id>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names {
    pub set: String,
    pub chart_state: String,
    /// A show or a studio project: which windows it has.
    pub kind: crate::setfile::Kind,
}

/// Builds the stage's nodes, numbering groups and splits as it goes.
struct Builder {
    next: u64,
}

impl Builder {
    fn id(&mut self) -> u64 {
        let id = self.next;
        self.next = self.next.saturating_add(1);
        id
    }

    fn group(&mut self, pane: &str) -> Node {
        let mut g = Group::new(GroupId(self.id()));
        g.tabs.push(PaneId::new(pane));
        // A view filling its place: no tab bar.
        g.chrome = app_shell::Chrome::Bare;
        Node::Group(g)
    }

    /// `first` and `second` along `axis`, `first` taking `share` of it.
    fn split(&mut self, axis: Axis, first: Node, second: Node, share: f64) -> Node {
        #[allow(clippy::cast_possible_truncation)]
        let share = share.clamp(0.02, 0.98) as f32;
        Node::Split(Split {
            id: SplitId(self.id()),
            axis,
            children: vec![
                Child { node: first, share },
                Child {
                    node: second,
                    share: 1.0 - share,
                },
            ],
        })
    }

    /// The main window's centre: the progress bar across the top, the
    /// arrangement with the mixer beside it (a wide window) or under it.
    fn main(&mut self, area: Area) -> Node {
        let (arrangement, mixer) = (self.group(ARRANGEMENT), self.group(MIXER));
        let work = if area.wide() {
            self.split(Axis::Row, arrangement, mixer, 1.0 - MIXER_WIDTH / area.w)
        } else {
            self.split(Axis::Column, arrangement, mixer, 0.62)
        };
        let progress = self.group(PROGRESS);
        self.split(Axis::Column, progress, work, PROGRESS_HEIGHT / area.h)
    }

    /// A studio project's main window: the progress bar over the
    /// arrangement alone (its mixer has a window of its own).
    fn main_alone(&mut self, area: Area) -> Node {
        let arrangement = self.group(ARRANGEMENT);
        let progress = self.group(PROGRESS);
        self.split(
            Axis::Column,
            progress,
            arrangement,
            PROGRESS_HEIGHT / area.h,
        )
    }
}

/// The stage's three windows over `area` (the main window's), its main
/// window's centre `page` instead when the session shows one of the app's
/// own pages (its pane given).
#[must_use]
pub fn stage(area: Area, names: &Names, page: Option<Pane>) -> Workspace {
    let studio = names.kind == crate::setfile::Kind::Studio;
    let mut b = Builder { next: 1 };
    let mut panes = vec![
        Pane::new(PROGRESS, PROGRESS, "Progress"),
        Pane::new(ARRANGEMENT, ARRANGEMENT, "Arrangement"),
        if studio {
            Pane::new(MIXER, MIXER, format!("Mixer — {}", names.set))
        } else {
            Pane::new(MIXER, MIXER, "Mixer")
        },
        {
            let mut chart = Pane::new(CHART, CHART, format!("Chart — {}", names.set));
            chart.state.clone_from(&names.chart_state);
            chart
        },
        Pane::new(LYRICS, LYRICS, "Lyrics"),
    ];
    if !studio {
        panes.push(Pane::new(SHOW, SHOW, format!("Lighting — {}", names.set)));
        panes.push(Pane::new(CUES, CUES, "Cues"));
    }
    let main = match &page {
        Some(p) => b.group(&p.id.0),
        None if studio => b.main_alone(area),
        None => b.main(area),
    };
    if let Some(p) = page {
        panes.push(p);
    }
    // The right display: a show's lighting, a studio project's mixer.
    let right = if studio {
        Window {
            bounds: Some(Rect::new(0.0, 0.0, 2560.0, 1440.0)),
            ..Window::new(WindowId::new(MIXER_WINDOW), b.group(MIXER))
        }
    } else {
        let (show, cues) = (b.group(SHOW), b.group(CUES));
        let lighting = b.split(Axis::Row, show, cues, 0.8);
        Window {
            bounds: Some(Rect::new(0.0, 0.0, 2560.0, 1440.0)),
            ..Window::new(WindowId::new(LIGHTING_WINDOW), lighting)
        }
    };
    let (chart, lyrics) = (b.group(CHART), b.group(LYRICS));
    let chart_window = b.split(Axis::Column, chart, lyrics, 0.67);
    let mut ws = Workspace::new();
    ws.windows = vec![
        Window::new(WindowId::main(), main),
        right,
        Window {
            bounds: Some(Rect::new(0.0, 0.0, 1440.0, 2560.0)),
            ..Window::new(WindowId::new(CHART_WINDOW), chart_window)
        },
    ];
    ws.panes = panes;
    ws.active = Some(PaneId::new(ARRANGEMENT));
    ws.next_id = b.next;
    ws
}

/// The Stage with `centre` (a pane of the Stage's: `CHART`, `LYRICS`,
/// `SHOW`) filling its main window, the window it had otherwise left out;
/// with `others` false, the main window alone (`STUDIO_SINGLE_WINDOW`).
/// Every pane is kept, so what follows the song (the chart's session)
/// still finds it. `centre` `None` is the Stage as [`stage`] makes it.
#[must_use]
pub fn with_centre(
    area: Area,
    names: &Names,
    centre: Option<&str>,
    page: Option<Pane>,
    others: bool,
) -> Workspace {
    let mut ws = stage(area, names, page);
    if let Some(pane) = centre {
        let mut b = Builder { next: ws.next_id };
        let node = b.group(pane);
        ws.next_id = b.next;
        ws.windows[0] = Window::new(WindowId::main(), node);
        // A pane is in one place: the window it was in goes.
        let id = PaneId::new(pane);
        let mut first = true;
        ws.windows.retain(|w| {
            let main = std::mem::take(&mut first);
            main || !w.center.as_ref().is_some_and(|n| holds(n, &id))
        });
        ws.active = Some(id);
    }
    if !others {
        ws.windows.truncate(1);
    }
    ws
}

/// The rig's window: Signal's screens as the iPad lays them out
/// (`docs/design/signal-extensions.md`, "The Rig layout"), each pane a
/// view of one of the rig's extensions (`studio.rig.*`, `studio.keys`).
pub const RIG_WINDOW: &str = "rig";

/// The guitar rig's panes (`studio.rig.*` views).
pub mod rig {
    pub const STATUS: &str = "studio.rig.status/status";
    pub const TUNER: &str = "studio.rig.status/tuner";
    pub const SETLIST: &str = "studio.rig.setlist/setlist";
    pub const MACROS: &str = "studio.rig.macros/macros";
    pub const GRID: &str = "studio.rig.grid/grid";
    pub const FX: &str = "studio.rig.grid/fx";
    pub const EDIT: &str = "studio.rig.grid/edit";
    pub const BROWSER: &str = "studio.rig.browser/browser";
    pub const SWITCHES: &str = "studio.rig.switches/switches";
}

/// The keys rig's panes (`studio.keys` views).
pub mod keys {
    pub const BROWSER: &str = "studio.keys/browser";
    pub const RACK: &str = "studio.keys/rack";
    pub const LAYERS: &str = "studio.keys/layers";
    pub const ROUTING: &str = "studio.keys/routing";
    pub const MIDI: &str = "studio.keys/midi";
    pub const KEYBOARD: &str = "studio.keys/keyboard";
    pub const SWITCHES: &str = "studio.keys/switches";
    pub const MACROS: &str = "studio.keys/macros";
}

/// Which rig the Rig window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigLayout {
    /// The guitar's (and the bass's: the same rig, another instrument).
    Guitar,
    Keys,
}

impl RigLayout {
    /// `guitar`, `bass` or `keys`, any case.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "guitar" | "bass" => Some(Self::Guitar),
            "keys" => Some(Self::Keys),
            _ => None,
        }
    }

    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Guitar => "guitar",
            Self::Keys => "keys",
        }
    }

    /// The pane the window is known by (its first, which titles it).
    #[must_use]
    pub const fn first(self) -> &'static str {
        match self {
            Self::Guitar => rig::STATUS,
            Self::Keys => keys::BROWSER,
        }
    }
}

impl Builder {
    /// `panes` as tabs of one group, the first showing.
    fn tabs(&mut self, panes: &[&str]) -> Node {
        let mut g = Group::new(GroupId(self.id()));
        g.tabs = panes.iter().map(|p| PaneId::new(*p)).collect();
        Node::Group(g)
    }

    /// The guitar rig: the status bar across the top; under it the
    /// setlist on the left, and the macros, the grid (its FX row, Edit and
    /// the browser as tabs) and the switches (and the tuner) down the
    /// right.
    fn guitar_rig(&mut self) -> Node {
        let status = self.group(rig::STATUS);
        let setlist = self.group(rig::SETLIST);
        let macros = self.group(rig::MACROS);
        let work = self.tabs(&[rig::GRID, rig::FX, rig::EDIT, rig::BROWSER]);
        let feet = self.tabs(&[rig::SWITCHES, rig::TUNER]);
        // 14 % macros, 48 % grid, 38 % switches of the right-hand column.
        let lower = self.split(Axis::Column, work, feet, 0.48 / 0.86);
        let right = self.split(Axis::Column, macros, lower, 0.14);
        let body = self.split(Axis::Row, setlist, right, 0.24);
        self.split(Axis::Column, status, body, 0.09)
    }

    /// The keys rig: the browser on the left; the rack (its layers,
    /// routing and MIDI as tabs), the keyboard and the switches (and the
    /// macros) down the right.
    fn keys_rig(&mut self) -> Node {
        let browser = self.group(keys::BROWSER);
        let rack = self.tabs(&[keys::RACK, keys::LAYERS, keys::ROUTING, keys::MIDI]);
        let keyboard = self.group(keys::KEYBOARD);
        let feet = self.tabs(&[keys::SWITCHES, keys::MACROS]);
        // 50 % rack, 26 % keyboard, 24 % switches.
        let lower = self.split(Axis::Column, keyboard, feet, 0.26 / 0.5);
        let right = self.split(Axis::Column, rack, lower, 0.5);
        self.split(Axis::Row, browser, right, 0.22)
    }
}

/// The Rig window showing `layout`, and its panes, its ids taken from
/// `next_id` on (the workspace's next id after it).
#[must_use]
pub fn rig_window(layout: RigLayout, set: &str, next_id: u64) -> (Window, Vec<Pane>, u64) {
    let mut b = Builder { next: next_id };
    let titled = |id: &str, title: &str| Pane::new(id, id, title);
    let window_title = if set.is_empty() {
        "Rig".to_owned()
    } else {
        format!("Rig — {set}")
    };
    let (center, panes) = match layout {
        RigLayout::Guitar => (
            b.guitar_rig(),
            vec![
                titled(rig::STATUS, &window_title),
                titled(rig::SETLIST, "Setlist"),
                titled(rig::MACROS, "Macros"),
                titled(rig::GRID, "Grid"),
                titled(rig::FX, "FX"),
                titled(rig::EDIT, "Edit"),
                titled(rig::BROWSER, "Browser"),
                titled(rig::SWITCHES, "Switches"),
                titled(rig::TUNER, "Tuner"),
            ],
        ),
        RigLayout::Keys => (
            b.keys_rig(),
            vec![
                titled(keys::BROWSER, &window_title),
                titled(keys::RACK, "Rack"),
                titled(keys::LAYERS, "Layers"),
                titled(keys::ROUTING, "Routing"),
                titled(keys::MIDI, "MIDI"),
                titled(keys::KEYBOARD, "Keyboard"),
                titled(keys::SWITCHES, "Switches"),
                titled(keys::MACROS, "Macros"),
            ],
        ),
    };
    let window = Window {
        // An iPad's screen, landscape: what the layout was drawn for.
        bounds: Some(Rect::new(120.0, 120.0, 1366.0, 1024.0)),
        ..Window::new(WindowId::new(RIG_WINDOW), center)
    };
    (window, panes, b.next)
}

/// Opens the Rig window showing `layout` in `ws`, in place of the one
/// there (a rig's window is one at a time); its first pane made active.
pub fn open_rig(ws: &mut Workspace, layout: RigLayout, set: &str) {
    let id = WindowId::new(RIG_WINDOW);
    if let Some(old) = ws.windows.iter().position(|w| w.id == id) {
        let gone = ws.windows.remove(old);
        if let Some(node) = gone.center.as_ref() {
            ws.panes.retain(|p| !holds(node, &p.id));
        }
    }
    let (window, panes, next) = rig_window(layout, set, ws.next_id);
    ws.next_id = next;
    ws.panes.extend(panes);
    ws.windows.push(window);
    ws.active = Some(PaneId::new(layout.first()));
}

/// Which rig the Rig window in `ws` shows, when it is open.
#[must_use]
pub fn rig_shown(ws: &Workspace) -> Option<RigLayout> {
    let window = ws.window(&WindowId::new(RIG_WINDOW))?;
    let node = window.center.as_ref()?;
    [RigLayout::Guitar, RigLayout::Keys]
        .into_iter()
        .find(|l| holds(node, &PaneId::new(l.first())))
}

/// `next` (a Stage laid out again) with the Rig window `was` had, so a
/// change of view or of the window's shape keeps the rig open.
pub fn carry_rig(was: &Workspace, next: &mut Workspace) {
    let id = WindowId::new(RIG_WINDOW);
    let Some(window) = was.window(&id).cloned() else {
        return;
    };
    if next.window(&id).is_some() {
        return;
    }
    if let Some(node) = window.center.as_ref() {
        next.panes
            .extend(was.panes.iter().filter(|p| holds(node, &p.id)).cloned());
    }
    // Its group and split ids stay its own: the new Stage's go on past them.
    next.next_id = next.next_id.max(was.next_id);
    next.windows.push(window);
}

/// Whether `node` has `pane` in it.
fn holds(node: &Node, pane: &PaneId) -> bool {
    match node {
        Node::Group(g) => g.tabs.contains(pane),
        Node::Split(s) => s.children.iter().any(|c| holds(&c.node, pane)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn views(node: &Node) -> Vec<String> {
        match node {
            Node::Group(g) => g.tabs.iter().map(|t| t.0.clone()).collect(),
            Node::Split(s) => s.children.iter().flat_map(|c| views(&c.node)).collect(),
        }
    }

    fn split(node: &Node) -> &Split {
        match node {
            Node::Split(s) => s,
            Node::Group(_) => panic!("a group"),
        }
    }

    #[test]
    fn a_studio_project_has_the_mixer_on_the_right_and_no_lighting() {
        let names = Names {
            set: "Crescendum".into(),
            kind: crate::setfile::Kind::Studio,
            ..Names::default()
        };
        let ws = stage(Area::default(), &names, None);
        let main = ws.windows[0].center.as_ref().unwrap();
        assert_eq!(views(main), [PROGRESS, ARRANGEMENT]);
        assert_eq!(ws.windows[1].id, WindowId::new(MIXER_WINDOW));
        assert_eq!(views(ws.windows[1].center.as_ref().unwrap()), [MIXER]);
        assert!(ws.panes.iter().all(|p| p.id.0 != SHOW && p.id.0 != CUES));
        let mixer = ws.panes.iter().find(|p| p.id.0 == MIXER).unwrap();
        assert_eq!(mixer.title, "Mixer — Crescendum");
    }

    #[test]
    fn the_stage_on_an_ultrawide_puts_the_mixer_beside_the_arrangement() {
        let ws = stage(Area::default(), &Names::default(), None);
        let main = ws.windows[0].center.as_ref().unwrap();
        let top = split(main);
        assert_eq!(top.axis, Axis::Column);
        assert_eq!(views(&top.children[0].node), [PROGRESS]);
        assert!((f64::from(top.children[0].share) - 50.0 / 1400.0).abs() < 1e-6);
        let work = split(&top.children[1].node);
        assert_eq!(work.axis, Axis::Row);
        assert_eq!(views(&top.children[1].node), [ARRANGEMENT, MIXER]);
        assert!((f64::from(work.children[1].share) - 1070.0 / 5120.0).abs() < 1e-6);
        let room = split(ws.windows[1].center.as_ref().unwrap());
        assert_eq!((room.axis, room.children[0].share), (Axis::Row, 0.8));
        let chart = ws.windows[2].center.as_ref().unwrap();
        assert_eq!(split(chart).axis, Axis::Column);
        assert_eq!(views(chart), [CHART, LYRICS]);
    }

    #[test]
    fn the_stage_on_a_16_by_9_puts_the_mixer_under_it() {
        let ws = stage(
            Area {
                w: 2560.0,
                h: 1440.0,
            },
            &Names::default(),
            None,
        );
        let top = split(ws.windows[0].center.as_ref().unwrap());
        let work = split(&top.children[1].node);
        assert_eq!(work.axis, Axis::Column);
        assert!((work.children[0].share - 0.62).abs() < 1e-6);
    }

    #[test]
    fn the_chart_fills_the_main_window() {
        let names = Names {
            set: "Set".into(),
            chart_state: "session:x".into(),
            kind: crate::setfile::Kind::default(),
        };
        // The Charts screenset: the chart alone in the main window, its own
        // window gone, the lighting's kept.
        let ws = with_centre(Area::default(), &names, Some(CHART), None, true);
        assert_eq!(views(ws.windows[0].center.as_ref().unwrap()), [CHART]);
        assert_eq!(ws.windows.len(), 2);
        assert_eq!(ws.windows[1].id, WindowId::new(LIGHTING_WINDOW));
        // Every pane is still the Stage's: the chart keeps its session.
        let chart = ws.panes.iter().find(|p| p.id.0 == CHART).unwrap();
        assert_eq!(chart.state, "session:x");
        // One window: no other.
        let ws = with_centre(Area::default(), &names, Some(CHART), None, false);
        assert_eq!(ws.windows.len(), 1);
        // No centre, one window: the Stage's own main window.
        let ws = with_centre(Area::default(), &names, None, None, false);
        assert_eq!(ws.windows.len(), 1);
        assert!(views(ws.windows[0].center.as_ref().unwrap()).contains(&ARRANGEMENT.to_owned()));
    }

    #[test]
    fn every_tab_is_a_pane_of_the_layout() {
        let ws = stage(Area::default(), &Names::default(), None);
        for w in &ws.windows {
            for v in views(w.center.as_ref().unwrap()) {
                assert!(ws.pane(&PaneId::new(&v)).is_some(), "{v}");
            }
        }
        assert_eq!(ws.windows[1].id.0, LIGHTING_WINDOW);
    }

    #[test]
    fn the_rig_window_lays_the_guitar_out_as_the_ipad_does() {
        let mut ws = stage(Area::default(), &Names::default(), None);
        let before = ws.windows.len();
        open_rig(&mut ws, RigLayout::Guitar, "CYA");
        assert_eq!(ws.windows.len(), before + 1);
        let window = ws.window(&WindowId::new(RIG_WINDOW)).unwrap();
        let shown = views(window.center.as_ref().unwrap());
        assert_eq!(
            shown,
            [
                rig::STATUS,
                rig::SETLIST,
                rig::MACROS,
                rig::GRID,
                rig::FX,
                rig::EDIT,
                rig::BROWSER,
                rig::SWITCHES,
                rig::TUNER,
            ]
        );
        // Every pane it shows is in the workspace, the first titling it.
        for id in &shown {
            assert!(ws.pane(&PaneId::new(id.as_str())).is_some(), "{id}");
        }
        assert_eq!(
            ws.pane(&PaneId::new(rig::STATUS)).unwrap().title,
            "Rig — CYA"
        );
        assert_eq!(ws.active, Some(PaneId::new(rig::STATUS)));
        // The status bar across the top, the setlist down the left.
        let top = split(window.center.as_ref().unwrap());
        assert_eq!(top.axis, Axis::Column);
        assert_eq!(views(&top.children[0].node), [rig::STATUS]);
        assert_eq!(split(&top.children[1].node).axis, Axis::Row);
        assert_eq!(rig_shown(&ws), Some(RigLayout::Guitar));
    }

    #[test]
    fn opening_the_keys_rig_replaces_the_guitars_window() {
        let mut ws = stage(Area::default(), &Names::default(), None);
        open_rig(&mut ws, RigLayout::Guitar, "");
        open_rig(&mut ws, RigLayout::Keys, "");
        let rigs = ws.windows.iter().filter(|w| w.id.0 == RIG_WINDOW).count();
        assert_eq!(rigs, 1);
        assert_eq!(rig_shown(&ws), Some(RigLayout::Keys));
        assert!(ws.pane(&PaneId::new(rig::STATUS)).is_none());
        assert_eq!(ws.pane(&PaneId::new(keys::BROWSER)).unwrap().title, "Rig");
    }

    #[test]
    fn a_stage_laid_out_again_keeps_the_rig_open() {
        let mut was = stage(Area::default(), &Names::default(), None);
        open_rig(&mut was, RigLayout::Guitar, "");
        let mut next = with_centre(Area::default(), &Names::default(), Some(CHART), None, true);
        carry_rig(&was, &mut next);
        assert_eq!(rig_shown(&next), Some(RigLayout::Guitar));
        assert!(next.pane(&PaneId::new(rig::SWITCHES)).is_some());
        assert!(next.next_id >= was.next_id);
        // Twice is still one window.
        carry_rig(&was, &mut next);
        assert_eq!(
            next.windows.iter().filter(|w| w.id.0 == RIG_WINDOW).count(),
            1
        );
    }

    #[test]
    fn rigs_are_named_as_the_launcher_names_them() {
        assert_eq!(RigLayout::parse("Guitar"), Some(RigLayout::Guitar));
        assert_eq!(RigLayout::parse("bass"), Some(RigLayout::Guitar));
        assert_eq!(RigLayout::parse(" Keys "), Some(RigLayout::Keys));
        assert_eq!(RigLayout::parse("drums"), None);
    }
}

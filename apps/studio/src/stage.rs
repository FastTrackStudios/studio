//! The session's **Stage**: three windows, the layout the launcher made
//! for a show over three displays (`extensions/launcher/src/layout.rs`,
//! `stage` and `stage_main`), ported onto app-shell's layout as it is:
//!
//! - **main**, "Studio — <set>": the song's progress bar across the top
//!   (as tall as it is), the arrangement with the mixer beside it on a
//!   wide display (the mixer 1070 points wide) or under it (0.62) when
//!   narrower;
//! - **lighting**, "Lighting — <set>": Ignition's room (0.8) with its cue
//!   list to the right;
//! - **chart**: Keyflow's chart, its note-input toolbar on its pane (0.67),
//!   over the lyrics.
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
}

/// The stage's three windows over `area` (the main window's), its main
/// window's centre `page` instead when the session shows one of the app's
/// own pages (its pane given).
#[must_use]
pub fn stage(area: Area, names: &Names, page: Option<Pane>) -> Workspace {
    let mut b = Builder { next: 1 };
    let mut panes = vec![
        Pane::new(PROGRESS, PROGRESS, "Progress"),
        Pane::new(ARRANGEMENT, ARRANGEMENT, "Arrangement"),
        Pane::new(MIXER, MIXER, "Mixer"),
        Pane::new(SHOW, SHOW, format!("Lighting — {}", names.set)),
        Pane::new(CUES, CUES, "Cues"),
        {
            let mut chart = Pane::new(CHART, CHART, format!("Chart — {}", names.set));
            chart.state.clone_from(&names.chart_state);
            chart
        },
        Pane::new(LYRICS, LYRICS, "Lyrics"),
    ];
    let main = match &page {
        Some(p) => b.group(&p.id.0),
        None => b.main(area),
    };
    if let Some(p) = page {
        panes.push(p);
    }
    let (show, cues) = (b.group(SHOW), b.group(CUES));
    let lighting = b.split(Axis::Row, show, cues, 0.8);
    let (chart, lyrics) = (b.group(CHART), b.group(LYRICS));
    let chart_window = b.split(Axis::Column, chart, lyrics, 0.67);
    let mut ws = Workspace::new();
    ws.windows = vec![
        Window::new(WindowId::main(), main),
        Window {
            bounds: Some(Rect::new(0.0, 0.0, 2560.0, 1440.0)),
            ..Window::new(WindowId::new(LIGHTING_WINDOW), lighting)
        },
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
    fn every_tab_is_a_pane_of_the_layout() {
        let ws = stage(Area::default(), &Names::default(), None);
        for w in &ws.windows {
            for v in views(w.center.as_ref().unwrap()) {
                assert!(ws.pane(&PaneId::new(&v)).is_some(), "{v}");
            }
        }
        assert_eq!(ws.windows[1].id.0, LIGHTING_WINDOW);
    }
}

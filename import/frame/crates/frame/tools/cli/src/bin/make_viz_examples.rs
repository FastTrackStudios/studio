//! Write the visualizer example surfaces and the catalogs they play against.
//!
//! ```text
//! make_viz_examples [--out-dir examples/surfaces]
//! ```
//! `09-comp-viz.frame` (`viz.comp` and knobs), `10-eq-graph.frame`
//! (`viz.eq_graph` and a readout column per band) and `11-adsr.frame`
//! (`viz.adsr` with voice playheads, and knobs), each with a
//! `*.catalog.json` beside it, so the desktop player's mock engine serves
//! their params and feeds:
//!
//! ```text
//! nix develop . -c cargo run -p frame-desktop -- examples/surfaces/10-eq-graph.frame
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use frame::document::proto::{Node, NodeId, NodeKind, Scene, Size, TextAlign, TextData, Transform};
use frame::surface::kit::{Place, draw, viz};
use frame::surface::proto::{Binding, Catalog, Control};
use frame::surface::runtime::Surface;
use frame::surface::widgets::{Builder, Theme};

const BG: &str = "#0a0c10";
const INK: &str = "#e9ecef";
const DIM: &str = "#8e98a8";
const KNOB: &str = "knob.soft_pointer";

/// One example: file stem, the surface, its catalog, the catalog's file.
pub struct Example {
    pub file: &'static str,
    pub catalog_file: &'static str,
    pub surface: Surface,
    pub catalog: Catalog,
}

fn main() {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/surfaces");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out-dir" => dir = args.next().map(PathBuf::from).unwrap_or(dir),
            other => {
                eprintln!(
                    "make_viz_examples: unknown argument {other}\nusage: make_viz_examples [--out-dir DIR]"
                );
                std::process::exit(2);
            }
        }
    }
    for ex in examples() {
        let fm = dir.join(ex.file);
        let cat = dir.join(ex.catalog_file);
        let text = frame::document::text::print(&ex.surface.to_scene());
        let json = serde_json::to_string_pretty(&ex.catalog).expect("catalogs serialize");
        if let Err(e) = std::fs::write(&fm, text).and_then(|()| std::fs::write(&cat, json + "\n")) {
            eprintln!("make_viz_examples: writing {}: {e}", fm.display());
            std::process::exit(1);
        }
        println!(
            "{{\"ok\":true,\"surface\":\"{}\",\"catalog\":\"{}\",\"controls\":{}}}",
            fm.display(),
            cat.display(),
            ex.surface.controls.len()
        );
    }
}

#[must_use]
pub fn examples() -> Vec<Example> {
    vec![comp(), eq(), adsr()]
}

/// A new surface: a page with a dark root frame `w` wide, titled.
struct Doc {
    scene: Scene,
    controls: BTreeMap<NodeId, Control>,
    root: NodeId,
}

impl Doc {
    fn new(name: &str, w: f64, title: &str, subtitle: &str) -> Self {
        let mut scene = Scene::new(name);
        let page = scene.pages()[0];
        let mut root = draw::frame(name, 0.0, 0.0, w, 100.0);
        root.fills = vec![draw::solid(BG)];
        let root_id = root.id;
        scene
            .apply(frame::document::proto::Op::Insert {
                node: root,
                parent: page,
                index: 0,
            })
            .expect("page exists");
        let mut d = Self {
            scene,
            controls: BTreeMap::new(),
            root: root_id,
        };
        let mut t = draw::text(
            "title",
            title,
            24.0,
            16.0,
            w - 48.0,
            13.0,
            INK,
            TextAlign::Left,
        );
        if let NodeKind::Text(TextData {
            font_weight,
            letter_spacing,
            ..
        }) = &mut t.kind
        {
            *font_weight = Some(700.0);
            *letter_spacing = 2.0;
        }
        d.add(root_id, t);
        let s = draw::text(
            "subtitle",
            subtitle,
            24.0,
            34.0,
            w - 48.0,
            10.0,
            DIM,
            TextAlign::Left,
        );
        d.add(root_id, s);
        d
    }

    fn add(&mut self, parent: NodeId, node: Node) -> NodeId {
        let theme = Theme::stage_dark();
        Builder {
            scene: &mut self.scene,
            controls: &mut self.controls,
            theme: &theme,
        }
        .add(parent, node)
    }

    /// Place kit component `id` at (`x`, `y`) of the root.
    fn place(&mut self, id: &str, (x, y): (f64, f64), p: &Place) -> NodeId {
        let theme = Theme::stage_dark();
        let mut b = Builder {
            scene: &mut self.scene,
            controls: &mut self.controls,
            theme: &theme,
        };
        let node = frame::surface::kit::place(&mut b, self.root, id, p)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        self.scene.node_mut(node).expect("placed").transform = Transform::translate(x, y);
        node
    }

    /// A row of knobs centred in `(x, w)` at `y`, one per `(legend, addr)`.
    fn knobs(&mut self, (x, w): (f64, f64), y: f64, knobs: &[(&str, &str)]) {
        let theme = Theme::stage_dark();
        let cell = w / knobs.len() as f64;
        for (i, (legend, addr)) in knobs.iter().enumerate() {
            let mut b = Builder {
                scene: &mut self.scene,
                controls: &mut self.controls,
                theme: &theme,
            };
            let p = Place::new(*legend)
                .size(46.0)
                .bind(Binding::param(*addr))
                .legend(*legend);
            let knob = frame::surface::kit::place_in_cell(&mut b, self.root, KNOB, &p)
                .expect("knob places");
            let holder = self
                .scene
                .node(knob)
                .and_then(|n| n.parent)
                .expect("in its cell");
            let hw = self.scene.node(holder).map_or(0.0, |n| n.size.width);
            self.scene.node_mut(holder).expect("cell").transform =
                Transform::translate(x + cell * (i as f64 + 0.5) - hw / 2.0, y);
        }
    }

    /// Size the root to what it holds, plus a margin.
    fn finish(mut self, margin: f64) -> Surface {
        let (_, _, _, h) = frame::surface::kit::drawn_bounds(&self.scene, self.root);
        if let Some(n) = self.scene.node_mut(self.root) {
            n.size = Size {
                width: n.size.width,
                height: (h + margin).ceil(),
            };
        }
        Surface {
            scene: self.scene,
            controls: self.controls,
        }
    }
}

fn catalog(
    id: &str,
    params: Vec<frame::surface::proto::ParamInfo>,
    streams: Vec<frame::surface::proto::FeedInfo>,
) -> Catalog {
    Catalog {
        id: id.into(),
        version: 1,
        params,
        actions: Vec::new(),
        streams,
        collections: Vec::new(),
    }
}

fn comp() -> Example {
    let mut d = Doc::new(
        "Compressor",
        760.0,
        "COMPRESSOR",
        "viz.comp — input and gain-reduction history, transfer curve, drag the threshold (wheel: ratio)",
    );
    d.place(
        "viz.comp",
        (20.0, 54.0),
        &Place::new("Compressor").size(720.0).options(&["ns=comp"]),
    );
    d.knobs(
        (20.0, 720.0),
        372.0,
        &[
            ("Threshold", "comp/threshold"),
            ("Ratio", "comp/ratio"),
            ("Knee", "comp/knee"),
            ("Makeup", "comp/makeup"),
        ],
    );
    Example {
        file: "09-comp-viz.frame",
        catalog_file: "comp-viz.catalog.json",
        surface: d.finish(18.0),
        catalog: catalog(
            "viz.comp",
            viz::catalog::comp_params("comp"),
            viz::catalog::comp_streams("comp", 300),
        ),
    }
}

fn eq() -> Example {
    const BANDS: usize = 24;
    let mut d = Doc::new(
        "EQ",
        860.0,
        "PARAMETRIC EQ",
        "viz.eq_graph — double-click to add a band, drag to move (Alt: freq only, Cmd: Q), drag out to delete, double-click a band to rename, right-click for its menu",
    );
    let graph = Place::new("EQ")
        .size(820.0)
        .options(&["ns=eq", "bands=24"])
        .bind(Binding::stream("eq/spectrum"));
    d.place("viz.eq_graph", (20.0, 54.0), &graph);
    let mut params = viz::catalog::eq_params("eq", BANDS);
    params.extend(viz::catalog::eq_view_params("eq"));
    Example {
        file: "10-eq-graph.frame",
        catalog_file: "eq-graph.catalog.json",
        surface: d.finish(20.0),
        catalog: catalog("viz.eq", params, viz::catalog::eq_streams("eq", 128)),
    }
}

fn adsr() -> Example {
    let mut d = Doc::new(
        "Envelope",
        560.0,
        "AMP ENVELOPE",
        "viz.adsr — drag A, D, S, R; dots are the sounding voices",
    );
    let p = Place::new("Envelope").size(520.0).options(&[
        "ns=env",
        "playheads=env/voices",
        "voices=4",
        "time=3.5",
    ]);
    d.place("viz.adsr", (20.0, 54.0), &p);
    d.knobs(
        (20.0, 520.0),
        54.0 + (520.0 * 0.46_f64).round() + 16.0,
        &[
            ("Attack", "env/attack"),
            ("Decay", "env/decay"),
            ("Sustain", "env/sustain"),
            ("Release", "env/release"),
        ],
    );
    Example {
        file: "11-adsr.frame",
        catalog_file: "adsr.catalog.json",
        surface: d.finish(18.0),
        catalog: catalog(
            "viz.adsr",
            viz::catalog::adsr_params("env"),
            vec![viz::catalog::adsr_stream("env", 4)],
        ),
    }
}

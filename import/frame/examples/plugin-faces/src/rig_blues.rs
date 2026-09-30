//! Rig pages: the blues rig as sections a player swipes between
//! (docs/rig-sections.md). Every page carries the section bar — one icon
//! per section, a dot under the one showing — and is built from the
//! units' own faces (`face::embed`), so every control on it is the unit's
//! own, bound to the same params.
//!
//! Sections, in signal order: Input, Drives, Pre-FX, Amp/Cab, Post EQ/Comp,
//! Post-FX, Time. A **module** is a saved view of one or more sections the
//! player configures, and saves presets for, as one unit.
//!
//! - `01-drives`, `02-drives-phone` — the four drives on a board.
//! - `03-amp-cab`, `04-amp-cab-phone` — the head standing on its 2×12,
//!   the gate in front, the cab's mic rail under it.
//! - `05-module-drive-amp` — the Drive → Amp module: the drives' row, then
//!   the Amp/Cab/Gate section, one unit with one preset.
//! - `11…14`, `21`, `22` — the units themselves.

pub mod amp_cab;
pub mod drives;

use document_proto::{NodeId, TextAlign};
use surface_kit::draw::{self, Lettering, family};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::face::{Face, canvas, embed, face_size};
use crate::reverb::ui;
use crate::suite_studio_blues::amps;
use crate::suite_studio_blues::parts::set;

/// The plugin id every rig page's params are merged under.
pub const PLUGIN: &str = "rig-blues";

const BG: &str = "#0e0f12";
const INK: &str = "#e8e6e1";
const DIM: &str = "#ffffff45";
/// The section bar's height.
const BAR_H: f64 = 76.0;

/// The rig's sections, in signal order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Input,
    Drives,
    PreFx,
    AmpCab,
    PostEq,
    PostFx,
    Time,
}

impl Section {
    pub const ALL: [Self; 7] = [Self::Input, Self::Drives, Self::PreFx, Self::AmpCab, Self::PostEq, Self::PostFx, Self::Time];

    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Input => "INPUT",
            Self::Drives => "DRIVES",
            Self::PreFx => "PRE-FX",
            Self::AmpCab => "AMP / CAB",
            Self::PostEq => "POST EQ · COMP",
            Self::PostFx => "POST-FX",
            Self::Time => "TIME",
        }
    }

    /// The icon, drawn in a `s`-square box at (`x`, `y`) as strokes.
    fn icon(self, b: &mut Builder<'_>, parent: NodeId, (x, y, s): (f64, f64, f64), color: &str) {
        let p = |fx: f64, fy: f64| (x + fx * s, y + fy * s);
        let arc = |cx: f64, cy: f64, r: f64, a0: f64, a1: f64| -> Vec<(f64, f64)> {
            (0..=16).map(|i| {
                let a = (a0 + (a1 - a0) * f64::from(i) / 16.0).to_radians();
                p(cx + r * a.cos(), cy + r * a.sin())
            }).collect()
        };
        let lines: Vec<Vec<(f64, f64)>> = match self {
            // A jack plug: barrel, tip, cable.
            Self::Input => vec![vec![p(0.1, 0.5), p(0.35, 0.5)], vec![p(0.35, 0.36), p(0.72, 0.36), p(0.72, 0.64), p(0.35, 0.64), p(0.35, 0.36)], vec![p(0.72, 0.5), p(0.92, 0.5)]],
            // A stompbox: box, two knobs, the switch.
            Self::Drives => vec![
                vec![p(0.22, 0.08), p(0.78, 0.08), p(0.78, 0.92), p(0.22, 0.92), p(0.22, 0.08)],
                arc(0.37, 0.28, 0.08, 0.0, 360.0),
                arc(0.63, 0.28, 0.08, 0.0, 360.0),
                arc(0.5, 0.7, 0.1, 0.0, 360.0),
            ],
            // A spring.
            Self::PreFx => vec![(0..=40).map(|i| {
                let t = f64::from(i) / 40.0;
                p(0.08 + 0.84 * t, 0.5 + 0.22 * (t * std::f64::consts::TAU * 5.0).sin())
            }).collect()],
            // A head on a cab.
            Self::AmpCab => vec![
                vec![p(0.12, 0.08), p(0.88, 0.08), p(0.88, 0.36), p(0.12, 0.36), p(0.12, 0.08)],
                vec![p(0.12, 0.42), p(0.88, 0.42), p(0.88, 0.92), p(0.12, 0.92), p(0.12, 0.42)],
                arc(0.32, 0.67, 0.13, 0.0, 360.0),
                arc(0.68, 0.67, 0.13, 0.0, 360.0),
            ],
            // Three faders.
            Self::PostEq => {
                let mut v = Vec::new();
                for (fx, fy) in [(0.25, 0.3), (0.5, 0.65), (0.75, 0.45)] {
                    v.push(vec![p(fx, 0.1), p(fx, 0.9)]);
                    v.push(vec![p(fx - 0.09, fy), p(fx + 0.09, fy)]);
                }
                v
            }
            // A wave rising and falling in depth: modulation.
            Self::PostFx => vec![(0..=40).map(|i| {
                let t = f64::from(i) / 40.0;
                p(0.06 + 0.88 * t, 0.5 + (0.1 + 0.25 * t) * (t * std::f64::consts::TAU * 2.0).sin())
            }).collect()],
            // A clock.
            Self::Time => vec![arc(0.5, 0.5, 0.4, 0.0, 360.0), vec![p(0.5, 0.2), p(0.5, 0.5), p(0.72, 0.62)]],
        };
        b.add(parent, ui::strokes(self.title(), &lines, 2.0, draw::solid(color)));
    }
}

/// The section bar across the top of a page `w` wide: the rig's name,
/// the page's title, then an icon per section with a dot under each of
/// `current`. Returns the bar's height.
fn section_bar(b: &mut Builder<'_>, root: NodeId, w: f64, current: &[Section], title: &str) -> f64 {
    let bar = b.add(root, draw::rect("Section bar", 0.0, 0.0, w, BAR_H, draw::linear(180.0, &[(0.0, "#1a1b20"), (1.0, "#131418")])));
    b.add(bar, draw::rect("rule", 0.0, BAR_H - 1.0, w, 1.0, draw::solid("#ffffff14")));
    let compact = w < 600.0;
    // The icons: centred on a phone, right of the title on a wide page.
    let (s, gap) = if compact { (26.0, 20.0) } else { (28.0, 30.0) };
    let n = Section::ALL.len() as f64;
    let span = n * s + (n - 1.0) * gap;
    let (x0, iy) = if compact { ((w - span) / 2.0, 12.0) } else { (w - span - 28.0, 16.0) };
    for (i, sec) in Section::ALL.iter().enumerate() {
        let x = x0 + i as f64 * (s + gap);
        let on = current.contains(sec);
        sec.icon(b, bar, (x, iy, s), if on { INK } else { DIM });
        if on {
            b.add(bar, draw::circle("current", x + s / 2.0, iy + s + 11.0, 3.2, draw::solid(INK)));
        }
    }
    let head = Lettering::family(family::OSWALD).weight(600.0);
    if compact {
        set(b, root, title, w / 2.0, BAR_H + 22.0, 15.0, INK, &head, 0.12);
        BAR_H + 40.0
    } else {
        set(b, bar, "BLUES RIG", 28.0 + 50.0, BAR_H * 0.34, 11.0, "#ffffff80", &Lettering::family(family::INTER).weight(700.0), 0.2);
        let t = set(b, bar, title, 28.0, BAR_H * 0.64, 20.0, INK, &head, 0.1);
        if let Some(n) = b.scene.node_mut(t) {
            if let document_proto::NodeKind::Text(d) = &mut n.kind {
                d.align = TextAlign::Left;
            }
            n.transform.tx = 28.0;
        }
        BAR_H
    }
}

/// A page `w × h` on the stage background.
fn page(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64) {
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size { width: w, height: h };
        n.fills = vec![draw::linear(180.0, &[(0.0, "#16171b"), (0.6, BG), (1.0, "#08090b")])];
        n.clips_content = true;
    }
}

/// The params of every face on a page.
fn params_of(faces: &[&Face]) -> Vec<surface_proto::ParamInfo> {
    let mut out: Vec<surface_proto::ParamInfo> = Vec::new();
    for f in faces {
        for p in &f.params {
            if !out.iter().any(|q| q.addr == p.addr) {
                out.push(p.clone());
            }
        }
    }
    out
}

/// A pedalboard at (`x`, `y`) `w × h`: black aluminium rails.
fn board(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let base = draw::rounded(draw::rect("board", x, y, w, h, draw::linear(180.0, &[(0.0, "#26272b"), (1.0, "#141518")])), 10.0);
    b.add(parent, draw::with_effects(base, vec![draw::drop_shadow("#000000b0", 0.0, 10.0, 18.0)]));
    let rails = ((h - 24.0) / 22.0) as usize;
    for i in 0..rails {
        let ry = y + 14.0 + i as f64 * 22.0;
        b.add(parent, draw::rect("rail slot", x + 12.0, ry, w - 24.0, 8.0, draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1c1d20")])));
    }
}

/// `faces` in a row from `x`, bottoms on `floor`, `gap` apart at
/// `scale`. Returns the row's right edge.
fn row(b: &mut Builder<'_>, parent: NodeId, faces: &[Face], x: f64, floor: f64, gap: f64, scale: f64) -> f64 {
    let mut cx = x;
    for f in faces {
        let (w, h) = face_size(f);
        embed(b, parent, f, (cx, floor - h * scale, w * scale, h * scale));
        cx += w * scale + gap;
    }
    cx - gap
}

fn row_width(faces: &[Face], gap: f64, scale: f64) -> f64 {
    faces.iter().map(|f| face_size(f).0 * scale).sum::<f64>() + gap * (faces.len() as f64 - 1.0)
}

/// The Drives section: the four on a board.
#[must_use]
pub fn drives_page() -> Face {
    let pedals = drives::all();
    const GAP: f64 = 30.0;
    let rw = row_width(&pedals, GAP, 1.0);
    let (w, h) = (rw + 160.0, BAR_H + 460.0);
    let scene = canvas("Drives", |b, root| {
        page(b, root, w, h);
        let top = section_bar(b, root, w, &[Section::Drives], "DRIVES");
        let (bx, by, bw, bh) = (40.0, top + 50.0, w - 80.0, 360.0);
        board(b, root, (bx, by, bw, bh));
        row(b, root, &pedals, (w - rw) / 2.0, by + bh - 36.0, GAP, 1.0);
    });
    let params = params_of(&pedals.iter().collect::<Vec<_>>());
    Face { file: "01-drives", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Drives section on a phone: two by two, the wide gold one across.
#[must_use]
pub fn drives_phone() -> Face {
    const W: f64 = 390.0;
    const H: f64 = 844.0;
    let pedals = drives::all();
    let scene = canvas("Drives (phone)", |b, root| {
        page(b, root, W, H);
        let top = section_bar(b, root, W, &[Section::Drives], "DRIVES");
        // Row 1: the boost and the green one; row 2: the gold one alone;
        // row 3: the British one. Each row scaled to fit 350 wide.
        let rows: [&[usize]; 3] = [&[0, 3], &[1], &[2]];
        let mut y = top + 12.0;
        for r in rows {
            let make: [fn() -> Face; 4] = [drives::blade, drives::pegasus, drives::british_blues, drives::tube_drive];
            let faces: Vec<Face> = r.iter().map(|&i| make[i]()).collect();
            let scale = (350.0 / row_width(&faces, 16.0, 1.0)).min(0.74);
            let rw = row_width(&faces, 16.0, scale);
            y += 286.0 * scale;
            row(b, root, &faces, (W - rw) / 2.0, y, 16.0, scale);
            y += 14.0;
        }
    });
    let params = params_of(&pedals.iter().collect::<Vec<_>>());
    Face { file: "02-drives-phone", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Amp/Cab section's parts at `scale` from (`x`, `y`): the gate
/// (if `gate_x` is given, at that x on the floor), the head on its cab,
/// the rail under. Returns the bottom.
struct AmpCab {
    head: Face,
    cab: Face,
    gate: Face,
}

impl AmpCab {
    fn new() -> Self {
        Self { head: amps::clean_head(), cab: amp_cab::cab(), gate: amp_cab::gate() }
    }
    fn faces(&self) -> Vec<&Face> {
        vec![&self.gate, &self.head, &self.cab]
    }
    /// The stack at (`x`, `y`), `scale`: head on cab, rail under (in
    /// `rows`). Returns the cab's floor line and the rail's bottom.
    fn stack(&self, b: &mut Builder<'_>, parent: NodeId, (x, y): (f64, f64), scale: f64, rows: usize) -> (f64, f64) {
        let (cw, _) = face_size(&self.cab);
        self.stack_with_rail(b, parent, (x, y), scale, Some((x, cw * scale, rows)))
    }
    /// As [`Self::stack`], the rail (if any) at its own x, width and rows.
    fn stack_with_rail(&self, b: &mut Builder<'_>, parent: NodeId, (x, y): (f64, f64), scale: f64, rail: Option<(f64, f64, usize)>) -> (f64, f64) {
        let (hw, hh) = face_size(&self.head);
        let (cw, ch) = face_size(&self.cab);
        let cx = x + (cw - hw) * scale / 2.0;
        embed(b, parent, &self.head, (cx, y, hw * scale, hh * scale));
        let cy = y + hh * scale - 6.0 * scale;
        embed(b, parent, &self.cab, (x, cy, cw * scale, ch * scale));
        let floor = cy + ch * scale;
        let Some((rx, rw, rows)) = rail else { return (floor, floor) };
        let rail = amp_cab::cab_rail(b, parent, (rx, floor + 24.0, rw), rows);
        let rh = b.scene.node(rail).map_or(0.0, |n| n.size.height);
        (floor, floor + 24.0 + rh)
    }
}

/// The Amp/Cab section: the head on its 2×12, the gate on the floor in
/// front of it, the mics' rail under.
#[must_use]
pub fn amp_cab_page() -> Face {
    let ac = AmpCab::new();
    let (gw, _) = face_size(&ac.gate);
    let (w, h) = (40.0 + gw + 40.0 + 1200.0 + 40.0, BAR_H + 40.0 + 548.0 + 700.0 + 24.0 + 108.0 + 40.0);
    let scene = canvas("Amp / Cab", |b, root| {
        page(b, root, w, h);
        let top = section_bar(b, root, w, &[Section::AmpCab], "AMP / CAB · GATE");
        let (floor, _) = ac.stack(b, root, (40.0 + gw + 40.0, top + 40.0), 1.0, 1);
        row(b, root, std::slice::from_ref(&ac.gate), 40.0, floor, 0.0, 1.0);
    });
    let params = params_of(&ac.faces());
    Face { file: "03-amp-cab", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Amp/Cab section on a phone: the stack across the screen, the gate
/// and the rail (in two rows) under it.
#[must_use]
pub fn amp_cab_phone() -> Face {
    const W: f64 = 390.0;
    const H: f64 = 844.0;
    let ac = AmpCab::new();
    let scene = canvas("Amp / Cab (phone)", |b, root| {
        page(b, root, W, H);
        let top = section_bar(b, root, W, &[Section::AmpCab], "AMP / CAB · GATE");
        let scale = (W - 24.0) / 1200.0;
        // The gate at the right, the cab's rail (two rows) beside it.
        let gs = 0.75;
        let (gw, gh) = face_size(&ac.gate);
        let rail_w = W - 24.0 - gw * gs - 12.0;
        let (floor, _) = ac.stack_with_rail(b, root, (12.0, top + 8.0), scale, Some((12.0, rail_w, 2)));
        embed(b, root, &ac.gate, (W - 12.0 - gw * gs, floor + 24.0, gw * gs, gh * gs));
    });
    let params = params_of(&ac.faces());
    Face { file: "04-amp-cab-phone", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Drive → Amp module: one unit, saved and recalled as one preset —
/// the drives' row on the floor, then the gate, then the head on its cab.
#[must_use]
pub fn module_drive_amp() -> Face {
    const GAP: f64 = 28.0;
    // The pedals a little over the amp's scale, so they read at a glance.
    const PS: f64 = 1.35;
    let pedals = drives::all();
    let ac = AmpCab::new();
    let rw = row_width(&pedals, GAP, PS);
    let (gw, _) = face_size(&ac.gate);
    let gw = gw * PS;
    let stack_x = 40.0 + rw + 40.0 + gw + 60.0;
    let (w, h) = (stack_x + 1200.0 + 40.0, BAR_H + 40.0 + 548.0 + 700.0 + 24.0 + 108.0 + 40.0);
    let scene = canvas("Drive → Amp", |b, root| {
        page(b, root, w, h);
        let top = section_bar(b, root, w, &[Section::Drives, Section::AmpCab], "MODULE · DRIVE → AMP");
        // The module's preset, saved and recalled as one.
        let pill = draw::rounded(draw::rect("Preset", 330.0, 20.0, 300.0, 36.0, draw::solid("#00000088")), 18.0);
        let pill = b.add(root, draw::stroked(pill, "#ffffff30", 1.0));
        set(b, pill, "PRESET   Slow Hand Clean", 140.0, 18.0, 12.0, INK, &Lettering::family(family::INTER).weight(600.0), 0.04);
        b.add(pill, ui::strokes("open", &[vec![(268.0, 15.0), (274.0, 21.0), (280.0, 15.0)]], 1.6, draw::solid(INK)));
        let (floor, _) = ac.stack(b, root, (stack_x, top + 40.0), 1.0, 1);
        let bh = 286.0 * PS + 70.0;
        board(b, root, (24.0, floor - bh + 36.0, rw + 32.0 + 40.0 + gw, bh));
        let end = row(b, root, &pedals, 40.0, floor, GAP, PS);
        // The chain runs on past the drives: the gate, then the amp.
        row(b, root, std::slice::from_ref(&ac.gate), end + 40.0, floor, 0.0, PS);
    });
    let mut faces: Vec<&Face> = pedals.iter().collect();
    faces.extend(ac.faces());
    let params = params_of(&faces);
    Face { file: "05-module-drive-amp", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── The Control view's pages (signal): a bar, and rows without bars ────

/// What the section bar switches between: the Drive → Amp module (the
/// two-row default), then each section.
pub const VIEWS: [&str; 8] = ["Drive → Amp", "Input", "Drives", "Pre-FX", "Amp / Cab", "Post EQ · Comp", "Post-FX", "Time"];

/// The Control view's section bar, `00-section-bar`: the module chip and
/// an icon per section, each a select button on `rig/section` whose dot
/// (and bright icon) shows while it is the one showing. 1400 × 52.
#[must_use]
pub fn section_bar_page() -> Face {
    const W: f64 = 1400.0;
    const H: f64 = 52.0;
    let n = VIEWS.len() as u32;
    let scene = canvas("Section bar", |b, root| {
        page(b, root, W, H);
        if let Some(r) = b.scene.node_mut(root) {
            r.fills = vec![draw::linear(180.0, &[(0.0, "#1a1b20"), (1.0, "#131418")])];
        }
        b.add(root, draw::rect("rule", 0.0, H - 1.0, W, 1.0, draw::solid("#ffffff14")));
        let select = |b: &mut Builder<'_>, cell: NodeId, lit: NodeId, i: u32| {
            let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i, count: n } }).slot(SlotRole::Led, lit.0);
            c.binding = Some(ui::bind("rig/section"));
            b.controls.insert(cell, c);
        };
        // The module chip: its name in a pill, lit when chosen.
        let head = Lettering::family(family::OSWALD).weight(600.0);
        let (cw, ch) = (170.0, 32.0);
        let cell = b.add(root, draw::frame("Drive → Amp", 16.0, (H - ch) / 2.0 - 3.0, cw, ch + 6.0));
        b.add(cell, draw::stroked(draw::rounded(draw::rect("pill", 0.0, 0.0, cw, ch, draw::solid("#00000000")), ch / 2.0), "#ffffff30", 1.0));
        let lit = b.add(cell, draw::frame("lit", 0.0, 0.0, cw, ch + 6.0));
        b.add(lit, draw::rounded(draw::rect("fill", 0.0, 0.0, cw, ch, draw::solid("#ffffff1c")), ch / 2.0));
        b.add(lit, draw::circle("dot", cw / 2.0, ch + 3.5, 2.6, draw::solid(INK)));
        set(b, cell, "DRIVE → AMP", cw / 2.0, ch / 2.0, 13.0, INK, &head, 0.12);
        select(b, cell, lit, 0);
        // The sections.
        let (s, gap) = (26.0, 40.0);
        let x0 = 16.0 + cw + 48.0;
        for (i, sec) in Section::ALL.iter().enumerate() {
            let x = x0 + i as f64 * (s + gap);
            let cell = b.add(root, draw::frame(sec.title(), x - gap / 2.0 + s / 2.0 - 22.0, 0.0, 44.0, H));
            let ox = 22.0 - s / 2.0;
            sec.icon(b, cell, (ox, 8.0, s), DIM);
            let lit = b.add(cell, draw::frame("lit", 0.0, 0.0, 44.0, H));
            sec.icon(b, lit, (ox, 8.0, s), INK);
            b.add(lit, draw::circle("dot", 22.0, 8.0 + s + 8.0, 2.6, draw::solid(INK)));
            select(b, cell, lit, i as u32 + 1);
        }
        let small = Lettering::family(family::INTER).weight(700.0);
        set(b, root, "BLUES RIG", W - 70.0, H / 2.0, 11.0, "#ffffff70", &small, 0.2);
    });
    let params = vec![ui::choice("rig/section", "Section", &VIEWS, 0)];
    Face { file: "00-section-bar", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// A row of the Control view on a 14" laptop, width : height (1126 × 260).
const ROW_ASPECT: f64 = 4.33;

/// The Drives row: the four on a board, no bar, spread to the row's
/// proportion so the pedals are as tall as the row allows.
#[must_use]
pub fn row_drives() -> Face {
    let pedals = drives::all();
    let h = 286.0 + 50.0;
    let w = h * ROW_ASPECT;
    let gap = (w - 80.0 - row_width(&pedals, 0.0, 1.0)) / (pedals.len() as f64 - 1.0);
    let scene = canvas("Drives row", |b, root| {
        page(b, root, w, h);
        board(b, root, (10.0, 8.0, w - 20.0, h - 16.0));
        row(b, root, &pedals, 40.0, h - 22.0, gap, 1.0);
    });
    let params = params_of(&pedals.iter().collect::<Vec<_>>());
    Face { file: "31-row-drives", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Amp/Cab row: the head on the left, the cab (at the head's height)
/// on its right, the gate right of the cab, and the mics' rail after it —
/// all on one floor.
#[must_use]
pub fn row_amp_cab() -> Face {
    let ac = AmpCab::new();
    let (hw, hh) = face_size(&ac.head);
    let cab = amp_cab::cab_sized(1200.0, hh);
    let (cw, ch) = face_size(&cab);
    let (gw, gh) = face_size(&ac.gate);
    let (m, gap, rail_w) = (24.0, 36.0, 230.0);
    let w = m + hw + gap + cw + gap + gw + gap + rail_w + m;
    let h = m + ch.max(hh) + m;
    let scene = canvas("Amp / Cab row", |b, root| {
        page(b, root, w, h);
        let floor = h - m;
        embed(b, root, &ac.head, (m, floor - hh, hw, hh));
        let cx = m + hw + gap;
        embed(b, root, &cab, (cx, floor - ch, cw, ch));
        let gx = cx + cw + gap;
        embed(b, root, &ac.gate, (gx, floor - gh, gw, gh));
        amp_cab::cab_rail(b, root, (gx + gw + gap, m + 30.0, rail_w), 4);
    });
    let mut faces = ac.faces();
    faces.push(&cab);
    let params = params_of(&faces);
    Face { file: "32-row-amp-cab", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// Every blues-rig page, then the units.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v = vec![drives_page(), drives_phone(), amp_cab_page(), amp_cab_phone(), module_drive_amp(), section_bar_page(), row_drives(), row_amp_cab()];
    v.extend(drives::all());
    v.push(amp_cab::cab());
    v.push(amp_cab::gate());
    v
}

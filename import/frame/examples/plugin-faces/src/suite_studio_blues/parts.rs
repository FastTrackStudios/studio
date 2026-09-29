//! What the studio-blues suite's units share: the hand-built pedal (its
//! painted enclosure, marker lettering, caps, toggles, tape label, stomp)
//! and small panel pieces for the amps and rack units.

use document_proto::{Node, NodeId, NodeKind, TextAlign};
use surface_kit::boutique::letters::{self, Hand};
use surface_kit::boutique::pedal::add_tape_label;
use surface_kit::{Place, TOUCH_MIN};
use surface_proto::{Binding, ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::reverb::ui;

/// The plugin id every unit's catalog merges into.
pub const PLUGIN: &str = "suite-studio-blues";

/// Every pedal is the Standard class's height (60 px/in).
pub const PEDAL_H: f64 = 286.0;

/// The chalky white of a paint marker on the enclosures.
pub const CHALK: &str = "#ebebf0";

/// A pedal of class width `w`: its painted enclosure, sized into `root`.
/// `trim` is the colour the paint wore through to.
pub fn pedal(b: &mut Builder<'_>, root: NodeId, w: f64, trim: Option<&str>) -> NodeId {
    ui::fit_root(b, root, w, PEDAL_H);
    let mut opts = vec![format!("w={w}"), format!("h={PEDAL_H}")];
    if let Some(t) = trim {
        opts.push(format!("trim={t}"));
    }
    let p = Place { options: Some(opts), ..Place::new("Enclosure").variant("navy") };
    ui::put_at(b, root, "panel.painted_pedal", &p, 0.0, 0.0)
}

/// Marker lettering centred on (`cx`, `cy`), `size` px capitals.
pub fn letter(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64) -> NodeId {
    b.add(parent, letters::hand("lettering", s, cx, cy, &Hand::marker(size), CHALK))
}

/// Lettering in any hand and colour.
#[allow(clippy::too_many_arguments)]
pub fn letter_in(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, hand: &Hand, color: &str) -> NodeId {
    b.add(parent, letters::hand("lettering", s, cx, cy, hand, color))
}

/// A kit knob `style`/`variant`, `d` across, shaft at (`cx`, `cy`), bound
/// to `addr` — no printed legend or readout (the face letters its own).
#[allow(clippy::too_many_arguments)]
pub fn knob(b: &mut Builder<'_>, parent: NodeId, style: &str, variant: &str, addr: &str, name: &str, d: f64, cx: f64, cy: f64) -> NodeId {
    let p = Place::new(name).size(d).variant(variant).bind(Binding::param(addr)).legend("").scale(&[]).touch(TOUCH_MIN);
    let k = ui::put(b, parent, style, &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// A knob with a printed scale round it, in `ink`.
#[allow(clippy::too_many_arguments)]
pub fn scaled_knob(b: &mut Builder<'_>, parent: NodeId, style: &str, variant: &str, addr: &str, name: &str, d: f64, cx: f64, cy: f64, scale: &[&str], ink: &str) -> NodeId {
    let p = Place::new(name).size(d).variant(variant).bind(Binding::param(addr)).legend("").scale(scale).ink(surface_kit::Ink::Color(ink.into())).keep_case().touch(TOUCH_MIN);
    let k = ui::put(b, parent, style, &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// A mini bat toggle through the lid on its nut, centred at (`cx`, `cy`).
pub fn toggle(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, size: f64, cx: f64, cy: f64) -> NodeId {
    let p = Place::new(name).size(size).variant("chrome").bind(Binding::param(addr)).legend("").touch(TOUCH_MIN);
    let t = ui::put(b, parent, "switch.toggle_nut", &p, cx, cy);
    ui::strip_legend(b, t);
    t
}

/// The stomp with its LED beside it.
#[allow(clippy::too_many_arguments)]
pub fn stomp(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, d: f64, cx: f64, cy: f64, led: &str, led_left: bool) -> NodeId {
    let side = if led_left { "side=left" } else { "side=right" };
    let p = Place::new(name).size(d).bind(Binding::param(addr)).legend("").options(&[side, &format!("color={led}")]);
    let root = surface_kit::place(b, parent, "switch.stomp_side_led", &p).unwrap_or_else(|e| panic!("{e}"));
    // Centre the actuator (not the pair) on (cx, cy).
    let (w, h) = b.scene.node(root).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let ax = if led_left { w - d * 0.85 } else { d * 0.85 };
    ui::move_to(b, root, cx - ax, cy - h / 2.0);
    root
}

/// A label-maker strip, `text` on `tape`, a hair crooked.
#[allow(clippy::too_many_arguments)]
pub fn tape(b: &mut Builder<'_>, parent: NodeId, text: &str, cx: f64, cy: f64, w: f64, h: f64, color: &str, tilt: f64) -> NodeId {
    add_tape_label(b, parent, text, cx, cy, w, h, color, tilt)
}

/// A radio column of small lamps, one per option of choice param `addr`,
/// each lit when its option is chosen: (`x`, `y`) is the first lamp, `dy`
/// the pitch.
#[allow(clippy::too_many_arguments)]
pub fn radio(b: &mut Builder<'_>, parent: NodeId, addr: &str, n: usize, x: f64, y: f64, dy: f64, d: f64, color: &str) {
    for i in 0..n {
        let cell = b.add(parent, surface_kit::draw::frame(&format!("mode {i}"), x - dy / 2.0, y + dy * i as f64 - dy / 2.0, dy, dy));
        b.add(cell, surface_kit::draw::stroked(surface_kit::draw::circle("ring", dy / 2.0, dy / 2.0, d / 2.0, surface_kit::draw::solid("#00000000")), "#d8d8dc", 1.0));
        let lit = surface_kit::draw::circle("lamp", dy / 2.0, dy / 2.0, d / 2.0 - 0.6, surface_kit::draw::solid(color));
        let lit = b.add(cell, surface_kit::draw::emissive(lit, surface_kit::draw::hex(color), 0.6, d * 1.5));
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: n as u32 } }).slot(SlotRole::Led, lit.0);
        c.binding = Some(Binding::param(addr));
        b.controls.insert(cell, c);
    }
}

/// Set type (the units that are printed, not lettered): `weight`, tracked
/// `track` em, centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn print(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64, color: &str, weight: f64, track: f64) -> NodeId {
    let w = s.chars().count() as f64 * size * (0.7 + track) + size * 2.0;
    let mut t: Node = ui::text("print", s, cx - w / 2.0, cy, w, size, color, TextAlign::Center, weight, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * track;
    }
    b.add(parent, t)
}

/// Set type aligned left from `x`.
#[allow(clippy::too_many_arguments)]
pub fn print_left(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, size: f64, color: &str, weight: f64, track: f64) -> NodeId {
    let w = s.chars().count() as f64 * size * (0.7 + track) + size * 2.0;
    let mut t: Node = ui::text("print", s, x, cy, w, size, color, TextAlign::Left, weight, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * track;
    }
    b.add(parent, t)
}

/// Set type aligned right to `x`.
#[allow(clippy::too_many_arguments)]
pub fn print_right(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, size: f64, color: &str, weight: f64, track: f64) -> NodeId {
    let w = s.chars().count() as f64 * size * (0.7 + track) + size * 2.0;
    let mut t: Node = ui::text("print", s, x - w, cy, w, size, color, TextAlign::Right, weight, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * track;
    }
    b.add(parent, t)
}

//! The mixer as layers, run by the host.
//!
//! Two shapes of one thing: [`Shape::Strips`] (the mixer: a strip per
//! track, left to right, a fader up and down) and [`Shape::Rows`] (the
//! track control panel: a row per track, top to bottom, a fader left to
//! right). Per track:
//!
//! | layer | what | moves with |
//! |---|---|---|
//! | the chunk's chrome | backgrounds, names, fader and pan tracks, unlit buttons, and the **hit regions**: fader, pan, mute, solo | scroll |
//! | knob | the fader's knob | the host's drag value `host.daw.track.<n>.volume` |
//! | pan | a bar from the centre | `host.daw.track.<n>.pan` (scaled) |
//! | meter | a bar from the bottom (the left) | the engine's `engine.daw.track.<n>.peak` |
//! | lit | mute and solo when on, the fader's dB | re-recorded when the document changes |
//!
//! A fader drag is the host's from press to release: it moves the knob at
//! the display's rate and writes the value into the engine's shared
//! table, where the engine reads it at once; the view hears the release
//! and writes the document once. Scrolling is a value the view writes
//! (`view.studio.mixer.scroll`) that every layer's position follows, so a
//! scroll frame is one value, whatever the track count.

use std::sync::Arc;

use anyrender::recording::RenderCommand;
use architect_workspace::paint::anim::{Animation, Input, Linear, Name, Property};
use architect_workspace::paint::peniko::Color;
use architect_workspace::paint::peniko::kurbo::{Affine, Rect};
use architect_workspace::paint::region::{Axis, Cursor, Drag, Region, RegionEvent, Shape as Hit};
use architect_workspace::paint::{Layer, LayerKey, PaintScene as _, PaintView, Scene};
use architect_workspace::{HostData, ViewCtx, ViewInput};
use daw_view::{Field, Track, fader, names};
use paint_kit::{Palette, mix, rect, rgb, text};

/// Which way the tracks go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The mixer: a strip per track, across.
    Strips,
    /// The track control panel: a row per track, down.
    Rows,
}

/// A strip's width (mixer) or a row's height (TCP).
pub const STRIP: f64 = 76.0;
pub const ROW: f64 = 52.0;
/// Tracks a chrome layer holds (four regions each, within a layer's 1,024).
pub const CHUNK: usize = 64;

const CHROME: LayerKey = 10_000;
const KNOB: LayerKey = 20_000;
const PAN: LayerKey = 30_000;
const METER: LayerKey = 40_000;
const LIT: LayerKey = 50_000;
const BACKDROP: LayerKey = 1;
const MASTER: LayerKey = 2;

/// What the mixer shows.
#[derive(Debug, Clone, Default)]
pub struct Model {
    pub revision: u64,
    pub tracks: Vec<Track>,
    pub engine: bool,
    pub problem: Option<String>,
}

/// What a gesture asks the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Set {
    /// A track's field, into the document.
    Track {
        lane: String,
        field: Field,
        value: f64,
    },
    /// The transport.
    Transport(daw_view::Transport),
}

/// The transport's two buttons: region ids past any track's.
const PLAY: u64 = u64::MAX - 1;
const STOP: u64 = u64::MAX - 2;
const TRANSPORT: LayerKey = 3;

/// What the view says of itself, for its state action.
#[derive(Debug, Clone, Default)]
pub struct Shared {
    pub scroll: f64,
    pub layers: usize,
    pub records: u64,
    /// Region events heard, and the drag values among them: a drag's
    /// frames reach the view at most once each.
    pub heard: u64,
    pub values_heard: u64,
    pub sets: u64,
}

/// One track's geometry, in its own space (origin at the strip's or
/// row's corner).
struct Geometry {
    fader: Rect,
    pan: Rect,
    mute: Rect,
    solo: Rect,
    meter: Rect,
    name: (f64, f64, f32),
}

fn geometry(shape: Shape, extent: f64) -> Geometry {
    match shape {
        Shape::Strips => {
            let bottom = (extent - 40.0).max(160.0);
            Geometry {
                pan: Rect::new(8.0, 34.0, STRIP - 8.0, 44.0),
                mute: Rect::new(8.0, 52.0, 34.0, 70.0),
                solo: Rect::new(STRIP - 34.0, 52.0, STRIP - 8.0, 70.0),
                fader: Rect::new(24.0, 84.0, 32.0, bottom),
                meter: Rect::new(48.0, 84.0, 56.0, bottom),
                name: (6.0, extent - 16.0, (STRIP - 12.0) as f32),
            }
        }
        Shape::Rows => {
            let right = extent.max(360.0);
            Geometry {
                name: (14.0, 20.0, 120.0),
                mute: Rect::new(14.0, 28.0, 36.0, 44.0),
                solo: Rect::new(40.0, 28.0, 62.0, 44.0),
                pan: Rect::new(140.0, 30.0, 200.0, 40.0),
                fader: Rect::new(214.0, 22.0, right - 30.0, 30.0),
                meter: Rect::new(214.0, 36.0, right - 30.0, 42.0),
            }
        }
    }
}

/// The region ids of track `n`.
const fn ids(n: usize) -> [u64; 4] {
    let b = n as u64 * 4;
    [b, b + 1, b + 2, b + 3]
}

pub struct MixerView {
    shape: Shape,
    model: HostData<Model>,
    shared: HostData<Shared>,
    sets: Box<dyn Fn(Set)>,
    seen: Option<(u64, bool, i64, i64)>,
    layers: Vec<(LayerKey, Layer)>,
    scroll: f64,
    dragging: Option<(f64, f64)>,
    /// The latest value of each drag, by region.
    held: std::collections::HashMap<u64, f64>,
    size: (f64, f64),
}

fn arc(s: Scene) -> Arc<Vec<RenderCommand>> {
    Arc::new(s.commands)
}

fn name(n: &str) -> Option<Name> {
    Name::new(n).ok()
}

impl MixerView {
    #[must_use]
    pub fn new(
        shape: Shape,
        model: HostData<Model>,
        shared: HostData<Shared>,
        sets: Box<dyn Fn(Set)>,
    ) -> Self {
        Self {
            shape,
            model,
            shared,
            sets,
            seen: None,
            layers: Vec::new(),
            scroll: 0.0,
            dragging: None,
            held: std::collections::HashMap::new(),
            size: (0.0, 0.0),
        }
    }

    fn scroll_name() -> Option<Name> {
        name("view.studio.mixer.scroll")
    }

    /// Where track `i` starts along the tracks' axis.
    fn offset(&self, i: usize) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "a track")]
        let i = i as f64;
        match self.shape {
            Shape::Strips => i * STRIP,
            Shape::Rows => i * ROW,
        }
    }

    #[expect(clippy::too_many_lines, reason = "the layer table")]
    fn record(&mut self, p: &Palette, extent: f64) {
        let shape = self.shape;
        let g = geometry(shape, extent);
        let tracks = self.model.read(|m| m.tracks.clone());
        let mut out: Vec<(LayerKey, Layer)> = Vec::new();
        let mut back = Scene::new();
        rect(&mut back, Rect::new(0.0, 0.0, 1.0e5, 1.0e5), p.background);
        if tracks.is_empty() {
            let why = self.model.read(|m| {
                if m.engine {
                    "The session has no tracks.".to_owned()
                } else {
                    "No engine is playing a session.".to_owned()
                }
            });
            text(&mut back, &why, 12.0, (16.0, 24.0), 600.0, p.ink_dim);
        }
        out.push((BACKDROP, Layer::new(arc(back))));
        let scroll = Self::scroll_name();
        let follow = |prop: Property, base: f64| -> Vec<Animation> {
            scroll
                .clone()
                .map(|s| {
                    vec![Animation::linear(
                        prop,
                        Input::Value(s),
                        Linear::new(-1.0, base),
                    )]
                })
                .unwrap_or_default()
        };
        let axis_prop = match shape {
            Shape::Strips => Property::X,
            Shape::Rows => Property::Y,
        };
        for (c, chunk) in tracks.chunks(CHUNK).enumerate() {
            let mut s = Scene::new();
            let mut regions = Vec::new();
            for (k, t) in chunk.iter().enumerate() {
                let i = c * CHUNK + k;
                // In the chunk's space: the chunk sits at its first track.
                let local = self.offset(i) - self.offset(c * CHUNK);
                let (ox, oy) = match shape {
                    Shape::Strips => (local, 0.0),
                    Shape::Rows => (0.0, local),
                };
                let at = |r: Rect| Rect::new(r.x0 + ox, r.y0 + oy, r.x1 + ox, r.y1 + oy);
                let (w, h) = match shape {
                    Shape::Strips => (STRIP, extent),
                    Shape::Rows => (extent, ROW),
                };
                let shade = if i % 2 == 0 {
                    p.surface
                } else {
                    mix(p.surface, p.ink, 0.03)
                };
                rect(&mut s, Rect::new(ox, oy, ox + w, oy + h), shade);
                let edge = match shape {
                    Shape::Strips => Rect::new(ox + w - 1.0, oy, ox + w, oy + h),
                    Shape::Rows => Rect::new(ox, oy + h - 1.0, ox + w, oy + h),
                };
                rect(&mut s, edge, p.border);
                let colour = t.color.map_or(p.border, rgb);
                match shape {
                    Shape::Strips => {
                        rect(&mut s, Rect::new(ox, oy, ox + w - 1.0, oy + 4.0), colour)
                    }
                    Shape::Rows => rect(&mut s, Rect::new(ox, oy, ox + 4.0, oy + h - 1.0), colour),
                }
                let (nx, ny, room) = g.name;
                text(&mut s, &t.name, 11.0, (nx + ox, ny + oy), room, p.ink);
                // Index above the pan in a strip.
                if shape == Shape::Strips {
                    text(
                        &mut s,
                        &(i + 1).to_string(),
                        10.0,
                        (ox + 6.0, oy + 22.0),
                        30.0,
                        p.ink_dim,
                    );
                }
                let well = mix(p.background, p.ink, 0.12);
                rect(&mut s, at(g.fader), well);
                rect(&mut s, at(g.pan), well);
                rect(&mut s, at(g.meter), mix(p.background, p.ink, 0.06));
                // Unity on the fader's track.
                let unity = match shape {
                    Shape::Strips => {
                        let y = g.fader.y1 - fader::UNITY * g.fader.height();
                        Rect::new(g.fader.x0 - 6.0, y - 0.5, g.fader.x1 + 6.0, y + 0.5)
                    }
                    Shape::Rows => {
                        let x = g.fader.x0 + fader::UNITY * g.fader.width();
                        Rect::new(x - 0.5, g.fader.y0 - 4.0, x + 0.5, g.fader.y1 + 4.0)
                    }
                };
                rect(&mut s, at(unity), p.ink_dim);
                for (r, label) in [(g.mute, "M"), (g.solo, "S")] {
                    rect(&mut s, at(r), mix(p.surface, p.ink, 0.1));
                    text(
                        &mut s,
                        label,
                        10.0,
                        (at(r).x0 + 8.0, at(r).y1 - 5.0),
                        14.0,
                        p.ink_dim,
                    );
                }
                // The regions: the host hit-tests them and runs the drags.
                let [f, pn, m, so] = ids(i);
                let grow = |r: Rect| Rect::new(r.x0 - 8.0, r.y0 - 6.0, r.x1 + 8.0, r.y1 + 6.0);
                let (vol, pan) = (name(&names::volume(t.index)), name(&names::pan(t.index)));
                if let (Some(vol), Some(pan)) = (vol, pan) {
                    let (faxis, fscale, fcursor) = match shape {
                        Shape::Strips => (Axis::Y, -1.0 / g.fader.height(), Cursor::NsResize),
                        Shape::Rows => (Axis::X, 1.0 / g.fader.width(), Cursor::EwResize),
                    };
                    regions.push(
                        Region::new(f, Hit::Rect(grow(at(g.fader))))
                            .cursor(fcursor)
                            .hover(Color::from_rgba8(255, 255, 255, 18))
                            .drag(Drag {
                                value: vol,
                                axis: faxis,
                                scale: fscale,
                                min: 0.0,
                                max: 1.0,
                                step: None,
                                initial: t.volume.clamp(0.0, 1.0),
                                coast: None,
                            }),
                    );
                    regions.push(
                        Region::new(pn, Hit::Rect(grow(at(g.pan))))
                            .cursor(Cursor::EwResize)
                            .hover(Color::from_rgba8(255, 255, 255, 18))
                            .drag(Drag {
                                value: pan,
                                axis: Axis::X,
                                scale: 2.0 / g.pan.width(),
                                min: -1.0,
                                max: 1.0,
                                step: None,
                                initial: t.pan.clamp(-1.0, 1.0),
                                coast: None,
                            }),
                    );
                }
                regions.push(
                    Region::new(m, Hit::Rect(at(g.mute)))
                        .cursor(Cursor::Pointer)
                        .press(Color::from_rgba8(0, 0, 0, 60)),
                );
                regions.push(
                    Region::new(so, Hit::Rect(at(g.solo)))
                        .cursor(Cursor::Pointer)
                        .press(Color::from_rgba8(0, 0, 0, 60)),
                );
            }
            let base = self.offset(c * CHUNK);
            let key = CHROME + LayerKey::try_from(c).unwrap_or(0);
            out.push((
                key,
                Layer::new(arc(s))
                    .z(1)
                    .regions(regions)
                    .animated(follow(axis_prop, base)),
            ));
        }
        // Per track: knob, pan bar, meter, lit buttons.
        let knob_c = mix(p.ink, p.surface, 0.1);
        let meter_c = Color::from_rgb8(0x4c, 0xd9, 0x64);
        let lit_mute = Color::from_rgb8(0xe0, 0x5a, 0x4a);
        let lit_solo = Color::from_rgb8(0xe8, 0xc0, 0x3a);
        for (i, t) in tracks.iter().enumerate() {
            let key = LayerKey::try_from(i).unwrap_or(0);
            let base = self.offset(i);
            let (Some(vol), Some(pan), Some(peak)) = (
                name(&names::volume(t.index)),
                name(&names::pan(t.index)),
                name(&names::peak(t.index)),
            ) else {
                continue;
            };
            let mut knob = Scene::new();
            let mut panbar = Scene::new();
            let mut meter = Scene::new();
            let (knob_anim, pan_anim, meter_anim) = match shape {
                Shape::Strips => {
                    // The knob's centre at the layer's origin, moved down the
                    // track by the value; the layer starts at the fader's foot.
                    rect(
                        &mut knob,
                        Rect::new(g.fader.x0 - 10.0, -6.0, g.fader.x1 + 10.0, 6.0),
                        knob_c,
                    );
                    rect(
                        &mut knob,
                        Rect::new(g.fader.x0 - 10.0, -0.5, g.fader.x1 + 10.0, 0.5),
                        p.background,
                    );
                    let mid = g.pan.center().x;
                    rect(
                        &mut panbar,
                        Rect::new(0.0, g.pan.y0 + 2.0, 1.0, g.pan.y1 - 2.0),
                        p.accent,
                    );
                    rect(
                        &mut meter,
                        Rect::new(g.meter.x0, -g.meter.height(), g.meter.x1, 0.0),
                        meter_c,
                    );
                    (
                        Animation::linear(
                            Property::Y,
                            Input::Value(vol),
                            Linear::new(-g.fader.height(), g.fader.y1),
                        ),
                        (
                            mid,
                            Animation::linear(
                                Property::ScaleX,
                                Input::Value(pan),
                                Linear::new(g.pan.width() / 2.0, 0.0),
                            ),
                        ),
                        (
                            g.meter.y1,
                            Animation::linear(
                                Property::ScaleY,
                                Input::Value(peak),
                                Linear::new(1.0, 0.0).clamp(0.0, 1.0),
                            ),
                        ),
                    )
                }
                Shape::Rows => {
                    rect(
                        &mut knob,
                        Rect::new(-5.0, g.fader.y0 - 6.0, 5.0, g.fader.y1 + 6.0),
                        knob_c,
                    );
                    let mid = g.pan.center().x;
                    rect(
                        &mut panbar,
                        Rect::new(0.0, g.pan.y0 + 2.0, 1.0, g.pan.y1 - 2.0),
                        p.accent,
                    );
                    rect(
                        &mut meter,
                        Rect::new(0.0, g.meter.y0, g.meter.width(), g.meter.y1),
                        meter_c,
                    );
                    (
                        Animation::linear(
                            Property::X,
                            Input::Value(vol),
                            Linear::new(g.fader.width(), g.fader.x0),
                        ),
                        (
                            mid,
                            Animation::linear(
                                Property::ScaleX,
                                Input::Value(pan),
                                Linear::new(g.pan.width() / 2.0, 0.0),
                            ),
                        ),
                        (
                            g.meter.x0,
                            Animation::linear(
                                Property::ScaleX,
                                Input::Value(peak),
                                Linear::new(1.0, 0.0).clamp(0.0, 1.0),
                            ),
                        ),
                    )
                }
            };
            let scroll_anim = |b: f64| follow(axis_prop, b);
            // Each layer: its scroll on the tracks' axis, its own motion on
            // the other (or a scale).
            let mut knob_l = Layer::new(arc(knob)).z(5);
            let mut anims = scroll_anim(base);
            anims.push(knob_anim);
            knob_l = knob_l.animated(anims);
            out.push((KNOB + key, knob_l));

            // A static place in the layer's transform; the scroll on the
            // tracks' axis and the layer's own motion as animations (at
            // most one each property).
            let (pan_x, pan_a) = pan_anim;
            let mut anims = scroll_anim(base);
            anims.push(pan_a);
            let pan_at = match shape {
                Shape::Strips => Affine::translate((pan_x, 0.0)),
                Shape::Rows => Affine::translate((pan_x, 0.0)),
            };
            out.push((
                PAN + key,
                Layer::new(arc(panbar)).at(pan_at).z(4).animated(anims),
            ));

            let (meter_at, meter_a) = meter_anim;
            let mut anims = scroll_anim(base);
            anims.push(meter_a);
            let meter_place = match shape {
                Shape::Strips => Affine::translate((0.0, meter_at)),
                Shape::Rows => Affine::translate((meter_at, 0.0)),
            };
            out.push((
                METER + key,
                Layer::new(arc(meter)).at(meter_place).z(3).animated(anims),
            ));

            // Mute and solo, lit; the fader's dB as text.
            let mut lit = Scene::new();
            let (ox, oy) = (0.0, 0.0);
            let at = |r: Rect| Rect::new(r.x0 + ox, r.y0 + oy, r.x1 + ox, r.y1 + oy);
            if t.mute {
                rect(&mut lit, at(g.mute), lit_mute);
                text(
                    &mut lit,
                    "M",
                    10.0,
                    (at(g.mute).x0 + 8.0, at(g.mute).y1 - 5.0),
                    14.0,
                    Color::BLACK,
                );
            }
            if t.solo {
                rect(&mut lit, at(g.solo), lit_solo);
                text(
                    &mut lit,
                    "S",
                    10.0,
                    (at(g.solo).x0 + 8.0, at(g.solo).y1 - 5.0),
                    14.0,
                    Color::BLACK,
                );
            }
            // Rounded first, so unity reads +0.0 and never -0.0.
            let db = (fader::db(t.volume) * 10.0).round() / 10.0 + 0.0;
            let label = if db.is_finite() {
                format!("{db:+.1}")
            } else {
                "-inf".to_owned()
            };
            let label_at = match shape {
                Shape::Strips => (g.fader.x0 - 18.0, g.fader.y0 - 4.0),
                Shape::Rows => (g.pan.x0 - 70.0, g.pan.y1),
            };
            text(&mut lit, &label, 9.0, label_at, 44.0, p.ink_dim);
            out.push((
                LIT + key,
                Layer::new(arc(lit)).z(6).animated(scroll_anim(base)),
            ));
        }
        // Play/pause and stop, in the pane's corner: the host's regions.
        let mut t = Scene::new();
        let corner = Rect::new(0.0, 0.0, 64.0, 26.0);
        rect(&mut t, corner, mix(p.surface, p.ink, 0.12));
        let mut tri = architect_workspace::paint::peniko::kurbo::BezPath::new();
        tri.move_to((12.0, 7.0));
        tri.line_to((22.0, 13.0));
        tri.line_to((12.0, 19.0));
        tri.close_path();
        t.fill(
            architect_workspace::paint::peniko::Fill::NonZero,
            Affine::IDENTITY,
            p.ink,
            None,
            &tri,
        );
        rect(&mut t, Rect::new(41.0, 8.0, 51.0, 18.0), p.ink);
        let regions = vec![
            Region::new(PLAY, Hit::Rect(Rect::new(0.0, 0.0, 32.0, 26.0)))
                .cursor(Cursor::Pointer)
                .hover(Color::from_rgba8(255, 255, 255, 24)),
            Region::new(STOP, Hit::Rect(Rect::new(32.0, 0.0, 64.0, 26.0)))
                .cursor(Cursor::Pointer)
                .hover(Color::from_rgba8(255, 255, 255, 24)),
        ];
        let (cw, ch) = self.size;
        let place = match shape {
            Shape::Strips => Affine::translate(((cw - 72.0).max(0.0), 6.0)),
            Shape::Rows => Affine::translate(((cw - 72.0).max(0.0), (ch - 32.0).max(0.0))),
        };
        out.push((
            TRANSPORT,
            Layer::new(arc(t)).at(place).z(100).regions(regions),
        ));
        let _ = MASTER;
        self.layers = out;
    }

    /// Which track and control a region id is.
    fn control(&self, id: u64) -> Option<(Track, u64)> {
        let n = usize::try_from(id / 4).ok()?;
        self.model
            .read(|m| m.tracks.get(n).cloned())
            .map(|t| (t, id % 4))
    }
}

impl PaintView for MixerView {
    fn input(&mut self, ctx: &ViewCtx, event: &ViewInput) -> bool {
        let before = self.scroll;
        let extent = match self.shape {
            Shape::Strips => f64::from(ctx.geometry.width),
            Shape::Rows => f64::from(ctx.geometry.height),
        };
        let total = self.model.read(|m| self.offset(m.tracks.len()));
        match event {
            ViewInput::Wheel { dx, dy, .. } => {
                let d = match self.shape {
                    Shape::Strips => f64::from(*dx) + f64::from(*dy),
                    Shape::Rows => f64::from(*dy),
                };
                self.scroll += d;
            }
            ViewInput::PointerDown { x, y, .. } => {
                self.dragging = Some((f64::from(*x), f64::from(*y)))
            }
            ViewInput::PointerMove { x, y, .. } => {
                if let Some((px, py)) = self.dragging {
                    let (x, y) = (f64::from(*x), f64::from(*y));
                    self.scroll -= match self.shape {
                        Shape::Strips => x - px,
                        Shape::Rows => y - py,
                    };
                    self.dragging = Some((x, y));
                }
            }
            ViewInput::PointerUp { .. } => self.dragging = None,
            ViewInput::Key {
                key, down: true, ..
            } if key == " " => {
                (self.sets)(Set::Transport(daw_view::Transport::Toggle));
            }
            _ => {}
        }
        self.scroll = self.scroll.clamp(0.0, (total - extent).max(0.0));
        let changed = (self.scroll - before).abs() > f64::EPSILON;
        if changed {
            let s = self.scroll;
            self.shared.update(|sh| sh.scroll = s);
        }
        changed
    }

    fn paint(&mut self, _: &ViewCtx, _: &mut Scene) {}

    fn region(&mut self, _: &ViewCtx, event: &RegionEvent) -> bool {
        self.shared.update(|s| s.heard += 1);
        match *event {
            RegionEvent::Value(id, _, v) => {
                self.held.insert(id, v);
                self.shared.update(|s| s.values_heard += 1);
            }
            RegionEvent::Release(id) => {
                if let (Some((t, k)), Some(v)) = (self.control(id), self.held.remove(&id)) {
                    let field = if k == 0 { Field::Volume } else { Field::Pan };
                    (self.sets)(Set::Track {
                        lane: t.lane,
                        field,
                        value: v,
                    });
                    self.shared.update(|s| s.sets += 1);
                }
            }
            RegionEvent::Press(PLAY) => (self.sets)(Set::Transport(daw_view::Transport::Toggle)),
            RegionEvent::Press(STOP) => (self.sets)(Set::Transport(daw_view::Transport::Stop)),
            RegionEvent::Press(id) => {
                if let Some((t, k)) = self.control(id)
                    && k >= 2
                {
                    let (field, on) = if k == 2 {
                        (Field::Mute, t.mute)
                    } else {
                        (Field::Solo, t.solo)
                    };
                    (self.sets)(Set::Track {
                        lane: t.lane,
                        field,
                        value: if on { 0.0 } else { 1.0 },
                    });
                    self.shared.update(|s| s.sets += 1);
                }
            }
            _ => {}
        }
        false
    }

    fn values(&mut self, _: &ViewCtx) -> Vec<(Name, f64)> {
        Self::scroll_name()
            .map(|n| vec![(n, self.scroll)])
            .unwrap_or_default()
    }

    fn layers(&mut self, ctx: &ViewCtx) -> Option<Vec<(LayerKey, Layer)>> {
        let p = Palette::of(&ctx.theme);
        self.size = (
            f64::from(ctx.geometry.width),
            f64::from(ctx.geometry.height),
        );
        let extent = match self.shape {
            Shape::Strips => f64::from(ctx.geometry.height),
            Shape::Rows => f64::from(ctx.geometry.width),
        };
        #[expect(clippy::cast_possible_truncation, reason = "a pane's size, for a key")]
        let key = (
            self.model.version(),
            p.dark,
            extent as i64,
            (self.size.0 + self.size.1 * 1.0e5) as i64,
        );
        if self.seen != Some(key) {
            self.record(&p, extent);
            self.seen = Some(key);
            let n = self.layers.len();
            self.shared.update(|s| {
                s.records += 1;
                s.layers = n;
            });
        }
        Some(self.layers.clone())
    }
}

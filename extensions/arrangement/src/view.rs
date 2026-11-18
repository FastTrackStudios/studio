//! The arrangement as layers.
//!
//! Content is recorded in **content space**: seconds × `R` pixels across
//! (`R` the zoom it was recorded at), rows of [`ROW_H`] down. The camera
//! (where the view starts, how far down it is scrolled, the zoom now) is
//! each layer's transform, so a pan or a scroll sends transforms and
//! nothing else. A zoom is an x-scale (`zoom / R`) while it moves; once
//! it has rested for [`SETTLE`] the view records again at the new zoom so
//! text and waveforms are sharp, which is the only time a zoom sends
//! content.
//!
//! | layer | what | moves with |
//! |---|---|---|
//! | [`BG`] | row stripes | the camera |
//! | [`GRID`] | bar and beat lines | the camera |
//! | [`BAND`]` + n` | the clips of rows `16n..16n+16`: body, waveform, name | the camera |
//! | [`SELECTION`] | the selected clips' outlines | the camera |
//! | [`RULER`] | bar numbers, markers, regions | the camera's x |
//! | [`HEADERS`] | the track names, colours, mute and solo | the camera's y |
//! | [`PLAYHEAD`] | one line | x from the app's playhead clock, run by the host |
//!
//! A band is sixteen rows so an edit re-sends a band, not the song, and
//! a scroll moves a few dozen layers, not hundreds.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyrender::recording::RenderCommand;
use architect_workspace::paint::anim::{Animation, Clock, Input, Linear, Property};
use architect_workspace::paint::peniko::kurbo::{Affine, BezPath, Rect};
use architect_workspace::paint::peniko::{Color, Fill};
use architect_workspace::paint::{Layer, LayerKey, PaintScene as _, PaintView, Scene};
use architect_workspace::{HostData, ViewCtx, ViewInput};
use paint_kit::{Palette, mix, rect, rgb, text};

use crate::model::{Model, Row, Wave};

pub const ROW_H: f64 = 52.0;
pub const HEADER_W: f64 = 180.0;
pub const RULER_H: f64 = 36.0;
/// Rows a band holds.
pub const BAND_ROWS: usize = 16;
/// How long a zoom rests before the view records at its new scale.
pub const SETTLE: Duration = Duration::from_millis(150);

pub const BG: LayerKey = 1;
pub const GRID: LayerKey = 2;
pub const SELECTION: LayerKey = 3;
pub const RULER: LayerKey = 4;
pub const RULER_BG: LayerKey = 5;
pub const HEADERS: LayerKey = 6;
pub const CORNER: LayerKey = 7;
pub const PLAYHEAD: LayerKey = 8;
pub const BAND: LayerKey = 1000;

/// Where the camera is: the view's left edge in seconds, how far down in
/// pixels, and pixels a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub left: f64,
    pub top: f64,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            zoom: 40.0,
        }
    }
}

/// What the view asks of whoever runs it (the engine, through its pane).
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Seek(f64),
    Toggle,
    Stop,
}

/// What the view says of itself (`STUDIO_ARRANGEMENT_STATE`), and what an
/// action asks of it (`STUDIO_ARRANGEMENT_VIEW`).
#[derive(Debug, Clone, Default)]
pub struct Shared {
    pub camera: Option<Camera>,
    /// A camera asked for, taken at the next frame.
    pub asked: Option<Camera>,
    pub selection: Vec<String>,
    /// Times the view recorded content (an open, an edit, a zoom at rest).
    pub records: u64,
    pub layers: usize,
}

/// What it recorded, at which zoom.
struct Recorded {
    at_zoom: f64,
    model: u64,
    dark: bool,
    height: f64,
    bg: Arc<Vec<RenderCommand>>,
    grid: Arc<Vec<RenderCommand>>,
    ruler: Arc<Vec<RenderCommand>>,
    ruler_bg: Arc<Vec<RenderCommand>>,
    headers: Arc<Vec<RenderCommand>>,
    corner: Arc<Vec<RenderCommand>>,
    playhead: Arc<Vec<RenderCommand>>,
    bands: Vec<Arc<Vec<RenderCommand>>>,
}

pub struct ArrangeView {
    model: HostData<Model>,
    shared: HostData<Shared>,
    intents: Box<dyn Fn(Intent)>,
    camera: Camera,
    zoomed_at: Option<Instant>,
    recorded: Option<Recorded>,
    selection: Vec<String>,
    selection_cmds: Arc<Vec<RenderCommand>>,
    selection_dirty: bool,
    panning: Option<(f64, f64)>,
    size: (f64, f64),
}

fn commands(scene: Scene) -> Arc<Vec<RenderCommand>> {
    Arc::new(scene.commands)
}

fn clip_color(row: &Row, c: Option<u32>, p: &Palette) -> Color {
    let base = c.or(row.color).map_or(p.accent, rgb);
    mix(base, p.surface, if p.dark { 0.35 } else { 0.25 })
}

/// A waveform's polygon over `width` pixels from `offset` seconds into
/// its source, `height` tall, centred on `mid`.
fn wave_path(
    w: &Wave,
    offset: f64,
    seconds: f64,
    x0: f64,
    width: f64,
    mid: f64,
    half: f64,
) -> Option<BezPath> {
    let cols = (width / 2.0).floor().clamp(1.0, 4000.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a column count"
    )]
    let n = cols as usize;
    let per_col = seconds / cols;
    let mut top = Vec::with_capacity(n);
    let mut bottom = Vec::with_capacity(n);
    for i in 0..n {
        #[expect(clippy::cast_precision_loss, reason = "a column")]
        let t0 = offset + i as f64 * per_col;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an index"
        )]
        let (a, b) = (
            (t0 * w.per_second).floor().max(0.0) as usize,
            ((t0 + per_col) * w.per_second).ceil().max(1.0) as usize,
        );
        let (mut lo, mut hi) = (0.0_f32, 0.0_f32);
        for &(l, h) in w.pairs.get(a..b.min(w.pairs.len())).unwrap_or(&[]) {
            lo = lo.min(l);
            hi = hi.max(h);
        }
        #[expect(clippy::cast_precision_loss, reason = "a column")]
        let x = x0 + (i as f64 + 0.5) * width / cols;
        top.push((x, mid - f64::from(hi) * half));
        bottom.push((x, mid - f64::from(lo) * half));
    }
    let mut path = BezPath::new();
    let first = *top.first()?;
    path.move_to(first);
    for p in top.iter().skip(1) {
        path.line_to(*p);
    }
    for p in bottom.iter().rev() {
        path.line_to(*p);
    }
    path.close_path();
    Some(path)
}

/// The clips of `rows` (the rows of one band, the first at `first`), at
/// `r` pixels a second.
fn record_band(
    m: &Model,
    first: usize,
    rows: &[Row],
    r: f64,
    p: &Palette,
) -> Arc<Vec<RenderCommand>> {
    let mut s = Scene::new();
    let label = if p.dark {
        Color::from_rgba8(0, 0, 0, 90)
    } else {
        Color::from_rgba8(255, 255, 255, 120)
    };
    for (i, row) in rows.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "a row")]
        let y = (first + i) as f64 * ROW_H;
        for c in &row.clips {
            let (x0, x1) = (c.start * r, c.end.max(c.start) * r);
            let w = (x1 - x0).max(1.0);
            let body = Rect::new(x0, y + 1.0, x0 + w, y + ROW_H - 1.0);
            let fill = clip_color(row, c.color, p);
            let fill = if c.mute {
                mix(fill, p.background, 0.6)
            } else {
                fill
            };
            rect(&mut s, body, fill);
            // Its name strip, then its waveform below.
            let strip = 13.0;
            rect(
                &mut s,
                Rect::new(x0, y + 1.0, x0 + w, y + 1.0 + strip),
                label,
            );
            if let Some(wave) = c.peaks.as_ref().and_then(|h| m.waves.get(h))
                && let Some(path) = wave_path(
                    wave,
                    c.offset,
                    c.end - c.start,
                    x0,
                    w,
                    y + strip + (ROW_H - strip) / 2.0,
                    (ROW_H - strip) / 2.0 - 3.0,
                )
            {
                let ink = mix(row.color.map_or(p.ink, rgb), p.ink, 0.55);
                s.fill(Fill::NonZero, Affine::IDENTITY, ink, None, &path);
            }
            // The edges, a pixel each, so clips that touch read as two.
            rect(
                &mut s,
                Rect::new(x0, y + 1.0, x0 + 1.0, y + ROW_H - 1.0),
                mix(fill, p.ink, 0.3),
            );
            #[expect(clippy::cast_possible_truncation, reason = "a width")]
            let room = (w - 6.0) as f32;
            let name = if c.name.is_empty() {
                row.name.as_str()
            } else {
                c.name.as_str()
            };
            text(&mut s, name, 10.0, (x0 + 4.0, y + 11.5), room, p.ink);
        }
    }
    commands(s)
}

fn record_grid(
    m: &Model,
    r: f64,
    height: f64,
    p: &Palette,
) -> (Arc<Vec<RenderCommand>>, Arc<Vec<RenderCommand>>) {
    let until = m.duration.max(60.0) + 30.0;
    let lines = daw_view::grid::lines(&m.tempo, until);
    let beat_px = lines.get(1).map_or(r, |l| l.seconds * r);
    let mut g = Scene::new();
    let mut ruler = Scene::new();
    let bar_ink = mix(p.background, p.ink, 0.18);
    let beat_ink = mix(p.background, p.ink, 0.08);
    // Bar numbers every bar, or every few when bars are narrow.
    let bars_per_label = {
        let bar_px = beat_px * f64::from(m.tempo.meter.0.max(1));
        let mut n = 1_u32;
        while bar_px * f64::from(n) < 36.0 && n < 1024 {
            n *= 2;
        }
        n
    };
    for l in &lines {
        let x = l.seconds * r;
        if l.beat == 1 {
            rect(&mut g, Rect::new(x, 0.0, x + 1.0, height), bar_ink);
            rect(
                &mut ruler,
                Rect::new(x, RULER_H - 12.0, x + 1.0, RULER_H),
                p.ink_dim,
            );
            if (l.bar - 1) % bars_per_label == 0 {
                text(
                    &mut ruler,
                    &l.bar.to_string(),
                    10.0,
                    (x + 3.0, RULER_H - 3.0),
                    40.0,
                    p.ink,
                );
            }
        } else if beat_px >= 8.0 {
            rect(&mut g, Rect::new(x, 0.0, x + 1.0, height), beat_ink);
            rect(
                &mut ruler,
                Rect::new(x, RULER_H - 5.0, x + 1.0, RULER_H),
                p.ink_dim,
            );
        }
    }
    // Regions as bands across the top, markers as flags.
    for mk in &m.marks {
        let x = mk.at * r;
        let c = mk.color.map_or(p.accent, rgb);
        match mk.end {
            Some(end) => {
                let x1 = end * r;
                rect(
                    &mut ruler,
                    Rect::new(x, 2.0, x1.max(x + 1.0), 15.0),
                    mix(c, p.background, 0.45),
                );
                #[expect(clippy::cast_possible_truncation, reason = "a width")]
                text(
                    &mut ruler,
                    &mk.name,
                    10.0,
                    (x + 3.0, 12.5),
                    (x1 - x - 5.0) as f32,
                    p.ink,
                );
            }
            None => {
                rect(&mut ruler, Rect::new(x, 2.0, x + 2.0, RULER_H), c);
                text(&mut ruler, &mk.name, 10.0, (x + 4.0, 12.5), 120.0, p.ink);
            }
        }
    }
    (commands(g), commands(ruler))
}

fn record_headers(m: &Model, p: &Palette) -> Arc<Vec<RenderCommand>> {
    let mut s = Scene::new();
    for (i, row) in m.rows.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "a row")]
        let y = i as f64 * ROW_H;
        let shade = if i % 2 == 0 {
            p.surface
        } else {
            mix(p.surface, p.ink, 0.03)
        };
        rect(&mut s, Rect::new(0.0, y, HEADER_W, y + ROW_H), shade);
        rect(
            &mut s,
            Rect::new(0.0, y + ROW_H - 1.0, HEADER_W, y + ROW_H),
            p.border,
        );
        let indent = f64::from(row.depth.min(6)) * 10.0;
        rect(
            &mut s,
            Rect::new(indent, y, indent + 4.0, y + ROW_H - 1.0),
            row.color.map_or(p.border, rgb),
        );
        #[expect(clippy::cast_possible_truncation, reason = "a width")]
        let room = (HEADER_W - indent - 48.0) as f32;
        text(
            &mut s,
            &row.name,
            12.0,
            (indent + 12.0, y + 20.0),
            room,
            p.ink,
        );
        if !row.kind.is_empty() && row.kind != daw_view::TRACK_KIND {
            text(
                &mut s,
                &row.kind,
                9.0,
                (indent + 12.0, y + 36.0),
                room,
                p.ink_dim,
            );
        }
        // Mute and solo, lit as the mixer has them.
        for (k, (on, lit, label)) in [
            (row.mute, Color::from_rgb8(0xe0, 0x5a, 0x4a), "M"),
            (row.solo, Color::from_rgb8(0xe8, 0xc0, 0x3a), "S"),
        ]
        .into_iter()
        .enumerate()
        {
            #[expect(clippy::cast_precision_loss, reason = "two buttons")]
            let x = HEADER_W - 40.0 + k as f64 * 18.0;
            let b = Rect::new(x, y + 8.0, x + 15.0, y + 23.0);
            rect(&mut s, b, if on { lit } else { mix(p.surface, p.ink, 0.1) });
            text(
                &mut s,
                label,
                9.0,
                (x + 4.0, y + 19.0),
                12.0,
                if on { Color::BLACK } else { p.ink_dim },
            );
        }
    }
    rect(
        &mut s,
        Rect::new(
            HEADER_W - 1.0,
            0.0,
            HEADER_W,
            m.rows.len().max(1) as f64 * ROW_H,
        ),
        p.border,
    );
    commands(s)
}

impl ArrangeView {
    #[must_use]
    pub fn new(
        model: HostData<Model>,
        shared: HostData<Shared>,
        intents: Box<dyn Fn(Intent)>,
    ) -> Self {
        Self {
            model,
            shared,
            intents,
            camera: Camera::default(),
            zoomed_at: None,
            recorded: None,
            selection: Vec::new(),
            selection_cmds: Arc::new(Vec::new()),
            selection_dirty: false,
            panning: None,
            size: (0.0, 0.0),
        }
    }

    fn content_height(&self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "rows")]
        let rows = self.model.read(|m| m.rows.len()) as f64;
        rows * ROW_H
    }

    fn clamp(&mut self) {
        let lanes = (self.size.1 - RULER_H).max(0.0);
        let max_top = (self.content_height() - lanes).max(0.0);
        let duration = self.model.read(|m| m.duration).max(10.0);
        let c = &mut self.camera;
        c.zoom = c.zoom.clamp(0.5, 2000.0);
        c.top = c.top.clamp(0.0, max_top);
        c.left = c.left.clamp(-1.0, duration);
    }

    fn second_at(&self, x: f64) -> f64 {
        self.camera.left + (x - HEADER_W) / self.camera.zoom
    }

    /// The clip under `(x, y)`, if any.
    fn clip_at(&self, x: f64, y: f64) -> Option<String> {
        let row = ((y - RULER_H + self.camera.top) / ROW_H).floor();
        if row < 0.0 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a row"
        )]
        let row = row as usize;
        let t = self.second_at(x);
        self.model.read(|m| {
            m.rows
                .get(row)?
                .clips
                .iter()
                .find(|c| t >= c.start && t < c.end)
                .map(|c| c.id.clone())
        })
    }

    fn record(&mut self, p: &Palette) {
        let r = self.camera.zoom;
        let height = self.content_height().max(ROW_H);
        let (version, rec) = self.model.read(|m| {
            let mut bg = Scene::new();
            for i in 0..m.rows.len() {
                #[expect(clippy::cast_precision_loss, reason = "a row")]
                let y = i as f64 * ROW_H;
                let shade = if i % 2 == 0 {
                    p.background
                } else {
                    mix(p.background, p.ink, 0.025)
                };
                rect(&mut bg, Rect::new(-1.0e5, y, 1.0e6, y + ROW_H), shade);
                rect(
                    &mut bg,
                    Rect::new(-1.0e5, y + ROW_H - 1.0, 1.0e6, y + ROW_H),
                    mix(p.background, p.ink, 0.06),
                );
            }
            let (grid, ruler) = record_grid(m, r, height, p);
            let bands = m
                .rows
                .chunks(BAND_ROWS)
                .enumerate()
                .map(|(b, rows)| record_band(m, b * BAND_ROWS, rows, r, p))
                .collect();
            let mut ruler_bg = Scene::new();
            rect(
                &mut ruler_bg,
                Rect::new(0.0, 0.0, 1.0e5, RULER_H),
                p.surface,
            );
            rect(
                &mut ruler_bg,
                Rect::new(0.0, RULER_H - 1.0, 1.0e5, RULER_H),
                p.border,
            );
            let mut corner = Scene::new();
            rect(
                &mut corner,
                Rect::new(0.0, 0.0, HEADER_W, RULER_H),
                p.surface,
            );
            rect(
                &mut corner,
                Rect::new(0.0, RULER_H - 1.0, HEADER_W, RULER_H),
                p.border,
            );
            rect(
                &mut corner,
                Rect::new(HEADER_W - 1.0, 0.0, HEADER_W, RULER_H),
                p.border,
            );
            #[expect(clippy::cast_possible_truncation, reason = "a width")]
            text(
                &mut corner,
                &m.name,
                12.0,
                (10.0, 22.0),
                (HEADER_W - 20.0) as f32,
                p.ink,
            );
            let mut line = Scene::new();
            let red = Color::from_rgb8(0xff, 0x45, 0x3a);
            rect(&mut line, Rect::new(-0.75, 0.0, 0.75, 1.0e4), red);
            let mut head = BezPath::new();
            head.move_to((-6.0, 0.0));
            head.line_to((6.0, 0.0));
            head.line_to((0.0, 8.0));
            head.close_path();
            line.fill(Fill::NonZero, Affine::IDENTITY, red, None, &head);
            (
                self.model.version(),
                Recorded {
                    at_zoom: r,
                    model: 0,
                    dark: p.dark,
                    height,
                    bg: commands(bg),
                    grid,
                    ruler,
                    ruler_bg: commands(ruler_bg),
                    headers: record_headers(m, p),
                    corner: commands(corner),
                    playhead: commands(line),
                    bands,
                },
            )
        });
        self.recorded = Some(Recorded {
            model: version,
            ..rec
        });
        self.selection_dirty = true;
        self.shared.update(|s| s.records += 1);
    }

    fn record_selection(&mut self, p: &Palette) {
        let Some(rec) = &self.recorded else { return };
        let r = rec.at_zoom;
        let mut s = Scene::new();
        self.model.read(|m| {
            for (i, row) in m.rows.iter().enumerate() {
                #[expect(clippy::cast_precision_loss, reason = "a row")]
                let y = i as f64 * ROW_H;
                for c in row.clips.iter().filter(|c| self.selection.contains(&c.id)) {
                    let (x0, x1) = (c.start * r, c.end * r);
                    let a = p.accent;
                    rect(&mut s, Rect::new(x0, y + 1.0, x1, y + 3.0), a);
                    rect(
                        &mut s,
                        Rect::new(x0, y + ROW_H - 3.0, x1, y + ROW_H - 1.0),
                        a,
                    );
                    rect(&mut s, Rect::new(x0, y + 1.0, x0 + 2.0, y + ROW_H - 1.0), a);
                    rect(&mut s, Rect::new(x1 - 2.0, y + 1.0, x1, y + ROW_H - 1.0), a);
                    rect(
                        &mut s,
                        Rect::new(x0, y + 1.0, x1, y + ROW_H - 1.0),
                        Color::from_rgba8(255, 255, 255, 28),
                    );
                }
            }
        });
        self.selection_cmds = commands(s);
        self.selection_dirty = false;
        let sel = self.selection.clone();
        self.shared.update(|s| s.selection = sel);
    }
}

impl PaintView for ArrangeView {
    fn input(&mut self, _ctx: &ViewCtx, event: &ViewInput) -> bool {
        let before = self.camera;
        match event {
            ViewInput::Wheel {
                dx, dy, x, mods, ..
            } => {
                let (dx, dy, x) = (f64::from(*dx), f64::from(*dy), f64::from(*x));
                if mods.iter().any(|m| *m == "Mod" || *m == "Ctrl") {
                    let at = self.second_at(x.max(HEADER_W));
                    self.camera.zoom *= (-dy * 0.002).exp();
                    self.clamp();
                    self.camera.left = at - (x.max(HEADER_W) - HEADER_W) / self.camera.zoom;
                    self.zoomed_at = Some(Instant::now());
                } else if mods.contains(&"Shift") {
                    self.camera.left += dy / self.camera.zoom;
                } else {
                    self.camera.top += dy;
                    self.camera.left += dx / self.camera.zoom;
                }
            }
            ViewInput::PointerDown { x, y, button, mods } => {
                let (x, y) = (f64::from(*x), f64::from(*y));
                if x >= HEADER_W && y < RULER_H {
                    (self.intents)(Intent::Seek(self.second_at(x).max(0.0)));
                    return false;
                }
                if x >= HEADER_W && *button == architect_workspace::Button::Primary {
                    match self.clip_at(x, y) {
                        Some(id) if mods.contains(&"Shift") || mods.contains(&"Mod") => {
                            if let Some(at) = self.selection.iter().position(|s| *s == id) {
                                self.selection.remove(at);
                            } else {
                                self.selection.push(id);
                            }
                        }
                        Some(id) => self.selection = vec![id],
                        None => {
                            self.selection.clear();
                            self.panning = Some((x, y));
                        }
                    }
                    self.selection_dirty = true;
                    return true;
                }
                self.panning = Some((x, y));
            }
            ViewInput::PointerMove { x, y, .. } => {
                if let Some((px, py)) = self.panning {
                    let (x, y) = (f64::from(*x), f64::from(*y));
                    self.camera.left -= (x - px) / self.camera.zoom;
                    self.camera.top -= y - py;
                    self.panning = Some((x, y));
                }
            }
            ViewInput::PointerUp { .. } => self.panning = None,
            ViewInput::Key {
                key, down: true, ..
            } => match key.as_str() {
                " " => (self.intents)(Intent::Toggle),
                "Home" | "Enter" => (self.intents)(Intent::Seek(0.0)),
                "Escape" => (self.intents)(Intent::Stop),
                "=" | "+" => {
                    self.camera.zoom *= 1.25;
                    self.zoomed_at = Some(Instant::now());
                }
                "-" => {
                    self.camera.zoom /= 1.25;
                    self.zoomed_at = Some(Instant::now());
                }
                _ => {}
            },
            _ => {}
        }
        self.clamp();
        self.camera != before || self.selection_dirty
    }

    fn paint(&mut self, _: &ViewCtx, _: &mut Scene) {}

    fn animating(&self, _: &ViewCtx) -> bool {
        // A zoom waiting to rest asks for frames until it is recorded.
        self.zoomed_at.is_some()
    }

    #[expect(clippy::too_many_lines, reason = "the layer table, one entry each")]
    fn layers(&mut self, ctx: &ViewCtx) -> Option<Vec<(LayerKey, Layer)>> {
        let p = Palette::of(&ctx.theme);
        self.size = (
            f64::from(ctx.geometry.width),
            f64::from(ctx.geometry.height),
        );
        if let Some(asked) = self.shared.update(|s| s.asked.take()) {
            if (asked.zoom - self.camera.zoom).abs() > 1e-9 {
                self.zoomed_at = Some(
                    Instant::now()
                        .checked_sub(SETTLE)
                        .unwrap_or_else(Instant::now),
                );
            }
            self.camera = asked;
            self.clamp();
        }
        let stale = self.recorded.as_ref().is_none_or(|r| {
            r.model != self.model.version()
                || r.dark != p.dark
                || (r.height - self.content_height().max(ROW_H)).abs() > 0.5
        });
        let rested = self.zoomed_at.is_some_and(|t| t.elapsed() >= SETTLE);
        if stale || rested {
            self.zoomed_at = None;
            self.record(&p);
        }
        if self.selection_dirty {
            self.record_selection(&p);
        }
        let rec = self.recorded.as_ref()?;
        let (w, h) = self.size;
        let c = self.camera;
        let k = c.zoom / rec.at_zoom;
        let lanes = Rect::new(HEADER_W, RULER_H, w.max(HEADER_W), h.max(RULER_H));
        let camera = Affine::translate((HEADER_W - c.left * c.zoom, RULER_H - c.top))
            * Affine::scale_non_uniform(k, 1.0);
        let along = Affine::translate((HEADER_W - c.left * c.zoom, 0.0))
            * Affine::scale_non_uniform(k, 1.0);
        let down = Affine::translate((0.0, RULER_H - c.top));
        let mut out = vec![
            (
                BG,
                Layer::new(Arc::clone(&rec.bg)).at(camera).clipped(lanes),
            ),
            (
                GRID,
                Layer::new(Arc::clone(&rec.grid))
                    .at(camera)
                    .clipped(lanes)
                    .z(1),
            ),
            (
                SELECTION,
                Layer::new(Arc::clone(&self.selection_cmds))
                    .at(camera)
                    .clipped(lanes)
                    .z(3),
            ),
            (RULER_BG, Layer::new(Arc::clone(&rec.ruler_bg)).z(10)),
            (
                RULER,
                Layer::new(Arc::clone(&rec.ruler))
                    .at(along)
                    .clipped(Rect::new(HEADER_W, 0.0, w.max(HEADER_W), RULER_H))
                    .z(11),
            ),
            (
                HEADERS,
                Layer::new(Arc::clone(&rec.headers))
                    .at(down)
                    .clipped(Rect::new(0.0, RULER_H, HEADER_W, h.max(RULER_H)))
                    .z(12),
            ),
            (CORNER, Layer::new(Arc::clone(&rec.corner)).z(13)),
            (
                PLAYHEAD,
                Layer::new(Arc::clone(&rec.playhead))
                    .at(Affine::translate((HEADER_W - c.left * c.zoom, 0.0)))
                    .clipped(Rect::new(HEADER_W, 0.0, w.max(HEADER_W), h))
                    .z(20)
                    // The host runs it: seconds of the app's playhead
                    // times pixels a second. A zoom sends this again.
                    .animated(vec![Animation::linear(
                        Property::X,
                        Input::clock(Clock::Playhead),
                        Linear::new(c.zoom, 0.0),
                    )]),
            ),
        ];
        for (b, band) in rec.bands.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "a band")]
            let (y0, y1) = (
                (b * BAND_ROWS) as f64 * ROW_H,
                ((b + 1) * BAND_ROWS) as f64 * ROW_H,
            );
            // Off screen, a band is not drawn (it keeps its fragment).
            let shown = y1 - c.top + RULER_H > RULER_H && y0 - c.top + RULER_H < h;
            let key = BAND + LayerKey::try_from(b).unwrap_or(0);
            let mut layer = Layer::new(Arc::clone(band)).at(camera).clipped(lanes).z(2);
            layer.visible = shown;
            out.push((key, layer));
        }
        self.shared.update(|s| {
            s.camera = Some(c);
            s.layers = out.len();
        });
        Some(out)
    }
}

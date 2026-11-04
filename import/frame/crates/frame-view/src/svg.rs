//! Frame's paint, as SVG: a [`Canvas`] that keeps each call as an SVG
//! element, so a surface draws wherever SVG does (Blitz's inline `<svg>`,
//! a browser, a file).
//!
//! Every path carries its own transform; clips and opacity layers are
//! groups. A gradient is drawn in its first stop's colour, an image not at
//! all: the surfaces this kind shows are flat.

use std::fmt::Write as _;

use frame::embed::Canvas;
use kurbo::{Affine, BezPath, Stroke};
use peniko::{Brush, Color, Fill};

/// One drawn path.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Its outline (`d`).
    pub d: String,
    /// Its `matrix(…)`.
    pub transform: String,
    /// `#rrggbb` and its opacity, or none.
    pub fill: Option<(String, f32)>,
    /// `nonzero` or `evenodd`.
    pub fill_rule: &'static str,
    /// `#rrggbb`, its opacity and its width, or none.
    pub stroke: Option<(String, f32, f64)>,
}

/// A clip, kept once in the SVG's `<defs>` and named by its index.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub d: String,
    pub transform: String,
}

/// What a surface painted.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Path(Path),
    /// Its children, clipped to `clip` (an index into [`Svg::clips`]) and
    /// composited at `opacity`.
    Group {
        clip: Option<usize>,
        opacity: Option<f32>,
        children: Vec<Node>,
    },
}

/// A painted surface, `width × height` px.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Svg {
    pub width: f64,
    pub height: f64,
    pub clips: Vec<Clip>,
    pub nodes: Vec<Node>,
}

impl Svg {
    /// The id of clip `i`, as the SVG names it.
    #[must_use]
    pub fn clip_id(i: usize) -> String {
        format!("frame-clip-{i}")
    }

    /// The SVG document.
    #[must_use]
    pub fn markup(&self) -> String {
        let mut out = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
            w = self.width,
            h = self.height
        );
        if !self.clips.is_empty() {
            out.push_str("<defs>");
            for (i, c) in self.clips.iter().enumerate() {
                let _ = write!(
                    out,
                    r#"<clipPath id="{}"><path d="{}" transform="{}"/></clipPath>"#,
                    Self::clip_id(i),
                    c.d,
                    c.transform
                );
            }
            out.push_str("</defs>");
        }
        for n in &self.nodes {
            node_markup(&mut out, n);
        }
        out.push_str("</svg>");
        out
    }
}

fn node_markup(out: &mut String, node: &Node) {
    match node {
        Node::Path(p) => {
            let _ = write!(out, r#"<path d="{}" transform="{}""#, p.d, p.transform);
            match &p.fill {
                Some((color, opacity)) => {
                    let _ = write!(
                        out,
                        r#" fill="{color}" fill-opacity="{opacity}" fill-rule="{}""#,
                        p.fill_rule
                    );
                }
                None => out.push_str(r#" fill="none""#),
            }
            if let Some((color, opacity, width)) = &p.stroke {
                let _ = write!(
                    out,
                    r#" stroke="{color}" stroke-opacity="{opacity}" stroke-width="{width}""#
                );
            }
            out.push_str("/>");
        }
        Node::Group {
            clip,
            opacity,
            children,
        } => {
            out.push_str("<g");
            if let Some(c) = clip {
                let _ = write!(out, r#" clip-path="url(#{})""#, Svg::clip_id(*c));
            }
            if let Some(o) = opacity {
                let _ = write!(out, r#" opacity="{o}""#);
            }
            out.push('>');
            for n in children {
                node_markup(out, n);
            }
            out.push_str("</g>");
        }
    }
}

/// A [`Canvas`] that records into an [`Svg`].
#[derive(Debug)]
pub struct SvgCanvas {
    clips: Vec<Clip>,
    /// The open groups, innermost last; the first is the root.
    stack: Vec<(Option<usize>, Option<f32>, Vec<Node>)>,
}

impl SvgCanvas {
    #[must_use]
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            stack: vec![(None, None, Vec::new())],
        }
    }

    /// What was painted, `width × height` px (groups left open closed).
    #[must_use]
    pub fn finish(mut self, width: f64, height: f64) -> Svg {
        while self.stack.len() > 1 {
            self.pop();
        }
        let nodes = self.stack.pop().map(|(_, _, n)| n).unwrap_or_default();
        Svg {
            width,
            height,
            clips: self.clips,
            nodes,
        }
    }

    fn push(&mut self, node: Node) {
        if let Some((_, _, children)) = self.stack.last_mut() {
            children.push(node);
        }
    }

    fn clip(&mut self, transform: Affine, shape: &BezPath) -> usize {
        self.clips.push(Clip {
            d: shape.to_svg(),
            transform: matrix(transform),
        });
        self.clips.len() - 1
    }
}

impl Default for SvgCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas for SvgCanvas {
    fn fill(
        &mut self,
        transform: Affine,
        rule: Fill,
        brush: &Brush,
        _brush_transform: Option<Affine>,
        shape: &BezPath,
    ) {
        let Some(color) = solid(brush) else {
            return;
        };
        self.push(Node::Path(Path {
            d: shape.to_svg(),
            transform: matrix(transform),
            fill: Some(hex(color)),
            fill_rule: match rule {
                Fill::NonZero => "nonzero",
                Fill::EvenOdd => "evenodd",
            },
            stroke: None,
        }));
    }

    fn stroke(&mut self, transform: Affine, style: &Stroke, brush: &Brush, shape: &BezPath) {
        let Some(color) = solid(brush) else {
            return;
        };
        let (color, opacity) = hex(color);
        self.push(Node::Path(Path {
            d: shape.to_svg(),
            transform: matrix(transform),
            fill: None,
            fill_rule: "nonzero",
            stroke: Some((color, opacity, style.width)),
        }));
    }

    fn push_clip(&mut self, transform: Affine, shape: &BezPath) {
        let clip = self.clip(transform, shape);
        self.stack.push((Some(clip), None, Vec::new()));
    }

    fn push_opacity(&mut self, transform: Affine, alpha: f32, shape: &BezPath) {
        let clip = self.clip(transform, shape);
        self.stack.push((Some(clip), Some(alpha), Vec::new()));
    }

    fn pop(&mut self) {
        if self.stack.len() < 2 {
            return;
        }
        if let Some((clip, opacity, children)) = self.stack.pop() {
            self.push(Node::Group {
                clip,
                opacity,
                children,
            });
        }
    }
}

fn solid(brush: &Brush) -> Option<Color> {
    match brush {
        Brush::Solid(c) => Some(*c),
        Brush::Gradient(g) => g.stops.first().map(|s| s.color.to_alpha_color()),
        Brush::Image(_) => None,
    }
}

/// `#rrggbb` and its alpha, which SVG takes apart (Blitz's SVG has no
/// `rgba()`).
fn hex(color: Color) -> (String, f32) {
    let c = color.to_rgba8();
    (
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        f32::from(c.a) / 255.0,
    )
}

fn matrix(t: Affine) -> String {
    let [a, b, c, d, e, f] = t.as_coeffs();
    format!("matrix({a} {b} {c} {d} {e} {f})")
}

//! The native projection of v1 node fields onto figcraft node fields, one property group at a
//! time: `put` writes a group's v1 fields onto a figcraft node, `get` reads them back, `native`
//! is what the group's figcraft side holds (for telling whether it was edited since).

use std::collections::BTreeMap;

use figcraft_color::{
    BlendMode, Color, ColorStop, Gradient, GradientKind, ImagePaint, ImageScaleMode, Paint,
    PaintKind,
};
use figcraft_doc::{
    self as fc, AutoLayout, Blur, BoolOp, ChildLayout, Constraints, CrossAlign, Effect, EffectKind,
    HConstraint, LayoutMode, LetterSpacing, LineHeight, MainAlign, MaskType, NodeKind, Shadow,
    Sizing, StrokeAlign, StrokeCap, StrokeJoin, StrokeProps, TextAlignH, TextCase, VConstraint,
    VectorNetwork,
};
use figcraft_geom::{Affine, BezPath, Point};
use frame::document::proto as v1;
use serde_json::{Value, json};
use uuid::Uuid;

/// What a projection needs besides the node: ids and image keys both ways.
#[derive(Debug, Default)]
pub(crate) struct Cx {
    /// v1 id → figcraft id, of the nodes that became figcraft components (instance targets).
    pub components: BTreeMap<Uuid, fc::NodeId>,
    /// figcraft id → v1 id.
    pub uuid_of: BTreeMap<fc::NodeId, Uuid>,
    /// Asset → image key.
    pub key_of: BTreeMap<v1::AssetId, String>,
    /// Image key → asset.
    pub asset_of: BTreeMap<String, v1::AssetId>,
}

impl Cx {
    fn key(&self, asset: &v1::AssetId) -> String {
        self.key_of
            .get(asset)
            .cloned()
            .unwrap_or_else(|| super::image_key(asset))
    }

    fn asset(&self, key: &str) -> v1::AssetId {
        self.asset_of.get(key).cloned().unwrap_or_else(|| {
            v1::AssetId(
                key.strip_prefix(super::KEY_PREFIX)
                    .unwrap_or(key)
                    .to_string(),
            )
        })
    }
}

/// A group of v1 fields projected together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum G {
    Name,
    Flags,
    Transform,
    Size,
    Opacity,
    Blend,
    Corners,
    Clip,
    Mask,
    Fills,
    Stroke,
    Effects,
    LayoutChild,
    Layout,
    Kind,
    Geometry,
    Source,
}

/// The groups in `put` order (the layout group's sizing goes over the child layout's), with their
/// shadow names and the v1 fields they cover.
pub(crate) const GROUPS: [(G, &str, &[&str]); 17] = [
    (G::Name, "name", &["name"]),
    (G::Flags, "flags", &["visible", "locked"]),
    (G::Transform, "transform", &["transform"]),
    (G::Size, "size", &["size"]),
    (G::Opacity, "opacity", &["opacity"]),
    (G::Blend, "blend", &["blend"]),
    (G::Corners, "corners", &["corner_radii"]),
    (G::Clip, "clip", &["clips_content"]),
    (G::Mask, "mask", &["mask"]),
    (G::Fills, "fills", &["fills"]),
    (G::Stroke, "stroke", &["stroke"]),
    (G::Effects, "effects", &["effects"]),
    (G::LayoutChild, "layout_child", &["layout_child"]),
    (G::Layout, "layout", &["layout"]),
    (G::Kind, "kind", &["kind"]),
    (
        G::Geometry,
        "geometry",
        &["fill_geometry", "stroke_geometry", "svg_snapshot"],
    ),
    (G::Source, "source", &["source"]),
];

/// Write group `g` of `v` onto `f`. `page`: `f` is a figcraft page.
pub(crate) fn put(g: G, v: &v1::Node, f: &mut fc::Node, cx: &Cx, page: bool) {
    match g {
        G::Name => f.name.clone_from(&v.name),
        G::Flags => {
            f.visible = v.visible;
            f.locked = v.locked;
        }
        G::Transform => f.transform = affine(v.transform),
        G::Size => {
            f.width = v.size.width;
            f.height = v.size.height;
        }
        G::Opacity => f.opacity = v.opacity as f32,
        G::Blend => f.blend = blend_to(v.blend),
        G::Corners => {
            let r = v.corner_radii;
            f.corners = fc::Corners {
                radii: [r.top_left, r.top_right, r.bottom_right, r.bottom_left],
                smoothing: 0.0,
            };
        }
        G::Clip => f.clip = v.clips_content,
        G::Mask => {
            f.is_mask = v.mask.is_some();
            f.mask_type = match v.mask {
                Some(v1::MaskKind::Vector) => MaskType::Vector,
                Some(v1::MaskKind::Luminance) => MaskType::Luminance,
                _ => MaskType::Alpha,
            };
        }
        G::Fills => f.fills = v.fills.iter().map(|p| paint_to(p, cx)).collect(),
        G::Stroke => {
            let s = &v.stroke;
            f.strokes = s.paints.iter().map(|p| paint_to(p, cx)).collect();
            f.stroke = StrokeProps {
                weight: s.weight,
                align: match s.align {
                    v1::StrokeAlign::Inside => StrokeAlign::Inside,
                    v1::StrokeAlign::Outside => StrokeAlign::Outside,
                    v1::StrokeAlign::Center => StrokeAlign::Center,
                },
                cap_start: cap_to(s.cap),
                cap_end: cap_to(s.cap),
                join: match s.join {
                    v1::StrokeJoin::Miter => StrokeJoin::Miter,
                    v1::StrokeJoin::Bevel => StrokeJoin::Bevel,
                    v1::StrokeJoin::Round => StrokeJoin::Round,
                },
                miter_limit: s.miter_limit,
                dashes: s.dash_pattern.clone(),
                ..StrokeProps::default()
            };
        }
        G::Effects => f.effects = v.effects.iter().map(effect_to).collect(),
        G::LayoutChild => {
            let l = &v.layout_child;
            f.child_layout = ChildLayout {
                absolute: l.absolute,
                min_w: l.min_width,
                max_w: l.max_width,
                min_h: l.min_height,
                max_h: l.max_height,
                ..ChildLayout::default()
            };
            f.constraints = Constraints {
                horizontal: match l.resize_x {
                    None | Some(v1::ResizeX::Left) => HConstraint::Left,
                    Some(v1::ResizeX::Right) => HConstraint::Right,
                    Some(v1::ResizeX::Center) => HConstraint::Center,
                    Some(v1::ResizeX::Stretch) => HConstraint::LeftRight,
                    Some(v1::ResizeX::Scale) => HConstraint::Scale,
                },
                vertical: match l.resize_y {
                    None | Some(v1::ResizeY::Top) => VConstraint::Top,
                    Some(v1::ResizeY::Bottom) => VConstraint::Bottom,
                    Some(v1::ResizeY::Center) => VConstraint::Center,
                    Some(v1::ResizeY::Stretch) => VConstraint::TopBottom,
                    Some(v1::ResizeY::Scale) => VConstraint::Scale,
                },
            };
        }
        G::Layout => {
            f.layout = v.layout.as_ref().map(|l| AutoLayout {
                mode: match l.direction {
                    v1::LayoutDirection::Horizontal => LayoutMode::Horizontal,
                    v1::LayoutDirection::Vertical => LayoutMode::Vertical,
                },
                wrap: l.wrap,
                gap: l.gap,
                padding: [
                    l.padding_top,
                    l.padding_right,
                    l.padding_bottom,
                    l.padding_left,
                ],
                main_align: match l.main_align.unwrap_or_default() {
                    v1::MainAxisAlign::Start => MainAlign::Min,
                    v1::MainAxisAlign::Center => MainAlign::Center,
                    v1::MainAxisAlign::End => MainAlign::Max,
                    v1::MainAxisAlign::SpaceBetween => MainAlign::SpaceBetween,
                },
                cross_align: match l.cross_align.unwrap_or_default() {
                    v1::CrossAxisAlign::Start => CrossAlign::Min,
                    v1::CrossAxisAlign::Center => CrossAlign::Center,
                    v1::CrossAxisAlign::End => CrossAlign::Max,
                    v1::CrossAxisAlign::Baseline => CrossAlign::Baseline,
                },
                ..AutoLayout::default()
            });
            if let Some(l) = &v.layout {
                let hug = |h| if h { Sizing::Hug } else { Sizing::Fixed };
                let (main, cross) = (hug(l.hug_main), hug(l.hug_cross));
                (f.child_layout.sizing_h, f.child_layout.sizing_v) = match l.direction {
                    v1::LayoutDirection::Horizontal => (main, cross),
                    v1::LayoutDirection::Vertical => (cross, main),
                };
            }
        }
        G::Kind => f.kind = kind_to(v, cx, page),
        G::Geometry | G::Source => {}
    }
}

/// Read group `g` of `f` into `v`.
pub(crate) fn get(g: G, f: &fc::Node, v: &mut v1::Node, cx: &Cx) {
    match g {
        G::Name => v.name.clone_from(&f.name),
        G::Flags => {
            v.visible = f.visible;
            v.locked = f.locked;
        }
        G::Transform => v.transform = transform(f.transform),
        G::Size => {
            v.size = v1::Size {
                width: f.width,
                height: f.height,
            }
        }
        G::Opacity => v.opacity = f64::from(f.opacity),
        G::Blend => v.blend = blend_from(f.blend),
        G::Corners => {
            let [top_left, top_right, bottom_right, bottom_left] = f.corners.radii;
            v.corner_radii = v1::CornerRadii {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            };
        }
        G::Clip => v.clips_content = f.clip,
        G::Mask => {
            v.mask = f.is_mask.then_some(match f.mask_type {
                MaskType::Alpha => v1::MaskKind::Alpha,
                MaskType::Vector => v1::MaskKind::Vector,
                MaskType::Luminance => v1::MaskKind::Luminance,
            });
        }
        G::Fills => v.fills = f.fills.iter().filter_map(|p| paint_from(p, cx)).collect(),
        G::Stroke => {
            let s = &f.stroke;
            v.stroke = v1::Stroke {
                paints: f.strokes.iter().filter_map(|p| paint_from(p, cx)).collect(),
                weight: s.weight,
                cap: cap_from(s.cap_start),
                join: match s.join {
                    StrokeJoin::Miter => v1::StrokeJoin::Miter,
                    StrokeJoin::Bevel => v1::StrokeJoin::Bevel,
                    StrokeJoin::Round => v1::StrokeJoin::Round,
                },
                align: match s.align {
                    StrokeAlign::Inside => v1::StrokeAlign::Inside,
                    StrokeAlign::Outside => v1::StrokeAlign::Outside,
                    StrokeAlign::Center => v1::StrokeAlign::Center,
                },
                miter_limit: s.miter_limit,
                dash_pattern: s.dashes.clone(),
                dash_offset: 0.0,
            };
        }
        G::Effects => v.effects = f.effects.iter().filter_map(effect_from).collect(),
        G::LayoutChild => {
            let l = &f.child_layout;
            v.layout_child = v1::LayoutChild {
                grow: None,
                align_self: None,
                absolute: l.absolute,
                min_width: l.min_w,
                min_height: l.min_h,
                max_width: l.max_w,
                max_height: l.max_h,
                resize_x: match f.constraints.horizontal {
                    HConstraint::Left => None,
                    HConstraint::Right => Some(v1::ResizeX::Right),
                    HConstraint::Center => Some(v1::ResizeX::Center),
                    HConstraint::LeftRight => Some(v1::ResizeX::Stretch),
                    HConstraint::Scale => Some(v1::ResizeX::Scale),
                },
                resize_y: match f.constraints.vertical {
                    VConstraint::Top => None,
                    VConstraint::Bottom => Some(v1::ResizeY::Bottom),
                    VConstraint::Center => Some(v1::ResizeY::Center),
                    VConstraint::TopBottom => Some(v1::ResizeY::Stretch),
                    VConstraint::Scale => Some(v1::ResizeY::Scale),
                },
            };
        }
        G::Layout => {
            v.layout = f.layout.as_ref().map(|l| {
                let direction = match l.mode {
                    LayoutMode::Vertical => v1::LayoutDirection::Vertical,
                    LayoutMode::Horizontal | LayoutMode::Grid => v1::LayoutDirection::Horizontal,
                };
                let (h, vv) = (
                    f.child_layout.sizing_h == Sizing::Hug,
                    f.child_layout.sizing_v == Sizing::Hug,
                );
                let (hug_main, hug_cross) = match direction {
                    v1::LayoutDirection::Horizontal => (h, vv),
                    v1::LayoutDirection::Vertical => (vv, h),
                };
                let [padding_top, padding_right, padding_bottom, padding_left] = l.padding;
                v1::AutoLayout {
                    direction,
                    gap: l.gap,
                    padding_left,
                    padding_right,
                    padding_top,
                    padding_bottom,
                    wrap: l.wrap,
                    main_align: Some(match l.main_align {
                        MainAlign::Min => v1::MainAxisAlign::Start,
                        MainAlign::Center => v1::MainAxisAlign::Center,
                        MainAlign::Max => v1::MainAxisAlign::End,
                        MainAlign::SpaceBetween
                        | MainAlign::SpaceAround
                        | MainAlign::SpaceEvenly => v1::MainAxisAlign::SpaceBetween,
                    }),
                    cross_align: Some(match l.cross_align {
                        CrossAlign::Min => v1::CrossAxisAlign::Start,
                        CrossAlign::Center => v1::CrossAxisAlign::Center,
                        CrossAlign::Max => v1::CrossAxisAlign::End,
                        CrossAlign::Baseline => v1::CrossAxisAlign::Baseline,
                    }),
                    hug_main,
                    hug_cross,
                }
            });
        }
        G::Kind => v.kind = kind_from(&f.kind, cx),
        G::Geometry => {
            v.fill_geometry.clear();
            v.stroke_geometry.clear();
            v.svg_snapshot = None;
        }
        G::Source => v.source = None,
    }
}

/// What group `g` holds on the figcraft side.
pub(crate) fn native(g: G, f: &fc::Node) -> Value {
    match g {
        G::Name => json!(f.name),
        G::Flags => json!([f.visible, f.locked]),
        G::Transform => json!(f.transform.as_coeffs()),
        G::Size => json!([f.width, f.height]),
        G::Opacity => json!(f.opacity),
        G::Blend => serde_json::to_value(f.blend).unwrap_or_default(),
        G::Corners => serde_json::to_value(f.corners).unwrap_or_default(),
        G::Clip => json!(f.clip),
        G::Mask => json!([
            f.is_mask,
            serde_json::to_value(f.mask_type).unwrap_or_default()
        ]),
        G::Fills => serde_json::to_value(&f.fills).unwrap_or_default(),
        G::Stroke => serde_json::to_value((&f.strokes, &f.stroke)).unwrap_or_default(),
        G::Effects => serde_json::to_value(&f.effects).unwrap_or_default(),
        G::LayoutChild => {
            let l = &f.child_layout;
            serde_json::to_value((
                l.absolute,
                l.min_w,
                l.max_w,
                l.min_h,
                l.max_h,
                f.constraints,
            ))
            .unwrap_or_default()
        }
        G::Layout => {
            serde_json::to_value((&f.layout, f.child_layout.sizing_h, f.child_layout.sizing_v))
                .unwrap_or_default()
        }
        G::Kind => serde_json::to_value(&f.kind).unwrap_or_default(),
        // Outlines and snapshots follow the shape: its size and kind (a vector's network, a text's
        // characters and style).
        G::Geometry => serde_json::to_value((f.width, f.height, &f.kind)).unwrap_or_default(),
        G::Source => Value::Null,
    }
}

fn kind_to(v: &v1::Node, cx: &Cx, page: bool) -> NodeKind {
    let k = match &v.kind {
        v1::NodeKind::Page if page => NodeKind::Page {
            background: Color::DEFAULT_PAGE,
            flows: Vec::new(),
            prototype: Default::default(),
        },
        v1::NodeKind::Document
        | v1::NodeKind::Page
        | v1::NodeKind::Frame
        | v1::NodeKind::Other { .. } => NodeKind::Frame,
        v1::NodeKind::Group => NodeKind::Group,
        v1::NodeKind::Component => NodeKind::Component {
            description: String::new(),
            props: BTreeMap::new(),
            prop_order: Vec::new(),
        },
        v1::NodeKind::ComponentSet => NodeKind::ComponentSet {
            description: String::new(),
        },
        v1::NodeKind::Instance { component } => {
            match component.and_then(|c| cx.components.get(&c.0)) {
                Some(c) => NodeKind::Instance {
                    component: *c,
                    props: BTreeMap::new(),
                },
                None => NodeKind::Frame,
            }
        }
        v1::NodeKind::Rectangle => NodeKind::Rectangle,
        v1::NodeKind::Ellipse => NodeKind::Ellipse {
            start: 0.0,
            sweep: std::f64::consts::TAU,
            inner: 0.0,
        },
        v1::NodeKind::Line => NodeKind::Line,
        v1::NodeKind::Polygon => NodeKind::Polygon { count: 3 },
        v1::NodeKind::Star => NodeKind::Star {
            count: 5,
            inner_ratio: 0.382,
        },
        v1::NodeKind::Vector => NodeKind::Vector {
            network: network_of(&v.fill_geometry),
        },
        v1::NodeKind::BooleanOperation { op } => NodeKind::BooleanOp {
            op: match op {
                None | Some(v1::BooleanOp::Union) => BoolOp::Union,
                Some(v1::BooleanOp::Subtract) => BoolOp::Subtract,
                Some(v1::BooleanOp::Intersect) => BoolOp::Intersect,
                Some(v1::BooleanOp::Exclude) => BoolOp::Exclude,
            },
        },
        v1::NodeKind::Text(t) => NodeKind::Text(text_to(t)),
        v1::NodeKind::Utility if v.children.is_empty() => NodeKind::Slice,
        v1::NodeKind::Utility => NodeKind::Frame,
    };
    // Only containers hold children in figcraft.
    if v.children.is_empty() || k.is_container() {
        k
    } else {
        NodeKind::Frame
    }
}

fn kind_from(k: &NodeKind, cx: &Cx) -> v1::NodeKind {
    match k {
        NodeKind::Page { .. } => v1::NodeKind::Page,
        NodeKind::Frame | NodeKind::Section { .. } => v1::NodeKind::Frame,
        NodeKind::Group => v1::NodeKind::Group,
        NodeKind::Rectangle => v1::NodeKind::Rectangle,
        NodeKind::Ellipse { .. } => v1::NodeKind::Ellipse,
        NodeKind::Polygon { .. } => v1::NodeKind::Polygon,
        NodeKind::Star { .. } => v1::NodeKind::Star,
        NodeKind::Line => v1::NodeKind::Line,
        NodeKind::Vector { .. } => v1::NodeKind::Vector,
        NodeKind::Text(t) => v1::NodeKind::Text(text_from(t)),
        NodeKind::BooleanOp { op } => v1::NodeKind::BooleanOperation {
            op: Some(match op {
                BoolOp::Union => v1::BooleanOp::Union,
                BoolOp::Subtract => v1::BooleanOp::Subtract,
                BoolOp::Intersect => v1::BooleanOp::Intersect,
                BoolOp::Exclude => v1::BooleanOp::Exclude,
            }),
        },
        NodeKind::Component { .. } => v1::NodeKind::Component,
        NodeKind::ComponentSet { .. } => v1::NodeKind::ComponentSet,
        NodeKind::Instance { component, .. } => v1::NodeKind::Instance {
            component: cx.uuid_of.get(component).copied().map(v1::NodeId),
        },
        NodeKind::Slice => v1::NodeKind::Utility,
    }
}

/// A vector network from SVG path data (all paths as one; none parse: empty).
fn network_of(paths: &[String]) -> VectorNetwork {
    let mut all = BezPath::new();
    for p in paths {
        if let Ok(b) = BezPath::from_svg(p) {
            all.extend(b);
        }
    }
    if all.elements().is_empty() {
        VectorNetwork::default()
    } else {
        VectorNetwork::from_path(&all)
    }
}

fn text_to(t: &v1::TextData) -> fc::TextData {
    let mut d = fc::TextData {
        characters: t.characters.clone(),
        ..fc::TextData::default()
    };
    let s = &mut d.style;
    if let Some(f) = &t.font_family {
        s.font_family.clone_from(f);
    }
    if let Some(f) = &t.font_style {
        s.font_style.clone_from(f);
    }
    s.font_size = t.font_size;
    if let Some(w) = t.font_weight {
        s.variations.insert("wght".into(), w);
    }
    if let Some(w) = t.font_width {
        s.variations.insert("wdth".into(), w);
    }
    s.line_height = t.line_height.map_or(LineHeight::Auto, LineHeight::Pixels);
    s.letter_spacing = LetterSpacing::Pixels(t.letter_spacing);
    s.case = match t.text_case {
        v1::TextCase::Original => TextCase::Original,
        v1::TextCase::Upper => TextCase::Upper,
        v1::TextCase::Lower => TextCase::Lower,
        v1::TextCase::Title => TextCase::Title,
        v1::TextCase::SmallCaps | v1::TextCase::SmallCapsForced => TextCase::SmallCaps,
    };
    d.align_h = match t.align {
        v1::TextAlign::Left => TextAlignH::Left,
        v1::TextAlign::Center => TextAlignH::Center,
        v1::TextAlign::Right => TextAlignH::Right,
        v1::TextAlign::Justified => TextAlignH::Justified,
    };
    d
}

fn text_from(d: &fc::TextData) -> v1::TextData {
    let s = &d.style;
    v1::TextData {
        characters: d.characters.clone(),
        font_family: Some(s.font_family.clone()),
        font_size: s.font_size,
        font_weight: s.variations.get("wght").copied(),
        font_style: Some(s.font_style.clone()),
        font_width: s.variations.get("wdth").copied(),
        line_height: match s.line_height {
            LineHeight::Pixels(p) => Some(p),
            LineHeight::Percent(p) => Some(p / 100.0 * s.font_size),
            LineHeight::Auto => None,
        },
        letter_spacing: match s.letter_spacing {
            LetterSpacing::Pixels(p) => p,
            LetterSpacing::Percent(p) => p / 100.0 * s.font_size,
        },
        text_case: match s.case {
            TextCase::Original => v1::TextCase::Original,
            TextCase::Upper => v1::TextCase::Upper,
            TextCase::Lower => v1::TextCase::Lower,
            TextCase::Title => v1::TextCase::Title,
            TextCase::SmallCaps => v1::TextCase::SmallCaps,
        },
        align: match d.align_h {
            TextAlignH::Left => v1::TextAlign::Left,
            TextAlignH::Center => v1::TextAlign::Center,
            TextAlignH::Right => v1::TextAlign::Right,
            TextAlignH::Justified => v1::TextAlign::Justified,
        },
        runs: Vec::new(),
    }
}

/// A v1 (Figma-order) transform as an affine map.
pub(crate) fn affine(t: v1::Transform) -> Affine {
    Affine::new([t.a, t.c, t.b, t.d, t.tx, t.ty])
}

pub(crate) fn transform(a: Affine) -> v1::Transform {
    let [a, c, b, d, tx, ty] = a.as_coeffs();
    v1::Transform { a, b, tx, c, d, ty }
}

fn color_to(c: v1::Color) -> Color {
    Color::new(c.r as f32, c.g as f32, c.b as f32, c.a as f32)
}

fn color_from(c: Color) -> v1::Color {
    v1::Color::rgba(
        f64::from(c.r),
        f64::from(c.g),
        f64::from(c.b),
        f64::from(c.a),
    )
}

fn blend_to(b: v1::BlendMode) -> BlendMode {
    match b {
        v1::BlendMode::Normal => BlendMode::Normal,
        v1::BlendMode::PassThrough => BlendMode::PassThrough,
        v1::BlendMode::Multiply => BlendMode::Multiply,
        v1::BlendMode::Screen => BlendMode::Screen,
        v1::BlendMode::Overlay => BlendMode::Overlay,
        v1::BlendMode::Darken => BlendMode::Darken,
        v1::BlendMode::Lighten => BlendMode::Lighten,
        v1::BlendMode::ColorDodge => BlendMode::ColorDodge,
        v1::BlendMode::ColorBurn => BlendMode::ColorBurn,
        v1::BlendMode::HardLight => BlendMode::HardLight,
        v1::BlendMode::SoftLight => BlendMode::SoftLight,
        v1::BlendMode::Difference => BlendMode::Difference,
        v1::BlendMode::Exclusion => BlendMode::Exclusion,
        v1::BlendMode::Hue => BlendMode::Hue,
        v1::BlendMode::Saturation => BlendMode::Saturation,
        v1::BlendMode::Color => BlendMode::Color,
        v1::BlendMode::Luminosity => BlendMode::Luminosity,
        v1::BlendMode::LinearBurn => BlendMode::PlusDarker,
        v1::BlendMode::LinearDodge => BlendMode::PlusLighter,
    }
}

fn blend_from(b: BlendMode) -> v1::BlendMode {
    match b {
        BlendMode::Normal => v1::BlendMode::Normal,
        BlendMode::PassThrough => v1::BlendMode::PassThrough,
        BlendMode::Multiply => v1::BlendMode::Multiply,
        BlendMode::Screen => v1::BlendMode::Screen,
        BlendMode::Overlay => v1::BlendMode::Overlay,
        BlendMode::Darken => v1::BlendMode::Darken,
        BlendMode::Lighten => v1::BlendMode::Lighten,
        BlendMode::ColorDodge => v1::BlendMode::ColorDodge,
        BlendMode::ColorBurn => v1::BlendMode::ColorBurn,
        BlendMode::HardLight => v1::BlendMode::HardLight,
        BlendMode::SoftLight => v1::BlendMode::SoftLight,
        BlendMode::Difference => v1::BlendMode::Difference,
        BlendMode::Exclusion => v1::BlendMode::Exclusion,
        BlendMode::Hue => v1::BlendMode::Hue,
        BlendMode::Saturation => v1::BlendMode::Saturation,
        BlendMode::Color => v1::BlendMode::Color,
        BlendMode::Luminosity => v1::BlendMode::Luminosity,
        BlendMode::PlusDarker => v1::BlendMode::LinearBurn,
        BlendMode::PlusLighter => v1::BlendMode::LinearDodge,
    }
}

fn cap_to(c: v1::StrokeCap) -> StrokeCap {
    match c {
        v1::StrokeCap::Round => StrokeCap::Round,
        v1::StrokeCap::Square => StrokeCap::Square,
        v1::StrokeCap::LineArrow => StrokeCap::LineArrow,
        v1::StrokeCap::TriangleArrow | v1::StrokeCap::TriangleFilled => StrokeCap::TriangleArrow,
        v1::StrokeCap::DiamondFilled => StrokeCap::DiamondFilled,
        v1::StrokeCap::CircleFilled => StrokeCap::CircleFilled,
        _ => StrokeCap::None,
    }
}

fn cap_from(c: StrokeCap) -> v1::StrokeCap {
    match c {
        StrokeCap::None => v1::StrokeCap::None,
        StrokeCap::Round => v1::StrokeCap::Round,
        StrokeCap::Square => v1::StrokeCap::Square,
        StrokeCap::LineArrow => v1::StrokeCap::LineArrow,
        StrokeCap::TriangleArrow | StrokeCap::ReversedTriangle => v1::StrokeCap::TriangleArrow,
        StrokeCap::CircleFilled => v1::StrokeCap::CircleFilled,
        StrokeCap::DiamondFilled => v1::StrokeCap::DiamondFilled,
    }
}

fn effect_to(e: &v1::Effect) -> Effect {
    let shadow = || Shadow {
        color: color_to(e.color),
        offset: (e.offset_x, e.offset_y),
        radius: e.radius,
        spread: e.spread,
        ..Shadow::default()
    };
    let kind = match e.kind {
        v1::EffectKind::DropShadow => EffectKind::DropShadow(shadow()),
        v1::EffectKind::InnerShadow => EffectKind::InnerShadow(shadow()),
        v1::EffectKind::LayerBlur => EffectKind::LayerBlur(Blur::uniform(e.radius)),
        v1::EffectKind::BackgroundBlur => EffectKind::BackgroundBlur(Blur::uniform(e.radius)),
    };
    Effect {
        kind,
        visible: e.visible,
    }
}

fn effect_from(e: &Effect) -> Option<v1::Effect> {
    let shadow = |kind, s: &Shadow| v1::Effect {
        kind,
        visible: e.visible,
        color: color_from(s.color),
        offset_x: s.offset.0,
        offset_y: s.offset.1,
        radius: s.radius,
        spread: s.spread,
    };
    let blur = |kind, b: &Blur| v1::Effect {
        kind,
        visible: e.visible,
        color: v1::Color::rgba(0.0, 0.0, 0.0, 0.0),
        offset_x: 0.0,
        offset_y: 0.0,
        radius: b.radius,
        spread: 0.0,
    };
    Some(match &e.kind {
        EffectKind::DropShadow(s) => shadow(v1::EffectKind::DropShadow, s),
        EffectKind::InnerShadow(s) => shadow(v1::EffectKind::InnerShadow, s),
        EffectKind::LayerBlur(b) => blur(v1::EffectKind::LayerBlur, b),
        EffectKind::BackgroundBlur(b) => blur(v1::EffectKind::BackgroundBlur, b),
        EffectKind::Noise(_) | EffectKind::Texture(_) | EffectKind::Glass(_) => return None,
    })
}

/// Where a gradient's three handles sit in gradient space (Figma's convention: start, end, and
/// the end of the secondary axis).
const LINEAR: [(f64, f64); 3] = [(0.0, 0.5), (1.0, 0.5), (0.0, 1.0)];
const RADIAL: [(f64, f64); 3] = [(0.5, 0.5), (1.0, 0.5), (0.5, 1.0)];

/// The affine map taking the unit points (0,0), (1,0), (0,1) to `p`.
fn basis(p: [(f64, f64); 3]) -> Affine {
    Affine::new([
        p[1].0 - p[0].0,
        p[1].1 - p[0].1,
        p[2].0 - p[0].0,
        p[2].1 - p[0].1,
        p[0].0,
        p[0].1,
    ])
}

fn handles(t: Option<v1::Transform>, at: [(f64, f64); 3]) -> [(f64, f64); 3] {
    let inv = affine(t.unwrap_or_default()).inverse();
    at.map(|(x, y)| {
        let p = inv * Point::new(x, y);
        (p.x, p.y)
    })
}

fn gradient_transform(g: &Gradient, at: [(f64, f64); 3]) -> Option<v1::Transform> {
    let h = basis([g.from, g.to, g.width]);
    let s = basis(at);
    let (dh, ds) = (h.determinant(), s.determinant());
    if !(dh.is_finite() && ds.is_finite()) || dh == 0.0 || ds == 0.0 {
        return None;
    }
    // Unit box → gradient space is the inverse of the handle map (gradient space → unit box).
    let t = (h * s.inverse()).inverse();
    t.as_coeffs()
        .iter()
        .all(|c| c.is_finite())
        .then(|| transform(t))
}

fn stops_to(stops: &[v1::GradientStop]) -> Vec<ColorStop> {
    stops
        .iter()
        .map(|s| ColorStop {
            position: s.position as f32,
            color: color_to(s.color),
        })
        .collect()
}

fn stops_from(stops: &[ColorStop]) -> Vec<v1::GradientStop> {
    stops
        .iter()
        .map(|s| v1::GradientStop {
            position: f64::from(s.position),
            color: color_from(s.color),
        })
        .collect()
}

fn paint_to(p: &v1::Paint, cx: &Cx) -> Paint {
    let gradient = |kind, stops: &[v1::GradientStop], t: &Option<v1::Transform>| {
        let [from, to, width] = handles(
            *t,
            if kind == GradientKind::Linear {
                LINEAR
            } else {
                RADIAL
            },
        );
        PaintKind::Gradient(Gradient {
            kind,
            stops: stops_to(stops),
            from,
            to,
            width,
        })
    };
    let kind = match &p.kind {
        v1::PaintKind::Solid { color } => PaintKind::Solid {
            color: color_to(*color),
        },
        v1::PaintKind::LinearGradient { stops, transform } => {
            gradient(GradientKind::Linear, stops, transform)
        }
        v1::PaintKind::RadialGradient { stops, transform } => {
            gradient(GradientKind::Radial, stops, transform)
        }
        v1::PaintKind::AngularGradient { stops, transform } => {
            gradient(GradientKind::Angular, stops, transform)
        }
        v1::PaintKind::Image {
            asset,
            scale_mode,
            transform,
            ..
        } => PaintKind::Image(ImagePaint {
            image: cx.key(asset),
            scale_mode: match scale_mode {
                v1::ImageScaleMode::Fill => ImageScaleMode::Fill,
                v1::ImageScaleMode::Fit => ImageScaleMode::Fit,
                v1::ImageScaleMode::Crop | v1::ImageScaleMode::Stretch => ImageScaleMode::Crop,
                v1::ImageScaleMode::Tile => ImageScaleMode::Tile,
            },
            scale: 1.0,
            rotation: 0,
            crop: transform.map(|t| affine(t).as_coeffs()),
            filters: Default::default(),
        }),
    };
    Paint {
        kind,
        opacity: p.opacity as f32,
        visible: p.visible,
        blend: blend_to(p.blend),
        color_alpha: 1.0,
    }
}

fn paint_from(p: &Paint, cx: &Cx) -> Option<v1::Paint> {
    let kind = match &p.kind {
        PaintKind::Solid { color } => v1::PaintKind::Solid {
            color: color_from(*color),
        },
        PaintKind::Gradient(g) => {
            let stops = stops_from(&g.stops);
            match g.kind {
                GradientKind::Linear => v1::PaintKind::LinearGradient {
                    stops,
                    transform: gradient_transform(g, LINEAR),
                },
                GradientKind::Radial | GradientKind::Diamond => v1::PaintKind::RadialGradient {
                    stops,
                    transform: gradient_transform(g, RADIAL),
                },
                GradientKind::Angular => v1::PaintKind::AngularGradient {
                    stops,
                    transform: gradient_transform(g, RADIAL),
                },
            }
        }
        PaintKind::Image(i) => v1::PaintKind::Image {
            asset: cx.asset(&i.image),
            source_ref: None,
            scale_mode: match i.scale_mode {
                ImageScaleMode::Fill => v1::ImageScaleMode::Fill,
                ImageScaleMode::Fit => v1::ImageScaleMode::Fit,
                ImageScaleMode::Crop => v1::ImageScaleMode::Crop,
                ImageScaleMode::Tile => v1::ImageScaleMode::Tile,
            },
            transform: i.crop.map(|c| transform(Affine::new(c))),
        },
        PaintKind::Pattern(_) => return None,
    };
    Some(v1::Paint {
        kind,
        opacity: f64::from(p.opacity),
        blend: blend_from(p.blend),
        visible: p.visible,
    })
}

use figma_api::models;
use frame_proto::NodeId;

#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub(super) fn with_opacity(&self, opacity: f64) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: (self.a * opacity).clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FillPaint {
    Solid(Rgba),
    GradientLinear {
        transform: Option<[[f64; 3]; 2]>,
        stops: Vec<GradientStop>,
    },
    GradientRadial {
        transform: Option<[[f64; 3]; 2]>,
        stops: Vec<GradientStop>,
    },
    GradientAngular {
        transform: Option<[[f64; 3]; 2]>,
        stops: Vec<GradientStop>,
    },
    Image {
        image_hash: Option<String>,
        data_base64: String,
        alpha: f64,
        scale_mode: ImageScaleMode,
        image_transform: Option<[[f64; 3]; 2]>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageScaleMode {
    #[default]
    Fill,
    Fit,
    Crop,
    Tile,
    Stretch,
}

impl FillPaint {
    pub(super) fn with_opacity(self, opacity: f64) -> Self {
        match self {
            FillPaint::Solid(color) => FillPaint::Solid(color.with_opacity(opacity)),
            FillPaint::GradientLinear { transform, stops } => FillPaint::GradientLinear {
                transform,
                stops: stops
                    .into_iter()
                    .map(|s| GradientStop {
                        offset: s.offset,
                        color: s.color.with_opacity(opacity),
                    })
                    .collect(),
            },
            FillPaint::GradientRadial { transform, stops } => FillPaint::GradientRadial {
                transform,
                stops: stops
                    .into_iter()
                    .map(|s| GradientStop {
                        offset: s.offset,
                        color: s.color.with_opacity(opacity),
                    })
                    .collect(),
            },
            FillPaint::GradientAngular { transform, stops } => FillPaint::GradientAngular {
                transform,
                stops: stops
                    .into_iter()
                    .map(|s| GradientStop {
                        offset: s.offset,
                        color: s.color.with_opacity(opacity),
                    })
                    .collect(),
            },
            FillPaint::Image {
                image_hash,
                data_base64,
                alpha,
                scale_mode,
                image_transform,
            } => FillPaint::Image {
                image_hash,
                data_base64,
                alpha: (alpha * opacity).clamp(0.0, 1.0),
                scale_mode,
                image_transform,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectKind {
    DropShadow,
    InnerShadow,
    LayerBlur,
    BackgroundBlur,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeEffect {
    pub kind: EffectKind,
    pub color: Rgba,
    pub offset_x: f64,
    pub offset_y: f64,
    pub radius: f64,
    pub spread: f64,
}

pub type StrokeCap = models::has_geometry_trait::StrokeCap;
pub type StrokeJoin = models::has_geometry_trait::StrokeJoin;
pub type StrokeAlign = models::has_geometry_trait::StrokeAlign;

#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    pub color: Rgba,
    pub width: f64,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub align: StrokeAlign,
    pub miter_limit: f64,
    pub dash_pattern: Vec<f64>,
    pub dash_offset: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlendMix {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextCase {
    Original,
    Upper,
    Lower,
    Title,
    SmallCaps,
    SmallCapsForced,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaintPrimitive {
    LayerStart {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        blend: BlendMix,
        opacity: f64,
    },
    LayerEnd {
        node_id: NodeId,
    },
    ClipStart {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        corner_radii: frame_proto::CornerRadii,
    },
    ClipEnd {
        node_id: NodeId,
    },
    Rect {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fills: Vec<(FillPaint, BlendMix)>,
        stroke: Option<StrokeStyle>,
        corner_radii: frame_proto::CornerRadii,
        effects: Vec<NodeEffect>,
        blend: BlendMix,
        rotation: f64,
    },
    Text {
        node_id: NodeId,
        x: f64,
        y: f64,
        text: String,
        color: Rgba,
        font_size: f64,
        line_height: Option<f64>,
        letter_spacing: f64,
        text_case: TextCase,
        blend: BlendMix,
    },
    Path {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill_paths: Vec<String>,
        stroke_paths: Vec<String>,
        svg_base64: Option<String>,
        fills: Vec<(FillPaint, BlendMix)>,
        stroke: Option<StrokeStyle>,
        effects: Vec<NodeEffect>,
        blend: BlendMix,
        rotation: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderDiagnostics {
    pub total_nodes: usize,
    pub non_normal_blend_nodes: usize,
    pub gradient_fill_nodes: usize,
    pub image_fill_nodes: usize,
    pub drop_shadow_nodes: usize,
    pub inner_shadow_nodes: usize,
    pub layer_blur_nodes: usize,
    pub layer_blur_approx_nodes: usize,
    pub background_blur_nodes: usize,
    pub background_blur_approx_nodes: usize,
    pub alpha_mask_nodes: usize,
    pub luminance_mask_nodes: usize,
}

impl RenderDiagnostics {
    pub fn format_summary(&self) -> String {
        format!(
            "Render diag: nodes {} | blend {} | grad {} | image {} | drop {} | inner {} | blur {} (approx {}) | bg-blur {} (approx {}) | alpha-mask {} | luminance-mask {}*",
            self.total_nodes,
            self.non_normal_blend_nodes,
            self.gradient_fill_nodes,
            self.image_fill_nodes,
            self.drop_shadow_nodes,
            self.inner_shadow_nodes,
            self.layer_blur_nodes,
            self.layer_blur_approx_nodes,
            self.background_blur_nodes,
            self.background_blur_approx_nodes,
            self.alpha_mask_nodes,
            self.luminance_mask_nodes
        )
    }
}

#[cfg(feature = "anyrender")]
#[derive(Debug, Clone, Copy)]
pub struct TextFontRef<'a> {
    pub bytes: &'a [u8],
    pub index: u32,
}

use frame_proto::{FrameDocument, NodeId, RenderNodeClass, RenderNodeProjection};

#[derive(Debug, Clone, PartialEq)]
pub enum PaintPrimitive {
    Rect {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        opacity: f64,
    },
    Text {
        node_id: NodeId,
        x: f64,
        y: f64,
        text: String,
        opacity: f64,
    },
    Path {
        node_id: NodeId,
        path_count: usize,
        opacity: f64,
    },
}

pub fn build_paint_primitives(doc: &FrameDocument, root: NodeId) -> Vec<PaintPrimitive> {
    doc.project_subtree(root)
        .into_iter()
        .filter(|p| p.visible)
        .flat_map(projection_to_primitives)
        .collect()
}

fn projection_to_primitives(p: RenderNodeProjection) -> Vec<PaintPrimitive> {
    let (x, y, w, h) = extract_rect(&p);
    let opacity = p.opacity.unwrap_or(1.0);

    match p.class {
        RenderNodeClass::Container | RenderNodeClass::Shape | RenderNodeClass::Utility => {
            vec![PaintPrimitive::Rect {
                node_id: p.id,
                x,
                y,
                width: w,
                height: h,
                opacity,
            }]
        }
        RenderNodeClass::Text => {
            let text = p
                .text
                .as_ref()
                .map(|t| t.characters.clone())
                .unwrap_or_default();
            vec![PaintPrimitive::Text {
                node_id: p.id,
                x,
                y,
                text,
                opacity,
            }]
        }
        RenderNodeClass::Vector => {
            let count = p.fill_geometry.len() + p.stroke_geometry.len();
            vec![PaintPrimitive::Path {
                node_id: p.id,
                path_count: count,
                opacity,
            }]
        }
        RenderNodeClass::Unknown => Vec::new(),
    }
}

fn extract_rect(p: &RenderNodeProjection) -> (f64, f64, f64, f64) {
    let (mut x, mut y) = (0.0, 0.0);
    let (mut w, mut h) = (0.0, 0.0);

    if let Some(size) = &p.size {
        w = size.x;
        h = size.y;
    }

    if let Some(t) = &p.relative_transform {
        if t.len() >= 2 && t[0].len() >= 3 && t[1].len() >= 3 {
            x = t[0][2];
            y = t[1][2];
        }
    }

    (x, y, w, h)
}

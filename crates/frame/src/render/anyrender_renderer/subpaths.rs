//! Open and closed subpaths: an authored stroke strokes open subpaths as
//! lines and never fills them.

/// `path`'s closed subpaths and its open ones, as two paths.
pub(crate) fn split_open_closed(path: &kurbo::BezPath) -> (kurbo::BezPath, kurbo::BezPath) {
    use kurbo::PathEl;
    let (mut closed, mut open) = (kurbo::BezPath::new(), kurbo::BezPath::new());
    let mut sub: Vec<PathEl> = Vec::new();
    let mut flush = |sub: &mut Vec<PathEl>| {
        // A lone move-to draws nothing either way.
        if sub.len() > 1 {
            let target = if matches!(sub.last(), Some(PathEl::ClosePath)) {
                &mut closed
            } else {
                &mut open
            };
            sub.iter().for_each(|el| target.push(*el));
        }
        sub.clear();
    };
    for el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) {
            flush(&mut sub);
        }
        sub.push(*el);
        if matches!(el, PathEl::ClosePath) {
            flush(&mut sub);
        }
    }
    flush(&mut sub);
    (closed, open)
}

/// The closed subpaths of SVG path data `d` (`None` when it doesn't parse).
pub(crate) fn closed_subpaths_svg(d: &str) -> Option<String> {
    let path = kurbo::BezPath::from_svg(d).ok()?;
    Some(split_open_closed(&path).0.to_svg())
}

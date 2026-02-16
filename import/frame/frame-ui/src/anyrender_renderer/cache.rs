use super::geometry::compute_svg_fit_transform;

/// Cache for parsed SVG path strings → BezPath. Keyed by string hash.
/// Eliminates re-parsing in blur kernel loops (6-20x per path per frame).
pub(super) fn cached_bez_path(path_data: &str) -> Option<kurbo::BezPath> {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, kurbo::BezPath>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path_data.hash(&mut hasher);
    let key = hasher.finish();

    if let Ok(cache) = CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return Some(cached.clone());
        }
    }

    let path = kurbo::BezPath::from_svg(path_data).ok()?;

    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, path.clone());
    }

    Some(path)
}

/// Cache for base64-encoded SVG → decoded text string.
/// Avoids base64 decode + UTF-8 validation every frame.
pub(super) fn cached_svg_decode(svg_base64: &str) -> Option<String> {
    use base64::Engine;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, String>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    svg_base64.hash(&mut hasher);
    let key = hasher.finish();

    if let Ok(cache) = CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return Some(cached.clone());
        }
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(svg_base64)
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;

    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, text.clone());
    }

    Some(text)
}

/// Cache for SVG text → fit transform. Avoids usvg::Tree::from_str per frame.
pub(super) fn cached_svg_fit_transform(
    svg_text: &str,
    target_width: f64,
    target_height: f64,
) -> kurbo::Affine {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, kurbo::Affine>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    svg_text.hash(&mut hasher);
    target_width.to_bits().hash(&mut hasher);
    target_height.to_bits().hash(&mut hasher);
    let key = hasher.finish();

    if let Ok(cache) = CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return *cached;
        }
    }

    let transform = compute_svg_fit_transform(svg_text, target_width, target_height);

    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, transform);
    }

    transform
}

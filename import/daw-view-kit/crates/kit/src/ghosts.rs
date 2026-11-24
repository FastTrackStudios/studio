//! Everyone else in the session, drawn faintly over the arrangement.
//!
//! Each remote peer is a ghost in their own colour: their time selection
//! as a wash, their edit cursor as a thin line, their play cursor (when
//! transports are independent) as a faint line with no trail, the items
//! they have selected outlined, and their mouse — an arrow with their
//! name — on the beat and the track it is over. Faint on purpose: this is
//! what other people are doing, and it must never be mistaken for, or
//! draw over, what YOU are doing.
//!
//! The collaboration driver publishes the roster here ([`publish`]); the
//! arrangement paints it every frame ([`paint`]), interpolating pointers
//! and extrapolating play cursors to the moment being drawn.

use std::collections::HashMap;

use fts_session::presence::Pointer;

pub use fts_session::presence::store::*;

// ── panels: where each named part of the window is ──────────────────────

/// A panel's rectangle in the window, logical pixels: x, y, width, height.
pub type PanelRect = (f64, f64, f64, f64);

thread_local! {
    static REGION_NODES: std::cell::RefCell<HashMap<String, std::rc::Rc<dioxus::prelude::MountedData>>> =
        std::cell::RefCell::new(HashMap::new());
    static REGION_RECTS: std::cell::RefCell<HashMap<String, PanelRect>> = std::cell::RefCell::new(HashMap::new());
}

/// A panel mounted: remember its node, measured by [`measure_regions`].
/// For `onmounted` on the panel's outermost element.
pub fn region_mounted(id: &str, node: std::rc::Rc<dioxus::prelude::MountedData>) {
    REGION_NODES.with(|n| n.borrow_mut().insert(id.to_owned(), node));
}

/// Re-measure every panel (they move with the window and the layout).
pub async fn measure_regions() {
    let nodes: Vec<(String, std::rc::Rc<dioxus::prelude::MountedData>)> = REGION_NODES.with(|n| {
        n.borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    });
    for (id, node) in nodes {
        match node.get_client_rect().await {
            Ok(r) if r.size.width > 0.0 && r.size.height > 0.0 => {
                REGION_RECTS.with(|m| {
                    m.borrow_mut()
                        .insert(id, (r.origin.x, r.origin.y, r.size.width, r.size.height))
                });
            }
            // Unmounted, or laid out to nothing: not somewhere to point.
            _ => {
                REGION_RECTS.with(|m| m.borrow_mut().remove(&id));
            }
        }
    }
}

/// A panel that anchors pointers to its content: what is under a point
/// of it, and where that thing is now. Coordinates are logical pixels
/// relative to the panel's own rectangle (see [`region_mounted`], under
/// the same name).
pub trait Anchor {
    /// The content under `(x, y)`: a key and a place on it (`None` over
    /// nothing in particular — the pointer is then placed in the panel).
    /// `size` is the panel's width and height.
    fn anchor(&self, x: f64, y: f64, size: (f64, f64)) -> Option<(String, f64, f64)>;
    /// Where that content is in this view now (`None`: not shown).
    fn place(&self, key: &str, u: f64, v: f64, size: (f64, f64)) -> Option<(f64, f64)>;
}

thread_local! {
    static ANCHORS: std::cell::RefCell<HashMap<String, std::rc::Rc<dyn Anchor>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// A panel's content anchoring, under its region name.
pub fn register_anchor(panel: &str, anchor: std::rc::Rc<dyn Anchor>) {
    ANCHORS.with(|a| a.borrow_mut().insert(panel.to_owned(), anchor));
}

fn anchor_for(panel: &str) -> Option<std::rc::Rc<dyn Anchor>> {
    ANCHORS.with(|a| a.borrow().get(panel).cloned())
}

/// Where a named panel is in this window.
#[must_use]
pub fn region_rect(id: &str) -> Option<PanelRect> {
    REGION_RECTS.with(|m| m.borrow().get(id).copied())
}

/// The mouse moved in the window (logical pixels): which panel it is
/// over and where in it, or where in the window. Over the lanes the
/// arrangement's own report wins.
pub fn local_window_pointer(x: f64, y: f64, window: (f64, f64)) {
    // Over the arrangement, the arrangement publishes the pointer (its
    // lanes and its track panel both). Off it, the window does — which is
    // how leaving it for the mixer below lets go of the lane it was on.
    let inside = |(rx, ry, rw, rh): PanelRect| x >= rx && y >= ry && x < rx + rw && y < ry + rh;
    match region_rect(ARRANGEMENT).map(inside) {
        Some(true) => return,
        Some(false) => OVER_LANES.store(false, std::sync::atomic::Ordering::Relaxed),
        // Not measured yet: trust the arrangement's own word.
        None if OVER_LANES.load(std::sync::atomic::Ordering::Relaxed) => return,
        None => {}
    }
    let hit = REGION_RECTS
        .with(|m| {
            m.borrow()
                .iter()
                .filter(|(id, _)| id.as_str() != ARRANGEMENT)
                .filter(|(_, (rx, ry, rw, rh))| x >= *rx && y >= *ry && x < rx + rw && y < ry + rh)
                // The smallest panel under the point is the one it is in.
                .min_by(|a, b| (a.1.2 * a.1.3).total_cmp(&(b.1.2 * b.1.3)))
                .map(|(id, rect)| (id.clone(), *rect))
        })
        .map(|(id, (rx, ry, rw, rh))| {
            // Anchored to the content when the panel can say what is there;
            // else a place in the panel.
            anchor_for(&id)
                .and_then(|a| a.anchor(x - rx, y - ry, (rw, rh)))
                .map_or_else(
                    || Pointer::Region {
                        region: id.clone(),
                        x: (x - rx) / rw,
                        y: (y - ry) / rh,
                    },
                    |(key, u, v)| Pointer::Anchor {
                        panel: id.clone(),
                        key,
                        u,
                        v,
                    },
                )
        });
    let pointer = hit.unwrap_or_else(|| Pointer::Region {
        region: "window".into(),
        x: x / window.0.max(1.0),
        y: y / window.1.max(1.0),
    });
    if let Ok(mut local) = LOCAL.lock() {
        local.pointer = Some(pointer);
    }
}

/// The mouse left the window.
pub fn local_window_left() {
    OVER_LANES.store(false, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut local) = LOCAL.lock() {
        local.pointer = None;
    }
}

/// Everyone else's pointer that is not over the lanes (those the
/// arrangement draws), placed in this window: (x, y, name, colour).
#[must_use]
pub fn window_pointers(window: (f64, f64)) -> Vec<(f64, f64, String, u32)> {
    let Ok(slot) = ROSTER.lock() else {
        return Vec::new();
    };
    let Some(published) = slot.as_ref() else {
        return Vec::new();
    };
    let now = now_ms();
    published
        .roster
        .peers
        .values()
        .filter_map(|peer| {
            let state = peer.state.as_ref()?;
            // Over the lanes or the track panel, the arrangement draws it.
            if drawn_by_arrangement(peer.trail.latest()?) {
                return None;
            }
            let (x, y) = peer.trail.screen_at(now, |p| match p {
                Pointer::Region { region, x, y } => {
                    let (rx, ry, rw, rh) = if region == "window" {
                        (0.0, 0.0, window.0, window.1)
                    } else {
                        // A panel this window is not showing: nowhere.
                        region_rect(region)?
                    };
                    Some((x.mul_add(rw, rx), y.mul_add(rh, ry)))
                }
                Pointer::Anchor { panel, key, u, v } => {
                    let (rx, ry, rw, rh) = region_rect(panel)?;
                    let (px, py) = anchor_for(panel)?.place(key, *u, *v, (rw, rh))?;
                    // Only where this window shows it: a measure on a page
                    // (or a strip scrolled) out of view is not drawn at all
                    // — never over whatever lies past the panel's edge.
                    let inside = (0.0..=rw).contains(&px) && (0.0..=rh).contains(&py);
                    inside.then_some((rx + px, ry + py))
                }
                _ => None,
            })?;
            Some((x, y, state.name.clone(), state.color))
        })
        .collect()
}

/// What this window shows, for the roster: its rows' tracks, and the
/// guids of the items on them (`items`, asked only when the rows changed).
pub fn local_shown(rows: &[(daw_proto::Track, u32)], items: impl FnOnce() -> Vec<String>) {
    if let Ok(mut shown) = SHOWN.lock()
        && shown.0.len() != rows.len()
    {
        shown.0 = rows.iter().map(|(t, _)| t.guid.clone()).collect();
        shown.1 = items();
        shown.2 = rows.iter().map(|(t, _)| t.name.clone()).collect();
    }
}

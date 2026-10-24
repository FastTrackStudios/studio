//! Mount only the visible mixer strips plus a small overscan margin.
//! Scrolling this pane never rebuilds the arrangement or expression editor.
use std::collections::HashMap;

use daw::service::{Fx, Track};
use daw_ui::art::geometry::mcp::STRIP_W;
use daw_ui::components::folders::FolderState;
use daw_ui::components::mixer::ChannelStripPreview;
use daw_ui::controls::FxSlotStack;
use dioxus::prelude::*;

use super::{FX_BAND_H, MIXER_W};

/// How many strips are mounted past each edge, and how far the pane must
/// scroll before the window is re-cut.
///
/// Mounting a channel strip is not cheap — measured with blitz-dom's
/// phase timer, a frame that mounted one cost 15-17 ms of `construct`
/// against 2 ms of everything else, because a strip is ~186 nodes of
/// traced vector art with its own gradients and text to shape. Re-cutting
/// on every strip of travel meant paying that on half the frames of a
/// scroll. Committing in steps, with enough overscan to cover the drift,
/// pays it on every third frame instead — the same trade the shared
/// TCP/timeline viewport makes, for the same reason.
const OVERSCAN_STRIPS: usize = 6;
/// The commit step, in strips. Must stay comfortably under the overscan.
const COMMIT_STRIPS: usize = 3;

fn strip_range(offset: f64, count: usize) -> std::ops::Range<usize> {
    let first = (offset.max(0.0) / STRIP_W as f64).floor() as usize;
    let visible = (MIXER_W / STRIP_W as f64).ceil() as usize;
    let first = first.min(count.saturating_sub(visible));
    first.saturating_sub(OVERSCAN_STRIPS)
        ..first.saturating_add(visible + OVERSCAN_STRIPS).min(count)
}

#[component]
pub(super) fn WorkstationMixer(
    tracks: Vec<Track>,
    depths: Vec<u32>,
    fx: HashMap<String, Vec<Fx>>,
    mut folders: Signal<FolderState>,
    height: f32,
    rule: String,
    background: String,
) -> Element {
    let mut first_strip = use_signal(|| 0usize);
    let range = strip_range(first_strip() as f64 * STRIP_W as f64, tracks.len());
    let total_w = tracks.len() as f32 * STRIP_W;
    let has_fx = !fx.is_empty();
    rsx! {
        div {
            "data-testid": "workstation-mixer",
            style: "flex:0 0 {MIXER_W}px;width:{MIXER_W}px;min-height:0;overflow-x:scroll;overflow-y:hidden;border-left:1px solid {rule};background:{background};",
            onscroll: move |event: ScrollEvent| {
                let next = (event.scroll_left().max(0.0) / STRIP_W as f64).floor() as usize;
                // Coarse on purpose: blitz scrolls the pane natively
                // whether or not dioxus hears about it, and the only
                // reason to hear about it is to mount what came into
                // range. See `OVERSCAN_STRIPS`.
                if first_strip().abs_diff(next) >= COMMIT_STRIPS {
                    first_strip.set(next);
                }
            },
            div {
                style: "position:relative;width:{total_w}px;height:100%;",
                for i in range {
                    {
                        let track = &tracks[i];
                        let guid = track.guid.clone();
                        rsx! {
                            div {
                                key: "{guid}",
                                style: "position:absolute;left:{i as f32 * STRIP_W}px;top:0;width:{STRIP_W}px;display:flex;flex-direction:column;overflow:hidden;",
                                if has_fx {
                                    div {
                                        style: "height:{FX_BAND_H}px;flex:0 0 auto;display:flex;align-items:flex-end;overflow:hidden;",
                                        FxSlotStack { fx: fx.get(&guid).cloned().unwrap_or_default(), width: STRIP_W }
                                    }
                                }
                                ChannelStripPreview {
                                    track: track.clone(), index: track.index, depth: depths[i],
                                    collapsed: folders.read().is_collapsed(&guid),
                                    onfoldertoggle: { let guid = guid.clone(); move |_| folders.write().toggle(&guid) },
                                    height,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

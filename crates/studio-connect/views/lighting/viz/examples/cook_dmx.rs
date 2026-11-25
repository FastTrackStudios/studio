//! Every profile look and every library effect, played headless on a
//! venue and recorded off the wire: universe 1's 512 bytes, `FPS` times a
//! second for `SECONDS`. What the desk would send, frame for frame, so
//! another controller can be programmed to send the same (the myDMX
//! showfile, `ignition-daslight`'s `mobile_rig` example).
//!
//! ```sh
//! cargo run -p ignition-viz --example cook_dmx -- \
//!     features/lighting/data/venues/room138-cbu  out.json
//! ```

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "a command-line tool; frame counts are small"
)]

use std::fmt::Write as _;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use ignition_viz::DmxUniverses;
use ignition_viz::playback::{LoadOptions, Playback, tick_playback};
use ignition_viz::spawn::{DmxRes, VenueRes};
use ignition_viz::venue::Venue;

const FPS: u16 = 10;
const SECONDS: u16 = 24;

/// One look or effect, held, and its frames.
fn record(venue_dir: &str, start: impl Fn(&mut Playback) -> bool) -> anyhow::Result<Option<Vec<Vec<u8>>>> {
    let venue = Venue::load(venue_dir)?;
    let mut playback = Playback::load(&venue, LoadOptions::default())?;
    if !start(&mut playback) {
        return Ok(None);
    }
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(VenueRes(venue));
    world.insert_resource(DmxRes(DmxUniverses::new()));
    world.insert_resource(playback);
    let mut frames = Vec::new();
    for i in 0..FPS * SECONDS {
        world.resource_mut::<Playback>().frozen_at = Some(f32::from(i) / f32::from(FPS));
        world
            .run_system_once(tick_playback)
            .map_err(|e| anyhow::anyhow!("tick: {e}"))?;
        let snapshot = world.resource::<DmxRes>().0.snapshot();
        frames.push(snapshot.get(&1).map_or_else(|| vec![0; 512], |u| u.to_vec()));
    }
    Ok(Some(frames))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(venue_dir), Some(out)) = (args.get(1), args.get(2)) else {
        anyhow::bail!("usage: cook_dmx <venue dir> <out.json>");
    };
    let venue = Venue::load(venue_dir)?;
    let names = Playback::load(&venue, LoadOptions::default())?;
    let looks: Vec<String> = names.looks.keys().cloned().collect();
    let mut effects: Vec<String> = names.library.keys().cloned().collect();
    effects.extend(names.bundles.keys().cloned());

    let mut json = format!("{{\"fps\":{FPS},\"items\":[");
    let mut first = true;
    for (kind, list) in [("look", &looks), ("effect", &effects)] {
        for name in list {
            let frames = if kind == "look" {
                record(venue_dir, |p| p.hold_look(name))?
            } else {
                record(venue_dir, |p| p.preview_effect(name))?
            };
            let Some(frames) = frames else {
                eprintln!("skipped {kind} {name}");
                continue;
            };
            if !first {
                json.push(',');
            }
            first = false;
            let body: Vec<String> = frames
                .iter()
                .map(|f| format!("[{}]", f.iter().map(u8::to_string).collect::<Vec<_>>().join(",")))
                .collect();
            write!(json, "{{\"kind\":\"{kind}\",\"name\":{},\"frames\":[{}]}}", serde_json::to_string(name)?, body.join(","))?;
            println!("{kind:6} {name}");
        }
    }
    json.push_str("]}");
    std::fs::write(out, json)?;
    Ok(())
}

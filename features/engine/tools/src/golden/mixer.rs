//! The golden session's mixer: meters frozen at chosen levels, and the
//! Signal chains on the kick's tracks.

use daw_proto::{Track, TrackLevels};
use mixer_view::signal::Chain;
use mixer_view::signal::chain::{Block, Status, Via};

/// The meters, frozen: each track's levels by its index, and the tracks
/// whose clip latch is lit.
#[derive(Clone, Debug, Default)]
pub struct Meters {
    pub levels: Vec<TrackLevels>,
    pub clipped: Vec<String>,
}

/// A level, peak and hold, in dB.
#[derive(Clone, Copy)]
struct Level {
    peak: f32,
    hold: f32,
}

/// The levels by track name: the meter tracks at their names' levels, the
/// rest a quiet signal so every strip's meter is lit.
fn level_of(name: &str, index: usize) -> Level {
    match name {
        // Below -18 dB: the safe colour only.
        "Quiet" => Level {
            peak: -24.0,
            hold: -20.0,
        },
        // Near 0 dB: into the warning colour.
        "Hot" => Level {
            peak: -3.0,
            hold: -1.0,
        },
        // Over 0 dB.
        "Over" => Level {
            peak: 0.5,
            hold: 1.0,
        },
        // Driven past full scale: the latch lights.
        "Clipped" | "Master" => Level {
            peak: 3.0,
            hold: 3.0,
        },
        _ => {
            let spread = f32::from(u8::try_from(index % 7).unwrap_or(0));
            Level {
                peak: spread.mul_add(3.5, -30.0),
                hold: spread.mul_add(3.5, -26.0),
            }
        }
    }
}

/// dB as a linear level.
fn linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Every track's meter, frozen.
#[must_use]
pub fn meters(tracks: &[Track]) -> Meters {
    let levels: Vec<TrackLevels> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let l = level_of(&t.name, i);
            TrackLevels {
                peak_left: linear(l.peak),
                peak_right: linear(l.peak - 2.0),
                hold_left: linear(l.hold),
                hold_right: linear(l.hold - 1.5),
            }
        })
        .collect();
    let clipped = tracks
        .iter()
        .zip(&levels)
        .filter(|(_, l)| l.peak_left >= 1.0)
        .map(|(t, _)| t.guid.clone())
        .collect();
    Meters { levels, clipped }
}

/// The kick's Signal chains, every block an insert, with the states a
/// block can be in: the kick's compressor bypassed and its Rescue phase
/// off, Out's saturator in error, Sub's band-pass unavailable and its gate
/// keyed from its own input. In is shown shut, one insert for the chain.
///
/// The fixture is compiled in and `mixer_view`'s own tests hold it to
/// parsing; were it ever not to, the kick has no Signal chains (and the
/// golden session's tests say so).
#[must_use]
pub fn signal() -> (Vec<(String, Chain)>, Vec<String>) {
    let mut kit = mixer_view::signal::fixtures::kick_kit().unwrap_or_default();
    for (path, chain) in &mut kit {
        match path.as_str() {
            "Drums/Kick" => {
                block(chain, "tone.comp", |b| b.bypass = true);
                if let Some(p) = chain.phases.iter_mut().find(|p| p.id.as_str() == "rescue") {
                    p.bypass = true;
                }
            }
            "Drums/Kick/Sum/Out" => block(chain, "tone.sat", |b| b.status = Status::Error),
            "Drums/Kick/Sub" => {
                block(chain, "tone.band_pass", |b| b.status = Status::Unavailable);
                block(chain, "tone.gate", |b| {
                    if let Some(key) = &mut b.key {
                        key.via = Via::OwnInput;
                    }
                });
            }
            _ => {}
        }
    }
    (kit, vec!["Drums/Kick/Sum/In".to_owned()])
}

/// Change the block `id` of `chain`, if it has one.
fn block(chain: &mut Chain, id: &str, change: impl FnOnce(&mut Block)) {
    if let Some(b) = chain
        .phases
        .iter_mut()
        .flat_map(|p| &mut p.blocks)
        .find(|b| b.id.as_str() == id)
    {
        change(b);
    }
}

//! A demo catalog to design and test surfaces against without a rig.

use crate::surface::proto::*;

/// A small guitar rig: an amp, a drive, a delay, a reverb, stomp actions,
/// meters and a tuner — enough to design and test real surfaces against.
#[must_use]
pub fn demo_catalog() -> Catalog {
    let db = |min, max| Range::linear(min, max, Unit::Db);
    let pct = Range::linear(0.0, 100.0, Unit::Percent);
    let param = |addr: &str, name: &str, group: &str, range: Range, default: f64| {
        ParamInfo::new(addr, name, range)
            .in_group(group)
            .with_default_real(default)
    };
    Catalog {
        id: "demo.guitar".into(),
        version: 1,
        params: vec![
            param(
                "amp/gain",
                "Gain",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/bass",
                "Bass",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/mid",
                "Mid",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/treble",
                "Treble",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param(
                "amp/presence",
                "Presence",
                "Amp",
                Range::linear(0.0, 10.0, Unit::None),
                5.0,
            ),
            param("amp/master", "Master", "Amp", db(-60.0, 6.0), -12.0),
            ParamInfo::new("amp/channel", "Channel", Range::default())
                .in_group("Amp")
                .choice(&["Clean", "Crunch", "Lead"]),
            param("drive/amount", "Drive", "Drive", pct.clone(), 40.0),
            param("drive/tone", "Tone", "Drive", pct.clone(), 50.0),
            ParamInfo::new("drive/on", "Drive On", Range::default())
                .in_group("Drive")
                .toggle()
                .short("Drive"),
            param(
                "delay/time",
                "Time",
                "Delay",
                Range::log(20.0, 2000.0, Unit::Milliseconds),
                350.0,
            ),
            param("delay/feedback", "Feedback", "Delay", pct.clone(), 30.0),
            param("delay/mix", "Mix", "Delay", pct.clone(), 20.0),
            ParamInfo::new("delay/on", "Delay On", Range::default())
                .in_group("Delay")
                .toggle()
                .short("Delay"),
            param(
                "reverb/decay",
                "Decay",
                "Reverb",
                Range::log(0.2, 10.0, Unit::Seconds),
                2.5,
            ),
            param(
                "reverb/tone",
                "Tone",
                "Reverb",
                Range::log(500.0, 12_000.0, Unit::Hz),
                4000.0,
            ),
            param("reverb/mix", "Mix", "Reverb", pct, 25.0),
            ParamInfo::new("reverb/on", "Reverb On", Range::default())
                .in_group("Reverb")
                .toggle()
                .short("Reverb"),
            param("rig/volume", "Volume", "Output", db(-60.0, 6.0), 0.0),
            ParamInfo {
                bipolar: true,
                ..param(
                    "rig/pan",
                    "Pan",
                    "Output",
                    Range::linear(-100.0, 100.0, Unit::Percent),
                    0.0,
                )
            },
        ],
        actions: vec![
            ActionInfo {
                id: EntryId::new("tap_tempo"),
                name: "Tap Tempo".into(),
                group: Some("Global".into()),
            },
            ActionInfo {
                id: EntryId::new("next_song"),
                name: "Next Song".into(),
                group: Some("Setlist".into()),
            },
            ActionInfo {
                id: EntryId::new("prev_song"),
                name: "Previous Song".into(),
                group: Some("Setlist".into()),
            },
            ActionInfo {
                id: EntryId::new("tuner"),
                name: "Tuner".into(),
                group: Some("Global".into()),
            },
        ],
        streams: vec![
            StreamInfo {
                id: EntryId::new("out_level"),
                name: "Output".into(),
                kind: StreamKind::Level { channels: 2 },
                rate_hz: 30.0,
            },
            StreamInfo {
                id: EntryId::new("in_level"),
                name: "Input".into(),
                kind: StreamKind::Level { channels: 1 },
                rate_hz: 30.0,
            },
            StreamInfo {
                id: EntryId::new("tuner"),
                name: "Tuner".into(),
                kind: StreamKind::Tuner,
                rate_hz: 30.0,
            },
            StreamInfo {
                id: EntryId::new("tempo"),
                name: "Tempo".into(),
                kind: StreamKind::Scalar {
                    range: Range::linear(40.0, 240.0, Unit::Bpm),
                },
                rate_hz: 4.0,
            },
        ],
        collections: vec![
            CollectionInfo {
                id: EntryId::new("songs"),
                name: "Songs".into(),
            },
            CollectionInfo {
                id: EntryId::new("presets"),
                name: "Presets".into(),
            },
        ],
    }
}

//! The golden session's automation: volume, pan, width, mute and a plugin
//! parameter on one track, each lane in its own mode, the points in every
//! shape the lanes draw, a point selected and an automation item.

use daw_proto::automation::{
    AutomationItem, EnvelopePoint, EnvelopeRef, EnvelopeShape, EnvelopeType,
};
use daw_proto::primitives::{AutomationMode, Duration, PositionInSeconds};
use daw_view_kit::automation::{Kind, Lane};

/// The track the lanes are on.
const TRACK: &str = "Automation";

/// Every point shape, in turn.
const SHAPES: [EnvelopeShape; 6] = [
    EnvelopeShape::Linear,
    EnvelopeShape::Square,
    EnvelopeShape::SlowStartEnd,
    EnvelopeShape::FastStart,
    EnvelopeShape::FastEnd,
    EnvelopeShape::Bezier,
];

/// The lanes, by the guid of the track they are on.
#[must_use]
pub fn lanes(tracks: &[daw_proto::Track]) -> Vec<(String, Vec<Lane>)> {
    let Some(track) = tracks.iter().find(|t| t.name == TRACK) else {
        return Vec::new();
    };
    let guid = track.guid.clone();
    let lanes = vec![
        lane(
            &guid,
            Target::Volume,
            AutomationMode::Read,
            &[0.25, 1.0, 1.6, 0.5, 1.0],
        ),
        lane(
            &guid,
            Target::Pan,
            AutomationMode::Touch,
            &[0.5, 0.0, 1.0, 0.5, 0.2],
        ),
        lane(
            &guid,
            Target::Width,
            AutomationMode::Latch,
            &[1.0, 0.4, 0.8, 0.0, 1.0],
        ),
        lane(
            &guid,
            Target::Mute,
            AutomationMode::Write,
            &[0.0, 1.0, 0.0, 1.0, 0.0],
        ),
        with_item(lane(
            &guid,
            Target::Param,
            AutomationMode::TrimRead,
            &[0.2, 0.9, 0.3, 0.7, 0.5],
        )),
    ];
    vec![(guid, lanes)]
}

/// What a lane automates.
#[derive(Clone, Copy)]
enum Target {
    Volume,
    Pan,
    Width,
    Mute,
    Param,
}

impl Target {
    fn envelope(self) -> EnvelopeRef {
        match self {
            Self::Volume => EnvelopeRef::Type(EnvelopeType::Volume),
            Self::Pan => EnvelopeRef::Type(EnvelopeType::Pan),
            Self::Width => EnvelopeRef::Type(EnvelopeType::Width),
            Self::Mute => EnvelopeRef::Type(EnvelopeType::Mute),
            Self::Param => EnvelopeRef::FxParam {
                fx_guid: "Automation/fx/eq".to_owned(),
                param_index: 3,
            },
        }
    }

    const fn kind(self) -> Kind {
        match self {
            Self::Volume => Kind::Volume,
            Self::Pan => Kind::Pan,
            Self::Width => Kind::Width,
            Self::Mute => Kind::Mute,
            Self::Param => Kind::Param,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Pan => "Pan",
            Self::Width => "Width",
            Self::Mute => "Mute",
            Self::Param => "Gain / EQ",
        }
    }
}

/// One lane: `values` spread over the first 30 seconds, each point taking
/// the next shape (a mute lane's are all steps), the second selected.
fn lane(track: &str, target: Target, mode: AutomationMode, values: &[f64]) -> Lane {
    let kind = target.kind();
    let points = points(kind, values, 0.0, 30.0);
    let mut lane = Lane {
        track: track.to_owned(),
        target: target.envelope(),
        name: target.name().to_owned(),
        kind,
        height: 48.0,
        mode,
        points,
        items: Vec::new(),
        played: Vec::new(),
        rest: daw_view_kit::automation::neutral(kind),
        fx_name: matches!(target, Target::Param).then(|| "EQ".to_owned()),
        leveled: Vec::new(),
        choices: Vec::new(),
    };
    lane.replay();
    lane
}

/// `values` as points from `from` to `to` seconds, in turn through the
/// shapes, the second selected.
fn points(kind: Kind, values: &[f64], from: f64, to: f64) -> Vec<EnvelopePoint> {
    let gaps = f64::from(
        u32::try_from(values.len().saturating_sub(1))
            .unwrap_or(1)
            .max(1),
    );
    values
        .iter()
        .zip(0_u32..)
        .zip(SHAPES.iter().cycle())
        .map(|((value, index), shape)| EnvelopePoint {
            index,
            time: PositionInSeconds::from_seconds(from + (to - from) * f64::from(index) / gaps),
            value: *value,
            shape: if matches!(kind, Kind::Mute) {
                EnvelopeShape::Square
            } else {
                *shape
            },
            tension: if matches!(shape, EnvelopeShape::Bezier) {
                0.6
            } else {
                0.0
            },
            selected: index == 1,
        })
        .collect()
}

/// `lane` with an automation item over 12 to 20 seconds, a wave of its own.
fn with_item(mut lane: Lane) -> Lane {
    let item = AutomationItem {
        name: "LFO".to_owned(),
        position: PositionInSeconds::from_seconds(12.0),
        length: Duration::from_seconds(8.0),
        ..AutomationItem::default()
    };
    let inside = points(lane.kind, &[0.1, 0.9, 0.1, 0.9, 0.1], 0.0, 8.0);
    lane.items.push((item, inside));
    lane.replay();
    lane
}

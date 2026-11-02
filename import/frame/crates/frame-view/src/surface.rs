//! The surface `frame-surface` shows: a knob and a meter from a Frame
//! document (`surfaces/knob-meter.frame`), on Frame's own embed runtime,
//! painted into an [`Svg`].

use std::collections::BTreeMap;

use frame::embed::{DocumentView, Embed, EmbedHost};
use frame::surface::proto::{
    Catalog, FeedKind, ParamAddr, ParamInfo, ParamValue, Range, StreamFrame, StreamInfo, Unit,
};
use frame::surface::runtime::Surface;
use kurbo::Affine;

use crate::svg::{Svg, SvgCanvas};

/// The document: a knob bound to [`KNOB`], a meter to [`LEVEL`].
pub const DOCUMENT: &str = include_str!("../surfaces/knob-meter.frame");

/// The knob's param.
pub const KNOB: &str = "surface/drive";

/// The meter's feed (a level, one channel).
pub const LEVEL: &str = "level";

/// The meter's span: the runtime's default scale, so a meter at `m`
/// (0..1) reads `FLOOR_DB + m × (CEILING_DB − FLOOR_DB)`.
const FLOOR_DB: f64 = -60.0;
const CEILING_DB: f64 = 6.0;

/// How long between two meter readings, ms: long enough that the last
/// one's bar and peak have fallen away, so the meter shows exactly the
/// new one.
const READING_MS: f64 = 1_000_000.0;

/// What the surface's controls bind to.
#[must_use]
pub fn catalog() -> Catalog {
    Catalog {
        id: crate::KIND.to_owned(),
        version: 1,
        params: vec![
            ParamInfo::new(KNOB, "Drive", Range::linear(0.0, 100.0, Unit::Percent))
                .with_default_real(50.0),
        ],
        streams: vec![StreamInfo::new(
            LEVEL,
            "Level",
            FeedKind::Level { channels: 1 },
        )],
        ..Catalog::default()
    }
}

/// The knob and the meter, each 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State {
    pub knob: f64,
    pub meter: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            knob: 0.5,
            meter: 0.6,
        }
    }
}

/// The surface, laid out `width` px wide.
pub struct KnobMeter {
    embed: Embed,
    state: State,
    /// When the meter's last reading was, ms.
    t: f64,
}

impl std::fmt::Debug for KnobMeter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnobMeter")
            .field("state", &self.state)
            .field("size", &self.embed.size())
            .finish_non_exhaustive()
    }
}

impl KnobMeter {
    /// The surface at `width` px, at [`State::default`].
    ///
    /// # Errors
    ///
    /// The document does not parse.
    pub fn new(width: f64) -> Result<Self, String> {
        let scene =
            frame::document::text::parse(DOCUMENT, BTreeMap::new()).map_err(|e| e.to_string())?;
        let view = DocumentView::new(Surface::from_scene(scene), catalog(), EmbedHost::Strip);
        let mut embed = Embed::new(view, EmbedHost::Strip);
        let height = embed.height_for(width).unwrap_or(width);
        embed.set_size(width, height);
        let mut surface = Self {
            embed,
            state: State::default(),
            t: 0.0,
        };
        surface.set(State::default());
        Ok(surface)
    }

    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    /// Moves the knob and the meter (each clamped to 0..1).
    pub fn set(&mut self, state: State) {
        let state = State {
            knob: state.knob.clamp(0.0, 1.0),
            meter: state.meter.clamp(0.0, 1.0),
        };
        self.embed.apply_values([ParamValue {
            addr: ParamAddr::new(KNOB),
            value: state.knob,
        }]);
        self.t += READING_MS;
        self.embed.apply_frame(
            StreamFrame {
                id: frame::surface::proto::EntryId::new(LEVEL),
                values: vec![FLOOR_DB + state.meter * (CEILING_DB - FLOOR_DB)],
            },
            self.t,
        );
        self.state = state;
    }

    /// The knob's value as the document shows it (its engineering units).
    #[must_use]
    pub fn knob_real(&self) -> Option<f64> {
        self.embed.value(KNOB)
    }

    /// The surface painted now.
    pub fn svg(&mut self) -> Svg {
        let (w, h) = self.embed.size();
        let mut canvas = SvgCanvas::new();
        self.embed.paint(&mut canvas, Affine::IDENTITY, self.t);
        canvas.finish(w, h)
    }
}

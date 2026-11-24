//! The live pass, over the recorded one.
//!
//! Three things cannot be recorded with the strips: the control under
//! the pointer (it changes with the mouse), the meters (they change
//! with the audio), and the play cursor (it changes with the clock).
//! All three are the same shape of problem — something live over
//! something recorded — so they are one pass.
//!
//! Drawing them here costs a handful of shapes a frame. Re-recording
//! the mixer to show a hover would cost the whole mixer, which is the
//! thing the recorded-scene architecture exists to avoid: a hover is
//! the most frequent event a window gets.

use anyrender::PaintScene;
use daw_proto::Track;
use daw_ui::art::paint::tcp as art;
use vello::kurbo::Affine;

use crate::palette::Palette;
use crate::text::Font;

/// The routing widget's state, from the track.
///
/// All three lanes are real now. Sends and receives used to draw as
/// unlit slots whatever the session did, because the counts were not on
/// `Track` and a strip cannot afford to ask the routing service per
/// track — so a session full of sends showed none, which is a lane
/// saying the opposite of the truth rather than saying nothing.
///
/// The widget wants to know WHETHER, not how many: it lights a lane. A
/// strip that drew the number would be drawing a thing that changes
/// under it for reasons it has no other way to see.
#[must_use]
pub const fn routes(track: &Track) -> art::Routing {
    art::Routing {
        parent_send: track.parent_send,
        sends: track.send_count > 0,
        receives: track.receive_count > 0,
    }
}

/// The colours the routing lanes light in.
///
/// The source art's own choices: the output lane is the accent, sends
/// the warn amber, receives the danger red.
#[must_use]
pub fn route_ink(palette: &Palette) -> art::RouteInk {
    art::RouteInk {
        out: crate::tcp::to_theme(palette.accent),
        send: crate::tcp::to_theme(palette.meter_warn),
        recv: crate::tcp::to_theme(palette.meter_danger),
    }
}

/// One part a value moves: its drawing, where it is cut (in the pane),
/// and what moves it.
#[derive(Clone)]
pub struct ValueLayer {
    pub scene: anyrender::Scene,
    pub clip: Option<vello::kurbo::Rect>,
    pub moves: Vec<ValueMove>,
    /// What places it in the pane, when not [`Apart::transform`].
    pub transform: Option<Affine>,
    /// Over [`Apart::above`] (the caps), not under it.
    pub over: bool,
}

/// `property` of a [`ValueLayer`], from value `name`: `scale × value +
/// offset`, clamped to `[min, max]`. `Visible` shows it at 0.5 or more.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueMove {
    pub property: ValueProperty,
    pub name: String,
    pub scale: f64,
    pub offset: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueProperty {
    /// Down, in the drawing's own units.
    Y,
    Opacity,
    Visible,
}

impl ValueMove {
    #[must_use]
    pub fn new(property: ValueProperty, name: &str, scale: f64, offset: f64) -> Self {
        Self {
            property,
            name: name.to_owned(),
            scale,
            offset,
            min: None,
            max: None,
        }
    }

    #[must_use]
    pub const fn clamped(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
}

/// A rectangle filled in `color`, as a scene.
pub fn filled(color: vello::peniko::Color, rect: vello::kurbo::Rect) -> anyrender::Scene {
    let mut scene = anyrender::Scene::new();
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &rect,
    );
    scene
}

/// A track's input-monitoring mode, as the art's own three states.
///
/// Mapped here so the drawing does not have to know what a `daw_proto`
/// track is — the art speaks in what it draws, not in what the engine
/// calls it.
/// The colour the monitor lamp lights in.
///
/// Not the arm's red: the arm already says "recording", and a second
/// red lamp under it read as a second arm. Through is a light grey —
/// a lamp that is on — and the tape-style mode, which is the one you
/// set deliberately, is the one that gets a colour.
pub fn monitor_lit(palette: &Palette, mode: art::Monitoring) -> daw_ui::theme::Color {
    match mode {
        art::Monitoring::NotWhenPlaying => TAPE,
        art::Monitoring::Off | art::Monitoring::Normal => crate::tcp::to_theme(palette.text),
    }
}

/// The tape-style monitoring colour: an orange, so it is neither the
/// arm's red nor the grey of monitoring straight through.
pub const TAPE: daw_ui::theme::Color = daw_ui::theme::Color {
    r: 0xf0,
    g: 0x8c,
    b: 0x2e,
    a: 0xff,
};

pub fn monitoring(track: &Track) -> art::Monitoring {
    use daw_proto::track::InputMonitoringMode as M;
    match track.input_monitor {
        M::Off => art::Monitoring::Off,
        M::Normal => art::Monitoring::Normal,
        M::NotWhenPlaying => art::Monitoring::NotWhenPlaying,
    }
}

/// The track's name on its plate.
///
/// Centred, because a plate is a nameplate and a name pinned to the
/// left of a wide one sits in the corner of an empty box.
///
/// The ink carries three states the name is the biggest target for.
/// Muted dims it towards the plate it sits on — a muted track is one
/// you are not hearing, and it should look like it from across forty
/// strips rather than requiring you to find a ten-pixel button. Soloed
/// takes the solo colour, because a solo is the loudest thing about a
/// session and the name should say so. Selected takes full text and an
/// accent rule along the plate's top edge, which marks the whole strip
/// below it rather than just the label.
pub fn name(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    text: &str,
    size: f32,
    left: f64,
    plate: vello::kurbo::Rect,
) {
    if track.selected {
        crate::art::place(
            scene,
            &art::name_selected(
                crate::tcp::to_theme(palette.accent),
                plate.width() - 4.0,
                plate.height(),
            ),
            font,
            left + plate.x0 + 2.0,
            plate.y0,
        );
    }
    // Solo before mute: a track can be both, and what you are hearing
    // is decided by the solo.
    let ink = if track.soloed {
        palette.solo
    } else if track.muted {
        palette.text_faint
    } else if track.selected {
        palette.text
    } else {
        palette.text_dim
    };
    let width = font.width(text, size);
    crate::tcp::glyphs(
        scene,
        font,
        ink,
        text,
        left + plate.x0 + (plate.width() - width) / 2.0,
        plate.y0 + plate.height() / 2.0 + f64::from(size) / 3.0,
        size,
    );
}

/// The fader's own value, in decibels, beside the cap that is setting
/// it.
///
/// Placed against the cap rather than at a fixed height, because the
/// thing it labels moves — and on the side away from the scale, so it
/// cannot be read as one of the meter's marks.
#[expect(
    clippy::too_many_arguments,
    reason = "a placement, and every part of it is one"
)]
pub fn readout(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    volume: f64,
    x: f64,
    y: f64,
    fader_w: f64,
    cap_y: f64,
    cap_h: f64,
) {
    use vello::kurbo::Rect;
    use vello::peniko::Fill;

    let label = volume_label(volume);
    const SIZE: f32 = 9.0;
    let width = font.width(&label, SIZE) + 6.0;
    let height = f64::from(SIZE) + 4.0;
    let left = x + fader_w + 3.0;
    let top = (y + cap_y + cap_h / 2.0 - height / 2.0).max(y);
    scene.fill(
        Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        palette.tcp_field,
        None,
        &Rect::new(left, top, left + width, top + height),
    );
    crate::tcp::glyphs(
        scene,
        font,
        palette.text,
        &label,
        left + 3.0,
        top + height - 4.0,
        SIZE,
    );
}

/// The fader's value as its readout prints it.
#[must_use]
pub fn volume_label(volume: f64) -> String {
    let db = art::fader_db(crate::tcp::volume_fraction(volume));
    // Silence has no logarithm and should not be given a number that
    // implies one.
    if volume <= 0.0 {
        "-inf".to_owned()
    } else if db >= 0.0 {
        format!("+{db:.1}")
    } else {
        format!("{db:.1}")
    }
}

/// How clear the window in the fader cap is.
///
/// Nearly all the way: the ring around it is solid and does not fade at
/// any setting, so the cap keeps its edges however bright the meter
/// behind it — and the middle can therefore afford to be glass rather
/// than a tint. See `paint::fader_cap_through`.
pub const CAP_GLASS: f64 = 0.88;

// A rack that moves is drawn by `controls`, over its own recording,
// alongside the meters and the level traces — one pass over the visible
// strips rather than a second one for the racks. It used to be its own
// function for the dragged strip alone, which stopped being a special
// case the moment a spectrum could move one too.

/// What a track's FX button should say.
///
/// `fx_count` is the only thing the track model knows about a chain, so
/// this answers the only question it can: is there one. Bypass is a
/// property of the chain rather than of the track, and claiming
/// `Active` for a chain that is entirely bypassed would be a lit button
/// over a track doing nothing — which is the failure the empty pill was
/// avoiding by never lighting at all.
#[must_use]
pub const fn chain(track: &Track) -> art::Chain {
    if track.fx_count == 0 {
        art::Chain::Empty
    } else {
        art::Chain::Active
    }
}

/// `button` (mute or solo, of track `index`) lit, as a part a value shows:
/// what a pane over the engine's values lights in the same frame as every
/// other pane drawing it.
pub fn lit_button(index: usize, part: &str, button: anyrender::Scene) -> ValueLayer {
    ValueLayer {
        scene: button,
        clip: None,
        moves: vec![ValueMove::new(
            ValueProperty::Visible,
            &crate::engine::track_value(index, part),
            1.0,
            0.0,
        )],
        transform: None,
        over: false,
    }
}

/// `track`'s index, when its buttons go apart (`apart`) and the engine
/// publishes them as values.
pub fn button_index(apart: bool, track: &Track) -> Option<usize> {
    usize::try_from(track.index)
        .ok()
        .filter(|i| apart && *i < crate::engine::VALUE_TRACKS)
}

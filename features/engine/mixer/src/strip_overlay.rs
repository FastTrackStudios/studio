//! The mixer strips' live pass, over the recorded strips: the control
//! under the pointer, the meters and the racks, drawn every frame without
//! re-recording the strips (see `crate::overlay`, the track panel's half).

use anyrender::PaintScene;
use daw_proto::Track;
use daw_ui::art::geometry::mcp as g;
use daw_ui::art::mixer_controls::Interaction;
use daw_ui::art::paint::tcp as art;
use vello::kurbo::Affine;

use crate::mcp::{Control, Mixer};
use crate::palette::Palette;
use crate::pointer::Spot;
use crate::text::Font;

use crate::overlay::*;

/// Everything the live pass needs about the racks.
///
/// Five parameters that travel together and mean nothing apart — the
/// settings a rack is drawn from, the levels and spectra that make one
/// move, the grip the pointer is on, and which panels the phase is
/// showing. Bundled because a function taking sixteen arguments is one
/// nobody can call correctly, not because they are conceptually one
/// thing.
pub struct Racks<'a> {
    pub settings: &'a crate::tone::Store,
    pub history: &'a mut std::collections::HashMap<String, crate::tone::Levels>,
    pub spectra: &'a mut std::collections::HashMap<String, crate::tone::Analyser>,
    /// The one grip the pointer is on, and the strip it is on.
    pub lit: Option<(usize, crate::tone::Grip)>,
    pub panels: &'a [crate::tone::Which],
    /// Which phases are folded shut, and whether that is one answer for
    /// the whole mixer or one per track.
    ///
    /// Synced is the default because the chain is read ACROSS: you fold
    /// Rescue away to compare everyone's Tone, and a mixer where each
    /// strip folded on its own would put a different processor at the
    /// same height on every track. Per-track is the setting for when
    /// you are working one track rather than comparing them.
    pub folded: &'a crate::tone::Fold,
    /// Every Signal FX the mixer shows (`crate::signal`): a track with one
    /// shows its phases and blocks in its rack box, as inserts. `None`:
    /// no track does.
    pub signal: Option<&'a mut crate::signal::Inserts>,
}

impl Racks<'_> {
    /// Which phases are shut on a given track.
    fn folded(&self, guid: &str) -> crate::tone::Folded {
        self.folded.of(guid)
    }
}

impl Racks<'_> {
    /// No racks at all — for the measurement shots, which are of a
    /// mixer at rest.
    #[must_use]
    pub fn none() -> Racks<'static> {
        // Leaked once, for the life of the process, so a `&mut` can be
        // handed out without a caller having to own the maps. This is
        // only reached by the shot path, which runs once.
        static EMPTY_SETTINGS: std::sync::OnceLock<crate::tone::Store> = std::sync::OnceLock::new();
        static EMPTY_FOLD: std::sync::OnceLock<crate::tone::Fold> = std::sync::OnceLock::new();
        Racks {
            folded: EMPTY_FOLD.get_or_init(crate::tone::Fold::default),
            settings: EMPTY_SETTINGS.get_or_init(crate::tone::Store::default),
            history: Box::leak(Box::default()),
            spectra: Box::leak(Box::default()),
            lit: None,
            panels: &[],
            signal: None,
        }
    }
}

/// Redraw one control in its hover or pressed state.
///
/// `transform` is the mixer's own — the same one the strips are
/// replayed under — so the overlay lands on top of the control it is
/// replacing rather than beside it.
pub fn control(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    mixer: &Mixer,
    track: &Track,
    spot: Spot,
    state: Interaction,
    rack_h: f64,
    transform: Affine,
) {
    let (Some((left, _, _)), Some(strip)) = (mixer.strip_box(spot.row), mixer.strip(spot.row))
    else {
        return;
    };
    // The SAME layout the strip was drawn from — the mixer's own
    // `Strip` for the row. This used to work its own positions out of
    // `Columns`, a `Collapse` and `buttons_top` — three recomputations
    // of what `Strip` already resolves once, and the hover cell drifted
    // away from the resting one every time the column moved.
    let _ = rack_h;
    let at = |control: Control| strip.rect(control).map(|r| (left + r.x0, r.y0));

    // Recorded in content space and replayed under the transform, so
    // the overlay records the same way and rides the same transform.
    let mut scene = anyrender::Scene::new();
    match spot.control {
        Control::Mute | Control::Solo => {
            let (label, on, lit) = if spot.control == Control::Mute {
                ("M", track.muted, crate::tcp::mute_lit(palette))
            } else {
                ("S", track.soloed, crate::tcp::solo_lit(palette))
            };
            if let Some(r) = strip.rect(spot.control) {
                crate::art::place(
                    &mut scene,
                    &art::gutter_button_sized(
                        &palette.chrome,
                        label,
                        on,
                        lit,
                        state,
                        (r.width(), r.height()),
                    ),
                    font,
                    left + r.x0,
                    r.y0,
                );
            }
        }
        Control::Monitor => {
            if let Some((x, y)) = at(Control::Monitor).filter(|_| track.armed) {
                crate::art::place(
                    &mut scene,
                    &art::monitor(
                        &palette.chrome,
                        monitoring(track),
                        monitor_lit(palette, monitoring(track)),
                        state,
                        art::Facing::Up,
                    ),
                    font,
                    x,
                    y,
                );
            }
        }
        Control::RecArm => {
            crate::art::place(
                &mut scene,
                &art::record_arm(
                    &palette.chrome,
                    crate::tcp::lit(palette).rec,
                    track.armed,
                    state,
                    strip.arm(),
                    crate::tcp::to_theme(crate::controls::strip_ground(palette, track)),
                ),
                font,
                at(Control::RecArm).map_or(left, |(x, _)| x),
                at(Control::RecArm).map_or(0.0, |(_, y)| y),
            );
        }
        Control::Pan => {
            crate::art::place(
                &mut scene,
                &art::pan_knob(
                    &palette.chrome,
                    track.pan.clamp(-1.0, 1.0),
                    crate::tcp::to_theme(palette.pan),
                    state,
                ),
                font,
                at(Control::Pan).map_or(left, |(x, _)| x),
                at(Control::Pan).map_or(0.0, |(_, y)| y),
            );
        }
        Control::Fx => {
            crate::art::place(
                &mut scene,
                &art::fx_pill(
                    &palette.chrome,
                    crate::tcp::lit(palette),
                    chain(track),
                    state,
                ),
                font,
                left + 7.0,
                rack_h + f64::from(g::FX_PILL_TOP),
            );
        }
        // The fader, the name plate and the routing widget have no
        // hover cell in the traced art — REAPER does not light them
        // either. They are still hit-testable and still draggable; they
        // just do not change appearance under the pointer.
        //
        // Nor does the clip latch: it is drawn by the meter, which
        // knows whether it is lit, and a hover cell for it here would
        // be a second opinion about that.
        Control::Volume | Control::Name | Control::Routing | Control::Clip | Control::Folder => {}
    }

    for command in &scene.commands {
        crate::palette::submit_command(painter, command, transform);
    }
}

/// Every control whose VALUE can change, drawn live for the strips on
/// screen.
///
/// The recorded strip holds what does not move: its ground, its colour
/// bands, its name, its rack, the fader's groove. What moves — the
/// fader's cap and lit travel, the pan knob's pointer, mute, solo, the
/// record arm — is drawn here, from the tracks as they are NOW.
///
/// This is what makes a parameter a reflection of state rather than a
/// picture taken at project open. Re-recording the mixer to show a
/// changed mute would cost the whole mixer for one rectangle; drawing
/// the controls live costs a handful of shapes per VISIBLE strip, which
/// is a few dozen strips however large the session is.
///
/// The division is the same one the profiling found worth making
/// everywhere in this app: record what is expensive and constant,
/// re-emit what is cheap and varying.
pub fn controls(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    mixer: &Mixer,
    tracks: &[Track],
    live: &crate::plan::Rows,
    pointer: &crate::pointer::Pointer,
    levels: &[daw_proto::TrackLevels],
    clipped: &Clips,
    rack_scroll: f64,
    racks: &mut Racks<'_>,
    scroll_x: f64,
    width: f64,
    transform: Affine,
    mut apart: Option<&mut Apart>,
) -> crate::profile::Counts {
    let mut counts = crate::profile::Counts::default();
    let mut scene = anyrender::Scene::new();
    if let Some(apart) = apart.as_deref_mut() {
        apart.transform = transform;
    }
    for row in mixer.visible(scroll_x, width) {
        let Some(track) = live.live(tracks, row) else {
            continue;
        };
        let Some((left, strip_w, strip_h)) = mixer.strip_box(row) else {
            continue;
        };
        draw_strip_controls(
            &mut scene,
            palette,
            font,
            track,
            mixer.label(row),
            rack_scroll,
            racks.folded(&track.guid),
            pointer,
            // Levels are indexed by PROJECT track index, not by mixer
            // row: the mixer shows a subset in its own order, and a
            // meter reading another track's level is worse than one
            // reading none.
            usize::try_from(track.index)
                .ok()
                .and_then(|i| levels.get(i))
                .copied(),
            clipped.is(&track.guid),
            racks.history.get_mut(&track.guid),
            racks.spectra.get_mut(&track.guid),
            racks.lit.filter(|(at, _)| *at == row).map(|(_, grip)| grip),
            racks.settings.get(&track.guid),
            racks.panels,
            row,
            left,
            strip_w,
            strip_h,
            mixer.rack_h,
            mixer.buttons_top,
            mixer.height,
            mixer.live,
            mixer.touch,
            mixer.themed(row).is_some(),
            apart.as_deref_mut(),
        );
        // A Signal FX's blocks, as inserts, over the rack box.
        if let (Some(inserts), Some(rack)) = (
            racks.signal.as_deref_mut(),
            mixer.strip(row).and_then(|s| s.rack_rect()),
        ) && let Some(strip) = inserts.strip_mut(&track.guid)
        {
            crate::signal::paint::over_rack(
                &mut scene,
                palette,
                font,
                strip,
                crate::tone::Panel::of(rack, left),
                rack_scroll,
            );
        }
        // A theme's strip: its live parts, over its recording.
        if let Some(theme) = mixer.themed(row) {
            let on = |spot: Option<Spot>| spot.filter(|s| s.row == row).map(|s| s.control);
            let fields = crate::skin::fields(
                palette,
                track,
                crate::skin::Shown {
                    level: usize::try_from(track.index)
                        .ok()
                        .and_then(|i| levels.get(i))
                        .copied(),
                    clipped: clipped.is(&track.guid),
                    hover: on(pointer.hovered()),
                    pressed: on(pointer.pressed()),
                    ..mixer.shown(row, font)
                },
            );
            theme.live(
                crate::skin::STRIP,
                (strip_w, strip_h - mixer.rack_h),
                &fields,
                &mut crate::skin::canvas::Host {
                    scene: &mut scene,
                    font,
                },
                Affine::translate((left, mixer.rack_h)),
            );
        }
        // The racks of the strips that are not selected, darkened —
        // over the recording and everything live on it, so the
        // selected one's is the bright one. The rack only: the strip's
        // own controls are the mixer's grey already and stay readable
        // across every track. A setting, and off when the mixer has no
        // selection at all: then there is nothing to be brighter than.
        let dim = crate::layout::dim_unselected();
        if dim > 0.0 && mixer.rack_h > 0.0 && !track.selected && live.any_selected(tracks) {
            scene.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                vello::peniko::Color::from_rgba8(0, 0, 0, 0xff).multiply_alpha(dim),
                None,
                &vello::kurbo::Rect::new(left, 0.0, left + strip_w, mixer.rack_h),
            );
        }
    }
    for command in &scene.commands {
        counts.replayed = counts.replayed.saturating_add(1);
        if crate::palette::submit_command(painter, command, transform) {
            counts.submitted = counts.submitted.saturating_add(1);
        }
    }
    counts
}

#[expect(
    clippy::too_many_arguments,
    reason = "one strip's worth of geometry and state; grouping it into a \
              struct would be a struct that exists for this call alone"
)]
fn draw_strip_controls(
    scene: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    track: &Track,
    label: Option<(&str, f32)>,
    scroll: f64,
    folded: crate::tone::Folded,
    pointer: &crate::pointer::Pointer,
    level: Option<daw_proto::TrackLevels>,
    clipped: bool,
    history: Option<&mut crate::tone::Levels>,
    spectrum: Option<&mut crate::tone::Analyser>,
    lit: Option<crate::tone::Grip>,
    settings: Option<&crate::tone::Tone>,
    panels: &[crate::tone::Which],
    row: usize,
    left: f64,
    width: f64,
    height: f64,
    rack_h: f64,
    buttons_top: f64,
    mixer_h: f64,
    live: bool,
    touch: bool,
    // A theme draws the strip's own controls: only the rack is drawn here.
    themed: bool,
    // The meter's moving parts apart, for a host that moves them by
    // values ([`Apart`]); `None` (the window's): drawn here.
    mut apart: Option<&mut Apart>,
) {
    let strip = crate::strip::Strip::laid_out(
        width,
        height,
        mixer_h,
        rack_h,
        buttons_top,
        settings.is_some_and(crate::tone::Tone::wants_column),
        live,
    )
    .touched(touch);
    // Every position comes from the layout, translated by the strip's
    // left edge. Nothing here works out where a control goes.
    let at = |control| strip.rect(control).map(|r| (left + r.x0, r.y0));
    let state = |control| pointer.state(Spot { row, control });

    if !themed {
        if let Some((x, y)) = at(Control::Pan) {
            crate::art::place(
                scene,
                &art::pan_knob(
                    &palette.chrome,
                    track.pan.clamp(-1.0, 1.0),
                    crate::tcp::to_theme(palette.pan),
                    state(Control::Pan),
                ),
                font,
                x,
                y,
            );
        }

        // The FX button, whose state is the track's chain count — a live
        // value like any other, and one that used to be recorded as Empty
        // and stay Empty for the life of the window.
        // Across the strip: a wide pill, the label centred, not the track
        // panel's small one pinned to the left.
        if let Some(r) = strip.rect(Control::Fx) {
            crate::art::place(
                scene,
                &art::fx_pill_wide(
                    &palette.chrome,
                    crate::tcp::lit(palette),
                    chain(track),
                    state(Control::Fx),
                    r.width(),
                ),
                font,
                left + r.x0,
                r.y0,
            );
        }

        // Routing, which used to be recorded — and recorded four pixels
        // above where the hit test looked for it, because the two resolved
        // against different lines. One layout, one position.
        if let Some((x, y)) = at(Control::Routing) {
            crate::art::place(
                scene,
                &art::routing(
                    &palette.chrome,
                    // The mixer stacks the lanes; the track panel sets them
                    // in a row. That is the whole difference between the
                    // theme's two routing images.
                    art::Axis::Vertical,
                    routes(track),
                    route_ink(palette),
                    state(Control::Routing),
                ),
                font,
                x,
                y,
            );
        }

        if let Some((x, y)) = at(Control::RecArm) {
            crate::art::place(
                scene,
                &art::record_arm(
                    &palette.chrome,
                    crate::tcp::lit(palette).rec,
                    track.armed,
                    state(Control::RecArm),
                    strip.arm(),
                    crate::tcp::to_theme(crate::controls::strip_ground(palette, track)),
                ),
                font,
                x,
                y,
            );
        }

        // Input monitoring, directly OVER the arm, in the band — the two
        // are one decision made twice, what the track records and whether
        // you hear it while it does, so they stack; and over rather than
        // under so the column below the arm is mute, solo, routing with
        // no gap where a lamp is not lit.
        //
        // Only on an armed track: monitoring is a fact about recording,
        // and a lamp for it on a track that is not recording is a lamp
        // with nothing to report.
        if let Some((x, y)) = at(Control::Monitor).filter(|_| track.armed) {
            crate::art::place(
                scene,
                &art::monitor(
                    &palette.chrome,
                    monitoring(track),
                    monitor_lit(palette, monitoring(track)),
                    state(Control::Monitor),
                    art::Facing::Up,
                ),
                font,
                x,
                y,
            );
        }

        let values = button_index(apart.as_deref().is_some_and(|a| a.buttons), track);
        for (control, label, on, lit, part) in [
            (
                Control::Mute,
                "M",
                track.muted,
                crate::tcp::mute_lit(palette),
                "mute",
            ),
            (
                Control::Solo,
                "S",
                track.soloed,
                crate::tcp::solo_lit(palette),
                "solo",
            ),
        ] {
            // At the size the strip gives it: a touchscreen's are bigger.
            let Some(r) = strip.rect(control) else {
                continue;
            };
            let button = |on: bool| {
                art::gutter_button_sized(
                    &palette.chrome,
                    label,
                    on,
                    lit,
                    state(control),
                    (r.width(), r.height()),
                )
            };
            // Lit by the engine's value, over it unlit, where a host
            // lights it so.
            if let (Some(index), Some(apart)) = (values, apart.as_deref_mut()) {
                crate::art::place(scene, &button(false), font, left + r.x0, r.y0);
                let mut shown = anyrender::Scene::new();
                crate::art::place(&mut shown, &button(true), font, left + r.x0, r.y0);
                apart.layers.push(lit_button(index, part, shown));
                continue;
            }
            crate::art::place(scene, &button(on), font, left + r.x0, r.y0);
        }
    }

    // A rack that is MOVING is drawn whole, live, over its own
    // recording — the panel grounds are opaque, so it covers rather
    // than composites. A spectrum moves with the audio and a grip moves
    // with the hand; either makes the recording stale, and both are
    // rare enough that the rest of the mixer stays in the cheap path.
    //
    // Only where the rack shows something that moves at this size: a
    // panel wide enough for its analyser. A rail-row or curves-only rack
    // stays its recording — redrawn on every publish it cost a whole rack
    // per strip per frame, and a mixer of narrow strips a hundred racks a
    // frame for nothing anyone could see change.
    if let (Some(box_), Some(tone)) = (strip.rack_rect(), settings)
        && (lit.is_some() || (spectrum.is_some() && crate::tone::Rack::at(box_.width()).detailed()))
    {
        // Clipped to the box, because the chain is longer than the box
        // and scrolls inside it: without this the panels above the
        // scroll paint into the rails and the ones below paint over the
        // strip's own controls.
        scene.push_clip_layer(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            &vello::kurbo::Rect::new(left + box_.x0, box_.y0, left + box_.x1, box_.y1),
        );
        // The scroll is the panel's own `y` — see `tone::layout`. One
        // offset, applied where the rack is placed, so the drawing and
        // the hit test cannot hold different opinions about it.
        let at = crate::tone::Panel::of(box_, left).up(scroll);
        let paint = |meters: &crate::live::Meters| {
            let mut rack = anyrender::Scene::new();
            crate::tone::draw(
                &mut rack,
                palette,
                font,
                tone,
                meters,
                tone.panels(panels),
                at,
                folded,
                lit,
                crate::controls::rack_ground(palette, track),
            );
            rack
        };
        match spectrum {
            // A lit strip is rebuilt every frame on purpose: the grip
            // under the pointer moves with the hand, and there is only
            // ever one of them.
            Some(analyser) if lit.is_none() => {
                let meters = analyser.meters().clone();
                let cached = analyser.rack(at.width, at.height, || paint(&meters));
                scene.commands.extend_from_slice(&cached.commands);
            }
            Some(analyser) => {
                let meters = analyser.meters().clone();
                scene.commands.extend(paint(&meters).commands);
            }
            None => scene
                .commands
                .extend(paint(&crate::live::Meters::default()).commands),
        }
        scene.pop_layer();
    }

    // The compressor's level history, under the threshold line drawn
    // across it. Live, because it is the one part of a rack that
    // changes with the audio — see `tone::levels`.
    //
    // Clipped to the same box and moved by the same scroll as the rack
    // it belongs to. It is a second pass over the same panels, and a
    // pass that skipped either would draw a compressor's levels where
    // its compressor is not.
    if let (Some(box_), Some(history), Some(tone)) = (strip.rack_rect(), history, settings) {
        scene.push_clip_layer(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            &vello::kurbo::Rect::new(left + box_.x0, box_.y0, left + box_.x1, box_.y1),
        );
        crate::tone::levels(
            scene,
            tone.panels(panels),
            history,
            // The reduction is what each compressor is DOING, so it is
            // computed from the settings it is doing it with — which is
            // why the whole `Tone` goes in rather than one `Comp`.
            tone,
            crate::tone::Panel::of(box_, left).up(scroll),
            folded,
        );
        scene.pop_layer();
    }

    // The meter, which is the most live thing on the strip: thirty
    // frames a second of it, and the only control here whose value does
    // not come from the track at all.
    //
    // Drawn whole rather than as a lit column over a recorded well —
    // the well is one rounded rectangle, and keeping the two halves in
    // separate passes is how a meter ends up lit past its own edge when
    // the strip resizes.
    // The fader IS the meter: its groove carries the signal, and the
    // cap rides over it as glass so the level reads through the one
    // The track's name, live, because its ink answers to three things
    // that change while the window is open — selection, mute and solo —
    // and re-recording a mixer to dim one name is the trade this
    // architecture exists to refuse. The plate under it is recorded;
    // only the text on it is not.
    if themed {
        return;
    }
    if let (Some((text, size)), Some(plate)) = (label, strip.rect(Control::Name)) {
        name(scene, palette, font, track, text, size, left, plate);
    }

    // place it was always hidden — right where the hand is, which on a
    // loud track is right where the peak is.
    //
    // One column instead of two, which is what makes this fit a narrow
    // strip. It used to be a meter beside a fader, and on an 86-wide
    // strip there was no room for the meter, so there simply was none.
    if strip.has_fader()
        && let Some((x, y)) = at(Control::Volume)
    {
        let travel = strip.travel();
        // Each channel on its own half, not the louder of the two and
        // not their sum: a summed meter cannot tell you that a stereo
        // source has collapsed to one side or that one leg of a pair is
        // dead, and those are two of the things a meter is watched for.
        let peak = level.map_or((0.0, 0.0), |level| {
            (
                crate::engine::meter_fraction(level.peak_left),
                crate::engine::meter_fraction(level.peak_right),
            )
        });
        // Where the signal HAS been. The bar is sampled thirty times a
        // second and cannot show a transient; this is the reading that
        // answers "did that hit".
        let hold = level.map_or((0.0, 0.0), |level| {
            (
                crate::engine::meter_fraction(level.hold_left),
                crate::engine::meter_fraction(level.hold_right),
            )
        });
        // A host that moves the meter by values: its parts apart, for a
        // track the engine publishes values for (a track past them is
        // drawn here, as the window draws it).
        if let Some(apart) = apart
            .filter(|_| usize::try_from(track.index).is_ok_and(|i| i < crate::engine::METER_TRACKS))
        {
            meter_apart(
                apart,
                scene,
                (palette, font),
                track,
                &strip,
                (x, y, left),
                pointer.state(crate::pointer::Spot {
                    row,
                    control: Control::Volume,
                }) == daw_ui::art::mixer_controls::Interaction::Pressed,
                clipped,
            );
            return;
        }
        // The scale, against the meter and lighting with it.
        if let Some(scale) = strip.scale_rect() {
            crate::art::place(
                scene,
                &art::fader_scale(
                    scale.width(),
                    scale.height(),
                    crate::tcp::to_theme(palette.text_faint),
                    [
                        crate::tcp::to_theme(palette.meter_safe),
                        crate::tcp::to_theme(palette.meter_warn),
                        crate::tcp::to_theme(palette.meter_danger),
                    ],
                    8.0,
                    peak.0.max(peak.1),
                ),
                font,
                left + scale.x0,
                scale.y0,
            );
        }
        crate::art::place(
            scene,
            &art::fader_track(
                &palette.chrome,
                peak,
                hold,
                clipped,
                [
                    crate::tcp::to_theme(palette.meter_safe),
                    crate::tcp::to_theme(palette.meter_warn),
                    crate::tcp::to_theme(palette.meter_danger),
                ],
                strip.columns.fader_w,
                travel,
            ),
            font,
            x,
            y,
        );
        // The cap, centred in the column and narrower than the meter,
        // so a channel shows down each side of it whatever it covers.
        // Where `Strip::cap` puts it, which is also where a finger takes
        // hold of it.
        let Some(cap) = strip.cap(track.volume) else {
            return;
        };
        let (cap_y, cap_h) = (cap.y0 - y, cap.height());
        crate::art::scaled(
            scene,
            &art::fader_cap_through(&palette.chrome, palette.chrome.hardware_mark, CAP_GLASS),
            font,
            left + cap.x0,
            cap.y0,
            cap_h / 53.0,
        );
        // What the fader is SET to, while it is being set. The numbers
        // beside the column are the meter's, so without this the one
        // value you are actually changing is the one you have to infer
        // from where the cap sits. Only while dragging: a permanent
        // readout on forty strips is forty numbers nobody is reading.
        if pointer.state(crate::pointer::Spot {
            row,
            control: Control::Volume,
        }) == daw_ui::art::mixer_controls::Interaction::Pressed
        {
            readout(
                scene,
                palette,
                font,
                track.volume,
                x,
                y,
                strip.columns.fader_w,
                cap_y,
                cap_h,
            );
        }
        // And the level again where the cap crosses it, so the column
        // is continuous rather than interrupted at the one height you
        // are looking at.
        crate::art::place(
            scene,
            &art::fader_through(
                peak,
                [
                    crate::tcp::to_theme(palette.meter_safe),
                    crate::tcp::to_theme(palette.meter_warn),
                    crate::tcp::to_theme(palette.meter_danger),
                ],
                strip.columns.fader_w,
                travel,
                cap_y,
                cap_h,
            ),
            font,
            x,
            y,
        );
    }
}

/// The meters drawn apart, for a host that moves them by values (an
/// Architect pane over an engine's value table): what a playing mixer
/// changes, so the picture's own layer does not.
///
/// The picture keeps each meter fully lit (its gradient is pinned to the
/// column's height, so a lit column is the same picture at any level);
/// the parts here cover what is not lit, put the holds where they are and
/// light the scale, each moved by one of the engine's values
/// ([`crate::engine::meter_value`]). The cap, its readout and the clip
/// latch are drawn over all of it ([`Apart::above`]). Composed in order,
/// it is [`controls`]' picture at the levels the values hold.
#[derive(Default)]
pub struct Apart {
    pub layers: Vec<ValueLayer>,
    pub above: anyrender::Scene,
    /// What places all of it in the pane (as [`controls`]' own drawing).
    pub transform: Affine,
    /// The tracks' mute and solo lit apart too: the picture draws each
    /// unlit, and a part over it, lit, shows by the engine's value
    /// ([`crate::engine::track_value`]). Only where the values are there.
    pub buttons: bool,
}

/// The meter of `track`'s strip apart (see [`Apart`]): its still part into
/// `scene`, its moving parts into `apart`.
#[allow(clippy::too_many_arguments)]
fn meter_apart(
    apart: &mut Apart,
    scene: &mut anyrender::Scene,
    (palette, font): (&Palette, &Font),
    track: &Track,
    strip: &crate::strip::Strip,
    (x, y, left): (f64, f64, f64),
    pressed: bool,
    clipped: bool,
) {
    use vello::kurbo::Rect;
    let travel = strip.travel();
    let fader_w = strip.columns.fader_w;
    let zones = [
        crate::tcp::to_theme(palette.meter_safe),
        crate::tcp::to_theme(palette.meter_warn),
        crate::tcp::to_theme(palette.meter_danger),
    ];
    let Ok(index) = usize::try_from(track.index) else {
        return;
    };
    let value = |part: &str| crate::engine::meter_value(index, part);
    // The scale: each mark lit or not by the louder channel.
    if let Some(scale) = strip.scale_rect() {
        for (at, lit, unlit) in art::fader_scale_marks(
            scale.width(),
            scale.height(),
            crate::tcp::to_theme(palette.text_faint),
            zones,
            8.0,
        ) {
            for (drawing, gate) in [
                // Lit at the level or above it: `v - at + 0.5 >= 0.5`.
                (
                    lit,
                    ValueMove::new(ValueProperty::Visible, &value("pk"), 1.0, 0.5 - at),
                ),
                // Unlit below it.
                (
                    unlit,
                    ValueMove::new(ValueProperty::Visible, &value("pk"), -1.0, 0.5 + at),
                ),
            ] {
                let mut part = anyrender::Scene::new();
                crate::art::place(&mut part, &drawing, font, left + scale.x0, scale.y0);
                apart.layers.push(ValueLayer {
                    scene: part,
                    clip: None,
                    moves: vec![gate],
                    transform: None,
                    over: false,
                });
            }
        }
    }
    // The column as the window draws it with nothing lit: still. Each
    // channel's bar is a part, shown while it is lit (as the window draws
    // one only past half a pixel), with what is not lit covered.
    crate::art::place(
        scene,
        &art::fader_track_unlit(&palette.chrome, fader_w, travel),
        font,
        x,
        y,
    );
    let ([left_ch, right_ch], span) = art::meter_columns(fader_w, travel);
    let ground = crate::controls::strip_ground(palette, track);
    let hold_h = art::HOLD_LINE;
    // What is not lit: the strip's ground (a wash over the panel's
    // backdrop) across the whole groove out to whole pixels, so the lit
    // column's soft edges are covered too, with the groove's seam
    // drawn on it as the meter draws it (the seam is the same all the way
    // up, so raising it changes nothing). One cover a channel, each cut to
    // its half of the groove (to the middle of the seam) and raised by its
    // own level.
    let groove = Rect::new(
        (x + left_ch.0).floor(),
        y + 0.5,
        (x + right_ch.0 + right_ch.1).ceil(),
        y + 0.5 + span,
    );
    let seam_mid = x + (left_ch.0 + left_ch.1 + right_ch.0) / 2.0;
    // What a cover paints: the groove with nothing lit, as it is drawn.
    let cover_of = || {
        let mut cover = filled(palette.surface, groove);
        cover.fill(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            ground,
            None,
            &groove,
        );
        crate::art::place(
            &mut cover,
            &art::fader_track_unlit(&palette.chrome, fader_w, travel),
            font,
            x,
            y,
        );
        cover
    };
    // Each channel's half of the groove: from its edge to the middle of
    // the seam.
    let half_of = |channel: usize| {
        if channel == 0 {
            Rect::new(groove.x0, groove.y0, seam_mid, groove.y1)
        } else {
            Rect::new(seam_mid, groove.y0, groove.x1, groove.y1)
        }
    };
    for (channel, peak) in ["pl", "pr"].into_iter().enumerate() {
        let mut bar = anyrender::Scene::new();
        crate::art::place(
            &mut bar,
            &art::fader_track_bar(zones, fader_w, travel, channel),
            font,
            x,
            y,
        );
        apart.layers.push(ValueLayer {
            scene: bar,
            clip: None,
            // Shown past half a pixel: `span × v > 0.5`.
            moves: vec![ValueMove::new(
                ValueProperty::Visible,
                &value(peak),
                span,
                0.0,
            )],
            transform: None,
            over: false,
        });
        let half = half_of(channel);
        let cover = cover_of();
        apart.layers.push(ValueLayer {
            scene: cover,
            clip: Some(half),
            moves: vec![
                ValueMove::new(ValueProperty::Y, &value(peak), -span, 0.0).clamped(-span, 0.0),
            ],
            transform: None,
            over: false,
        });
    }
    for ((cx, cw), hold) in [left_ch, right_ch].into_iter().zip(["hl", "hr"]) {
        // The hold: a line where the signal has been, coloured as the
        // meter is at that height (the first zone, the second over it as
        // the hold rises to the knee, the third over that above it). Made
        // below the column, so with no value it is not seen.
        let park = hold_h;
        let line = Rect::new(
            x + cx,
            y + travel - 0.5 - hold_h / 2.0 + park,
            x + cx + cw,
            y + travel - 0.5 + hold_h / 2.0 + park,
        );
        // The highest a line goes: its top at the column's top.
        let top = 1.0 - travel + hold_h / 2.0 - park;
        let lift = ValueMove::new(ValueProperty::Y, &value(hold), -span, -park).clamped(top, 0.0);
        let shown = ValueMove::new(ValueProperty::Visible, &value(hold), 1.0, 0.499);
        let knee = art::ZONE_KNEE;
        for (color, fade) in [
            (palette.meter_safe, None),
            (palette.meter_warn, Some((1.0 / knee, 0.0))),
            (
                palette.meter_danger,
                Some((1.0 / (1.0 - knee), -knee / (1.0 - knee))),
            ),
        ] {
            let mut moves = vec![lift.clone(), shown.clone()];
            if let Some((scale, offset)) = fade {
                moves.push(
                    ValueMove::new(ValueProperty::Opacity, &value(hold), scale, offset)
                        .clamped(0.0, 1.0),
                );
            }
            apart.layers.push(ValueLayer {
                scene: filled(color, line),
                clip: Some(Rect::new(x + cx, y + 0.5, x + cx + cw, y + travel - 0.5)),
                moves,
                transform: None,
                over: false,
            });
        }
    }
    // The seam again over the bars, as the meter draws it over its own.
    let mut seam = anyrender::Scene::new();
    crate::art::place(
        &mut seam,
        &art::fader_track_over_seam(&palette.chrome, fader_w, travel),
        font,
        x,
        y,
    );
    apart.layers.push(ValueLayer {
        scene: seam,
        clip: None,
        moves: Vec::new(),
        transform: None,
        over: false,
    });
    // Over all of it: the clip latch, the cap and what it is set to.
    if clipped {
        crate::art::place(
            &mut apart.above,
            &art::fader_clip_latch(zones, fader_w, travel),
            font,
            x,
            y,
        );
    }
    let Some(cap) = strip.cap(track.volume) else {
        return;
    };
    let (cap_y, cap_h) = (cap.y0 - y, cap.height());
    crate::art::scaled(
        &mut apart.above,
        &art::fader_cap_through(&palette.chrome, palette.chrome.hardware_mark, CAP_GLASS),
        font,
        left + cap.x0,
        cap.y0,
        cap_h / 53.0,
    );
    // Through the cap's pane: the level seen through it, as the meter draws
    // it over the cap (its own gradient, part transparent). The window cuts
    // it at the level; here it is the pane's one-pixel rows, each its share
    // of that drawing, over the cap, as opaque as the lit part covers it:
    // `row's foot − lit top`, clamped to 0..1, which is linear in the level
    // (and the window's own soft edge where the level crosses a row).
    for (channel, peak) in ["pl", "pr"].into_iter().enumerate() {
        let (through, (from, to, pane_l, pane_r)) =
            art::fader_through_channel(zones, fader_w, travel, (cap_y, cap_h), channel);
        if to - from < 0.5 || pane_r - pane_l < 0.5 {
            continue;
        }
        let mut seen = anyrender::Scene::new();
        crate::art::place(&mut seen, &through, font, x, y);
        let (x0, x1) = ((x + pane_l).floor(), (x + pane_r).ceil());
        let mut row = (y + from).floor();
        while row < y + to {
            // The lit top in the drawing is `travel - 0.5 - span v`; the
            // row's foot `row + 1` (in the pane, less `y`).
            let foot = row + 1.0 - y;
            apart.layers.push(ValueLayer {
                scene: seen.clone(),
                clip: Some(Rect::new(x0, row, x1, row + 1.0)),
                moves: vec![
                    ValueMove::new(
                        ValueProperty::Opacity,
                        &value(peak),
                        span,
                        foot - travel + 0.5,
                    )
                    .clamped(0.0, 1.0),
                ],
                transform: None,
                over: true,
            });
            row += 1.0;
        }
    }
    if pressed {
        readout(
            &mut apart.above,
            palette,
            font,
            track.volume,
            x,
            y,
            fader_w,
            cap_y,
            cap_h,
        );
    }
}

/// Which tracks have clipped since anyone last looked.
///
/// By GUID rather than by index, for the reason everything per-track
/// here is: a preset can hide a track and a fold can move one, and a
/// clip latch that followed a row would report the neighbour's fault.
///
/// A latch rather than a reading: a peak-hold decays, which is right
/// for reading a level and wrong for reporting one. The whole value of
/// "this clipped" is that it is still saying so when you look up.
#[derive(Clone, Debug, Default)]
pub struct Clips {
    by_guid: std::collections::HashSet<String>,
}

impl Clips {
    /// Note whatever this frame's levels did.
    ///
    /// Linear 1.0 is 0 dBFS, which is where the sample ran out of
    /// numbers. Anything at or above it is an over.
    pub fn note(&mut self, guid: &str, level: daw_proto::TrackLevels) {
        let over = level.peak_left.max(level.peak_right) >= 1.0;
        if over && !self.by_guid.contains(guid) {
            self.by_guid.insert(guid.to_owned());
        }
    }

    #[must_use]
    pub fn is(&self, guid: &str) -> bool {
        self.by_guid.contains(guid)
    }

    /// Clear one track's latch — what clicking it does.
    ///
    /// Returns whether there was anything to clear, so a click that
    /// lands on an unlit latch can fall through to the fader under it
    /// rather than being swallowed.
    pub fn clear(&mut self, guid: &str) -> bool {
        self.by_guid.remove(guid)
    }

    /// And all of them, which is what a desk's "reset peaks" does.
    pub fn clear_all(&mut self) {
        self.by_guid.clear();
    }

    #[must_use]
    pub fn any(&self) -> bool {
        !self.by_guid.is_empty()
    }
}

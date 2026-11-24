//! The resolved theme as the views paint with it ([`Palette`]), the panel's
//! measured width, replaying a recorded scene ([`submit_command`]), and the
//! song's key at a time ([`key_at`]): what the arrangement, the mixer and
//! the panels around them all draw with.

use anyrender::PaintScene;
use anyrender::recording::RenderCommand;
use vello::kurbo::Affine;
use vello::peniko::Color;

/// The panel's width, from the measured REAPER geometry.
pub const TCP_WIDTH: f64 = 343.0;

/// Colours resolved once, so the draw loop never parses a hex string.
#[derive(Clone, PartialEq)]
pub struct Palette {
    pub surface: Color,
    pub row_a: Color,
    pub row_b: Color,
    pub divider: Color,
    pub grid: Color,
    /// The sub-bar lines, dimmer than the bar lines they sit between.
    pub grid_beat: Color,
    /// The grid's lines finer than a beat.
    pub grid_sub: Color,
    pub ruler_bg: Color,
    pub ruler_fg: Color,
    /// The ruler's quieter ink: beats and subdivisions.
    pub ruler_fg2: Color,
    pub item_edge: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    pub accent: Color,
    pub mute: Color,
    pub solo: Color,
    pub rec: Color,
    pub meter_safe: Color,
    pub meter_warn: Color,
    /// Pan's own colour — yellow, so it is not mistaken for volume's
    /// blue in the column beside it.
    pub pan: Color,
    pub meter_danger: Color,
    /// The track panel's own surfaces. Named `tcp_*` because they come
    /// from the theme's TCP context, which a REAPER theme colours
    /// separately from the arrange view — a panel drawn in arrange
    /// colours is the giveaway that a theme was only half applied.
    pub tcp_tint: Color,
    pub tcp_gutter: Color,
    pub tcp_column: Color,
    pub tcp_rule: Color,
    pub tcp_field: Color,
    pub tcp_button: Color,
    pub tcp_combo: Color,
    pub tcp_meter_well: Color,
    /// How strongly a track's colour tints its row, from the theme.
    pub track_tint: f32,
    /// The palette the ported control art is drawn against.
    ///
    /// `daw_theme_art`'s components reach for `Theme::default()`, which
    /// is right for the one theme they were drawn for and wrong here:
    /// this window opens whatever REAPER theme the user has, and taking
    /// the default put the track panel's buttons in a grey the rest of
    /// the window had moved away from. So the drawings take a palette,
    /// and this is it.
    pub chrome: daw_ui::theme::Chrome,
}

impl Palette {
    #[must_use]
    pub fn from_theme(theme: &daw_ui::theming::Theme) -> Self {
        let c = |col: daw_ui::theming::Color| Color::from_rgba8(col.r, col.g, col.b, col.a);
        Self {
            surface: c(theme.arrange.bg),
            row_a: c(theme.arrange.row_bg[0]),
            row_b: c(theme.arrange.row_bg[1]),
            divider: c(theme.arrange.row_divider[0]),
            grid: c(theme.arrange.grid_measure),
            grid_beat: c(theme.arrange.grid_beat),
            grid_sub: c(theme.arrange.grid_sub),
            ruler_bg: c(theme.arrange.ruler_bg),
            ruler_fg: c(theme.arrange.ruler_fg),
            ruler_fg2: c(theme.arrange.ruler_fg2),
            item_edge: c(theme.arrange.item_edge),
            text_dim: c(theme.tokens.text_dim),
            text_faint: c(theme.tokens.text_faint),
            accent: c(theme.tokens.accent),
            mute: c(theme.tokens.mute),
            solo: c(theme.tokens.solo),
            rec: c(theme.tokens.rec),
            meter_safe: c(theme.tokens.meter_safe),
            meter_warn: c(theme.tokens.meter_warn),
            pan: c(theme.tokens.route_send),
            meter_danger: c(theme.tokens.meter_danger),
            tcp_tint: c(theme.tokens.surface_raised),
            tcp_gutter: c(theme.tokens.surface),
            tcp_column: c(theme.tokens.surface_sunken),
            tcp_rule: c(theme.tokens.border),
            tcp_field: c(theme.tokens.surface_sunken),
            tcp_button: c(theme.tokens.surface),
            tcp_combo: c(theme.tokens.surface_sunken),
            tcp_meter_well: c(theme.tokens.surface_sunken),
            track_tint: theme.metrics.track_tint,
            chrome: chrome(theme),
            text: c(theme.tokens.text),
        }
    }
}

/// The key in force at `at`, as the KEY track's item names it ("F major"):
/// the last change at or before it, or the first if `at` comes before them
/// all. `None` for a session with no key items.
#[must_use]
pub fn key_at(project: &daw_ui::studio::project::Project, at: f64) -> Option<String> {
    let mut keys: Vec<(f64, &str)> = project
        .tracks
        .iter()
        .filter(|t| {
            t.name
                .trim()
                .eq_ignore_ascii_case(crate::key_track::KEY_TRACK)
        })
        .flat_map(|t| project.lane(&t.guid))
        .filter_map(|item| {
            let name = project.title(item)?;
            crate::key_track::parse_key(name).map(|_| (item.position.as_seconds(), name))
        })
        .collect();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    keys.iter()
        .rev()
        .find(|(from, _)| *from <= at + 1e-6)
        .or_else(|| keys.first())
        .map(|(_, name)| (*name).to_owned())
}

/// `outer` applied after `inner`.
///
/// Named rather than written as `*` because multiplying two affines is
/// composition, not arithmetic: there is nothing here to overflow, and
/// the lint that flags the operator is right to flag it everywhere else.
fn compose(outer: Affine, inner: Affine) -> Affine {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "affine composition; the operator is matrix multiplication"
    )]
    let composed = outer * inner;
    composed
}

/// Push one recorded command through `transform`.
///
/// The brush is forwarded WHOLE rather than unwrapped.
///
/// This used to match `Paint::Solid` and return false for anything else,
/// which silently dropped every gradient that reached a recorded scene:
/// the fader cap's moulding, the volume knob's drop shadow and body, the
/// record arm's ring. They were not mis-drawn, they were not drawn at
/// all — whatever had been painted underneath showed through instead,
/// which is why the result looked merely flat rather than broken. A
/// direct `painter.fill` with the same gradient worked, which is what
/// finally separated "gradients do not work" from "this replay discards
/// them".
pub fn submit_command(
    painter: &mut impl PaintScene,
    cmd: &RenderCommand,
    transform: Affine,
) -> bool {
    match cmd {
        RenderCommand::Fill(fill) => {
            painter.fill(
                fill.fill,
                compose(transform, fill.transform),
                &fill.brush,
                fill.brush_transform,
                &fill.shape,
            );
            true
        }
        RenderCommand::Stroke(stroke) => {
            painter.stroke(
                &stroke.style,
                compose(transform, stroke.transform),
                &stroke.brush,
                stroke.brush_transform,
                &stroke.shape,
            );
            true
        }
        // Text. Dropping this arm is not a silent degradation of
        // quality, it is a track panel with no names in it, so it is
        // handled here rather than defaulted.
        RenderCommand::GlyphRun(run) => {
            painter.draw_glyphs(
                &run.font_data,
                run.font_size,
                run.hint,
                &run.normalized_coords,
                run.embolden,
                &run.style,
                &run.brush,
                run.brush_alpha,
                compose(transform, run.transform),
                run.glyph_transform,
                run.glyphs.iter().copied(),
            );
            true
        }
        // Clipping. The rack scrolls, which means it draws past its own
        // box in both directions — above into the rails, below over the
        // strip's own controls — and a clip is the only thing that
        // stops it. Dropped, as these were, a scrolled rack silently
        // painted over its neighbours.
        //
        // Balanced by construction: whoever pushes one pops it inside
        // the same scene, and the overlay submits a scene whole. The
        // RECORDED scenes are replayed in per-strip ranges and must
        // therefore never carry a layer, since a range could cut
        // between a push and its pop.
        RenderCommand::PushClipLayer(clip) => {
            painter.push_clip_layer(clip.fill, compose(transform, clip.transform), &clip.clip);
            true
        }
        RenderCommand::PopLayer => {
            painter.pop_layer();
            true
        }
        _ => false,
    }
}
/// The resolved theme, as the art crate's control palette.
///
/// Started from the art's own defaults and overridden rather than built
/// field by field: `Chrome` carries relationships that were measured
/// together — the hardware face, its edge and its mark — and a palette
/// assembled from scratch would silently lose the ones this theme has
/// nothing to say about.
fn chrome(theme: &daw_ui::theming::Theme) -> daw_ui::theme::Chrome {
    let c = |col: daw_ui::theming::Color| daw_ui::theme::Color {
        r: col.r,
        g: col.g,
        b: col.b,
        a: col.a,
    };
    daw_ui::theme::Chrome {
        surface: c(theme.tokens.surface),
        surface_raised: c(theme.tokens.surface_raised),
        surface_sunken: c(theme.tokens.surface_sunken),
        border: c(theme.tokens.border),
        text: c(theme.tokens.text),
        text_dim: c(theme.tokens.text_dim),
        text_faint: c(theme.tokens.text_faint),
        accent: c(theme.tokens.accent),
        // A control's face, its edge and the ink on it. The buttons read
        // as part of the panel when these come off the same ladder the
        // panel does, and as borrowed art when they do not.
        //
        hardware: c(theme.tokens.surface_raised),
        hardware_edge: c(theme.tokens.border),
        // The art prints on this and rings with it — the record arm's
        // unlit ring is #a6a6a6 in the source, and a control FACE, not a
        // label. Mapped to `text_dim` alone it came out dark enough to
        // vanish against its own housing, which is why the fader's grip
        // needed hand-brightening at the call site.
        hardware_mark: c(theme.tokens.text_dim).mix(c(theme.tokens.text), 0.5),
        ..daw_ui::theme::Theme::default().chrome
    }
}

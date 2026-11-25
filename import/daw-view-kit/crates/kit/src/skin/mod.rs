//! The mixer drawn from a frame theme instead of the painters here.
//!
//! A theme is frame documents (`.frame`, or the older `.fm`) whose
//! `mcp.strip` page is one channel strip: its tokens are the colours, its
//! bindings read the track ([`schema`]), its `@name` nodes are the
//! controls a click lands on ([`control`]), and the controls it places are
//! components. See `docs/frame-themes.md`.
//!
//! **Layered**: `FTS_THEME` is a list of paths (`:`-separated, the base
//! first), each a file or a folder of them, every one over the ones before
//! it — so a theme built on another is only the files it changes: one
//! control, the strip, a colour (`frame::theme::runtime::layer`).
//!
//! Compiled in `FTS_THEME_MODE` or the documents' first mode, and
//! **hot-reloaded**: [`poll`] notices any of the files changing (or one
//! added to or taken from a folder) and recompiles, and a widget that sees
//! [`version`] move re-records. A theme that fails to compile leaves the
//! last good one on screen and says why once.
//!
//! The Tone rack is still the painters' own — the theme draws the REAPER
//! part of the strip, below it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
// The clock is `web_time`'s, which a browser has.
use std::time::Duration;
use web_time::Instant;

use daw_proto::Track;
use frame::theme::runtime::{Fields, Rgba, Schema, Theme, Value};

use crate::controls::Control;
use crate::palette::Palette;

pub mod canvas;
pub mod export;
pub mod uses;

/// The role a theme draws a mixer strip as.
pub const STRIP: &str = "mcp.strip";

/// The role a theme draws behind a strip's rack as, the rack's height
/// tall: the strip's ground, its edge, the selection rule at the top. The
/// strip's own fields.
pub const RACKBACK: &str = "mcp.rackback";

/// The role a theme draws one folder step as: the band under a nested
/// strip for each folder it is inside, `INDENT_STEP` tall. Drawn by the
/// painters when a theme has none.
pub const STEP: &str = "mcp.step";

/// How often [`poll`] looks at the file.
const POLL: Duration = Duration::from_millis(250);

/// What a theme can bind, in slot order: the spec's fields
/// (`daw_theme::spec::FIELDS`).
///
/// Live, because the mixer draws them every frame rather than re-record
/// for them: the values the overlay already draws live (mute, solo, arm,
/// the fader, the meter, which control the pointer is on).
#[must_use]
pub fn schema() -> Schema {
    daw_theme::spec::schema()
}

/// One strip's values, in [`schema`]'s order.
pub struct StripFields(Vec<Value>);

impl Fields for StripFields {
    fn field(&self, slot: u32) -> Value {
        self.0.field(slot)
    }
}

/// What a strip shows beyond the track itself.
#[derive(Clone, Copy, Default)]
pub struct Shown<'a> {
    /// The name as fitted to the plate, and the size it fits at.
    pub label: Option<(&'a str, f32)>,
    pub depth: usize,
    pub collapsed: bool,
    pub level: Option<daw_proto::TrackLevels>,
    pub clipped: bool,
    /// The control the pointer is over, and the one it is pressing.
    pub hover: Option<Control>,
    pub pressed: Option<Control>,
    /// The strip's width (the record input is elided to it).
    pub width: f64,
    /// The height the mixer's sections are resolved against: the whole
    /// panel's, less the rack — the same for every strip, however deep.
    pub mixer_h: f64,
    /// Whether a rack sits over the strip (which then draws the
    /// selection rule itself, at the top of the rack).
    pub rack: bool,
    /// The face text is measured in, for what has to be sized to it.
    pub font: Option<&'a crate::text::Font>,
    /// The mode the strip is laid out for — see `Settings::mixing`.
    pub mixing: bool,
    pub recording: bool,
}

static SCHEMA: std::sync::LazyLock<Schema> = std::sync::LazyLock::new(schema);

/// One folder step's values, for [`STEP`]: the colour of the folder it
/// stands for and how many folders out it is (1, the strip's own).
#[must_use]
pub fn step_fields(tint: vello::peniko::Color, level: usize) -> StripFields {
    let [r, g, b, a] = tint.to_rgba8().to_u8_array();
    values(&[
        ("folder.tint", Value::Color(Rgba::from_rgba8(r, g, b, a))),
        ("folder.level", Value::Num(crate::num::coord(level))),
    ])
}

/// The role a theme draws one track panel row as.
pub const ROW: &str = export::tcp::ROW;

/// What a track panel row shows beyond its track.
#[derive(Clone, Copy, Default)]
pub struct RowShown<'a> {
    /// How deep in folders the track is.
    pub depth: i32,
    /// The row's height, less the divider under it.
    pub height: f64,
    /// The face its text is measured in.
    pub font: Option<&'a crate::text::Font>,
    /// The control the pointer is over, and the one it is pressing.
    pub hover: Option<crate::row::Control>,
    pub pressed: Option<crate::row::Control>,
    /// Not recording: the window is in a mode other than Record, and the
    /// row has no arm (`ui.recording` false). Off by default — a row
    /// asked for with nothing said is a recording one, as it always was.
    pub idle: bool,
    /// A folder whose children are folded away (`track.collapsed`).
    pub collapsed: bool,
}

/// A row control's name in a theme: its `@name`, its `ui.hover`.
#[must_use]
pub const fn row_name(control: crate::row::Control) -> &'static str {
    use crate::row::Control as C;
    match control {
        C::Mute => "mute",
        C::Solo => "solo",
        C::RecArm => "arm",
        C::Volume => "volume",
        C::Pan => "pan",
        C::Routing => "routing",
        C::Fx => "fx",
        C::Name => "name",
        C::Folder => "folder",
        C::Phase => "phase",
        C::LanePrev => "lane-prev",
        C::LaneNext => "lane-next",
        C::LaneName => "lane-name",
        C::LaneShow => "lane-show",
        C::Automation => "automation",
    }
}

/// The row control a theme's `@name` is.
#[must_use]
pub fn row_control(name: &str) -> Option<crate::row::Control> {
    use crate::row::Control as C;
    [
        C::Mute,
        C::Solo,
        C::RecArm,
        C::Volume,
        C::Pan,
        C::Routing,
        C::Fx,
        C::Name,
        C::Folder,
        C::Phase,
    ]
    .into_iter()
    .find(|c| row_name(*c) == name)
}

/// The values a row's targets are laid out from — its indent, at
/// `depth` — for hit testing, which needs nothing else.
#[must_use]
pub fn row_hit_fields(depth: i32) -> StripFields {
    let tcp = crate::tcp::Tcp::FULL;
    values(&[(
        "row.indent",
        Value::Num((f64::from(depth.max(0)) * tcp.indent()).min(tcp.max_indent())),
    )])
}

/// `track`'s values for a track panel row: the strip's, and what the row
/// shows beyond them — the indent, the number and icon as the rail fits
/// them, the name as the field elides it, the record input.
#[must_use]
pub fn row_fields(palette: &Palette, track: &Track, shown: RowShown<'_>) -> StripFields {
    let tcp = crate::tcp::Tcp::FULL;
    let indent = (f64::from(shown.depth.max(0)) * tcp.indent()).min(tcp.max_indent());
    let mut out = fields(
        palette,
        track,
        Shown {
            collapsed: shown.collapsed,
            ..Shown::default()
        },
    );
    let mut set = |name: &str, v: Value| {
        if let Some(at) = SCHEMA
            .slot(name)
            .and_then(|s| usize::try_from(s).ok())
            .and_then(|s| out.0.get_mut(s))
        {
            *at = v;
        }
    };
    set("row.indent", Value::Num(indent));
    let named = |c: Option<crate::row::Control>| Value::text(c.map_or("", row_name));
    set("ui.hover", named(shown.hover));
    set("ui.pressed", named(shown.pressed));
    set("ui.recording", Value::Bool(!shown.idle));
    let Some(font) = shown.font else {
        return out;
    };
    let rail = crate::tcp::rail_layout(font, track, indent, 0.0, shown.height);
    set("row.has_number", Value::Bool(rail.number.is_some()));
    if let Some(number) = rail.number {
        set("row.number", Value::text(&number.text));
        set("row.number_size", Value::Num(f64::from(number.size)));
        set("row.number_x", Value::Num(number.x));
        set("row.number_baseline", Value::Num(number.baseline));
    }
    set("row.icon", Value::Bool(rail.icon.is_some()));
    if let Some(icon) = rail.icon {
        set("row.icon_x", Value::Num(icon.x0));
        set("row.icon_y", Value::Num(icon.y0));
        set("row.icon_size", Value::Num(icon.width()));
    }
    let (_, field_h) = crate::tcp::band(0.0, shown.height, crate::tcp::Density::at(shown.height));
    let size = tcp.name_size(field_h);
    let name_w = (tcp.volume_x() - tcp.name_x() - indent).max(0.0);
    set(
        "row.name",
        Value::text(&font.elide(&track.name, size, name_w)),
    );
    set("row.name_size", Value::Num(f64::from(size)));
    set(
        "row.input",
        Value::text(&font.elide(&crate::tcp::record_input(track), 11.0, 180.0)),
    );
    out
}

/// `track`'s values for a strip theme.
#[must_use]
pub fn fields(palette: &Palette, track: &Track, shown: Shown<'_>) -> StripFields {
    let mut values = vec![Value::Missing; SCHEMA.names().count()];
    let mut set = |name: &str, v: Value| {
        if let Some(slot) = SCHEMA.slot(name).and_then(|s| usize::try_from(s).ok())
            && let Some(at) = values.get_mut(slot)
        {
            *at = v;
        }
    };
    let color = |c: vello::peniko::Color| {
        let [r, g, b, a] = c.to_rgba8().to_u8_array();
        Value::Color(Rgba::from_rgba8(r, g, b, a))
    };
    let fraction = |v: f32| crate::engine::meter_fraction(v);
    let level = shown.level.unwrap_or_default();
    let named = |c: Option<Control>| c.map_or(Value::text(""), |c| Value::text(name(c)));

    set("track.name", Value::text(&track.name));
    let (label, label_size) = shown.label.unwrap_or((&track.name, 11.0));
    set("track.label", Value::text(label));
    set("track.label_size", Value::Num(f64::from(label_size)));
    set("track.number", Value::Num(f64::from(track.index) + 1.0));
    set(
        "track.color",
        color(crate::tcp::track_color(palette, track)),
    );
    set("track.has_color", Value::Bool(track.color.is_some()));
    set("track.selected", Value::Bool(track.selected));
    set("track.folder", Value::Bool(track.is_folder));
    set("track.collapsed", Value::Bool(shown.collapsed));
    set("track.depth", Value::Num(crate::num::coord(shown.depth)));
    let input = crate::tcp::record_input(track);
    let input = match shown.font {
        // As the painters elide it: to the field, less its caret.
        Some(font) => font.elide(&input, 10.0, (shown.width - 12.0).min(75.0) - 14.0),
        None => input,
    };
    set("track.input", Value::text(&input));
    set(
        "track.ground",
        color(crate::controls::strip_ground(palette, track)),
    );
    let edge = crate::controls::strip_edge(palette, track);
    set("track.has_edge", Value::Bool(edge.is_some()));
    set("track.edge", edge.map_or(Value::Missing, color));
    set("track.tint", color(crate::tcp::row_tint(palette, track)));
    set("track.band", color(crate::tcp::folder_band(palette, track)));
    set("track.parent_send", Value::Bool(track.parent_send));
    set("ui.rack", Value::Bool(shown.rack));
    set("mixer.h", Value::Num(shown.mixer_h));
    set("ui.mixing", Value::Bool(shown.mixing));
    set("ui.recording", Value::Bool(shown.recording));

    set("track.mute", Value::Bool(track.muted));
    set("track.solo", Value::Bool(track.soloed));
    set("track.armed", Value::Bool(track.armed));
    set(
        "track.monitor",
        Value::Num(match track.input_monitor {
            daw_proto::InputMonitoringMode::Off => 0.0,
            daw_proto::InputMonitoringMode::Normal => 1.0,
            daw_proto::InputMonitoringMode::NotWhenPlaying => 2.0,
        }),
    );
    set("track.phase", Value::Bool(track.phase_inverted));
    set(
        "track.fader",
        Value::Num(crate::tcp::volume_fraction(track.volume)),
    );
    set("track.volume", Value::Num(track.volume));
    set("track.pan", Value::Num(track.pan.clamp(-1.0, 1.0)));
    set("track.fx", Value::Num(f64::from(track.fx_count)));
    set("track.sends", Value::Num(f64::from(track.send_count)));
    set("track.receives", Value::Num(f64::from(track.receive_count)));
    let (left, right) = (fraction(level.peak_left), fraction(level.peak_right));
    set("meter.left", Value::Num(left));
    set("meter.right", Value::Num(right));
    set("meter.peak", Value::Num(left.max(right)));
    set("meter.hold_left", Value::Num(fraction(level.hold_left)));
    set("meter.hold_right", Value::Num(fraction(level.hold_right)));
    set("meter.clip", Value::Bool(shown.clipped));
    set("ui.hover", named(shown.hover));
    set("ui.pressed", named(shown.pressed));
    let readout = crate::overlay::volume_label(track.volume);
    if let Some(font) = shown.font {
        set(
            "track.volume_label_w",
            Value::Num(font.width(&readout, 9.0) + 6.0),
        );
    }
    set("track.volume_label", Value::text(&readout));
    StripFields(values)
}

/// A control's name in a theme: the `x` of a node named `@x`.
#[must_use]
pub const fn name(control: Control) -> &'static str {
    match control {
        Control::Fx => "fx",
        Control::Pan => "pan",
        Control::RecArm => "arm",
        Control::Mute => "mute",
        Control::Solo => "solo",
        Control::Routing => "routing",
        Control::Volume => "fader",
        Control::Name => "name",
        Control::Monitor => "monitor",
        Control::Clip => "clip",
        Control::Folder => "folder",
    }
}

/// The control a theme's `@name` is.
#[must_use]
pub fn control(name: &str) -> Option<Control> {
    Some(match name {
        "fx" => Control::Fx,
        "pan" => Control::Pan,
        "arm" => Control::RecArm,
        "mute" => Control::Mute,
        "solo" => Control::Solo,
        "routing" => Control::Routing,
        "fader" | "volume" => Control::Volume,
        "name" => Control::Name,
        "monitor" => Control::Monitor,
        "clip" => Control::Clip,
        "folder" => Control::Folder,
        _ => return None,
    })
}

// ── the loaded theme ──────────────────────────────────────────────────

/// Which pack and theme is loaded, as the actions report it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chosen {
    /// The pack's name; `None` for layers given by hand (`FTS_THEME`).
    pub pack: Option<String>,
    /// The theme's name in its pack.
    pub theme: Option<String>,
    pub mode: Option<String>,
    /// The layers, base first.
    pub layers: Vec<PathBuf>,
}

struct Loaded {
    chosen: Chosen,
    /// Every file the layers came to and when it last changed.
    watch: daw_theme::Watch,
    checked: Instant,
    /// The last error reported, so a broken file is reported once.
    reported: Option<String>,
}

#[cfg(not(test))]
static THEME: RwLock<Option<Arc<Theme>>> = RwLock::new(None);

// Each test its own: a test that sets a theme must not reach the
// painters' tests running beside it.
#[cfg(test)]
thread_local! {
    static THEME: RwLock<Option<Arc<Theme>>> = const { RwLock::new(None) };
}
static VERSION: AtomicU64 = AtomicU64::new(0);
#[cfg(not(test))]
static SOURCE: Mutex<Option<Loaded>> = Mutex::new(None);

#[cfg(test)]
thread_local! {
    static SOURCE: Mutex<Option<Loaded>> = const { Mutex::new(None) };
}

/// The files being watched, for `f` to look at or change.
fn with_source<R>(f: impl FnOnce(&mut Option<Loaded>) -> R) -> R {
    #[cfg(not(test))]
    return f(&mut SOURCE.lock().unwrap_or_else(PoisonError::into_inner));
    #[cfg(test)]
    SOURCE.with(|s| f(&mut s.lock().unwrap_or_else(PoisonError::into_inner)))
}

/// The theme the mixer draws with, when there is one with a strip.
#[must_use]
pub fn current() -> Option<Arc<Theme>> {
    loaded().filter(|t| t.has_role(STRIP))
}

/// The theme loaded, whatever it draws — for a view that asks the theme
/// for its own role (the rails) and draws itself when there is none.
#[must_use]
pub fn loaded() -> Option<Arc<Theme>> {
    #[cfg(not(test))]
    return THEME.read().unwrap_or_else(PoisonError::into_inner).clone();
    #[cfg(test)]
    THEME.with(|t| t.read().unwrap_or_else(PoisonError::into_inner).clone())
}

/// Which pack and theme is loaded; `None` for the built-in look.
#[must_use]
pub fn chosen() -> Option<Chosen> {
    with_source(|s| s.as_ref().map(|l| l.chosen.clone()))
}

/// Each colour of the [`Palette`] the painters draw with, and the spec's
/// token that sets it (`daw_theme::spec::TOKENS`).
pub const PALETTE: &[(&str, &str)] = &[
    ("surface", "arrange.ground"),
    ("row_a", "arrange.row"),
    ("row_b", "arrange.row_alt"),
    ("divider", "arrange.divider"),
    ("grid", "grid.bar"),
    ("grid_beat", "grid.beat"),
    ("grid_sub", "grid.sub"),
    ("ruler_bg", "ruler.ground"),
    ("ruler_fg", "ruler.ink"),
    ("ruler_fg2", "ruler.ink_dim"),
    ("item_edge", "clip.edge"),
    ("text", "text"),
    ("text_dim", "text.dim"),
    ("text_faint", "text.faint"),
    ("accent", "accent"),
    ("mute", "mute"),
    ("solo", "solo"),
    ("rec", "rec"),
    ("meter_safe", "meter.safe"),
    ("meter_warn", "meter.warn"),
    ("pan", "pan"),
    ("meter_danger", "meter.danger"),
    ("tcp_tint", "tcp.tint"),
    ("tcp_gutter", "rail.ground"),
    ("tcp_column", "tcp.column"),
    ("tcp_rule", "rail.rule"),
    ("tcp_field", "field"),
    ("tcp_button", "rail.button"),
    ("tcp_combo", "combo"),
    ("tcp_meter_well", "rack.well"),
    ("playhead", "playhead"),
    ("edit_cursor", "cursor.edit"),
];

/// Each colour of the control art's palette (`palette.chrome`), and its
/// token.
pub const CHROME: &[(&str, &str)] = &[
    ("hardware", "hardware"),
    ("hardware_edge", "hardware.edge"),
    ("hardware_mark", "hardware.mark"),
    ("surface", "surface"),
    ("surface_raised", "surface.raised"),
    ("surface_sunken", "surface.sunken"),
    ("border", "border"),
    ("text", "text"),
    ("text_dim", "text.dim"),
    ("text_faint", "text.faint"),
    ("accent", "accent"),
    ("selected", "selected"),
];

/// The palette's numbers and lists, and their tokens: how much a track's
/// colour tints its row, how opaque a clip is, the clip palette.
pub const PALETTE_VALUES: &[(&str, &str)] = &[
    ("track_tint", "tint.strength"),
    ("clip_alpha", "clip.alpha"),
    ("clips", "clip.<n>"),
];

/// The clip palette's tokens, in order.
pub const CLIPS: [&str; 16] = [
    "clip.0", "clip.1", "clip.2", "clip.3", "clip.4", "clip.5", "clip.6", "clip.7", "clip.8",
    "clip.9", "clip.10", "clip.11", "clip.12", "clip.13", "clip.14", "clip.15",
];

/// The [`Palette`] colour named `field`.
fn palette_field<'a>(p: &'a mut Palette, field: &str) -> Option<&'a mut vello::peniko::Color> {
    Some(match field {
        "surface" => &mut p.surface,
        "row_a" => &mut p.row_a,
        "row_b" => &mut p.row_b,
        "divider" => &mut p.divider,
        "grid" => &mut p.grid,
        "grid_beat" => &mut p.grid_beat,
        "grid_sub" => &mut p.grid_sub,
        "ruler_bg" => &mut p.ruler_bg,
        "ruler_fg" => &mut p.ruler_fg,
        "ruler_fg2" => &mut p.ruler_fg2,
        "item_edge" => &mut p.item_edge,
        "text" => &mut p.text,
        "text_dim" => &mut p.text_dim,
        "text_faint" => &mut p.text_faint,
        "accent" => &mut p.accent,
        "mute" => &mut p.mute,
        "solo" => &mut p.solo,
        "rec" => &mut p.rec,
        "meter_safe" => &mut p.meter_safe,
        "meter_warn" => &mut p.meter_warn,
        "pan" => &mut p.pan,
        "meter_danger" => &mut p.meter_danger,
        "tcp_tint" => &mut p.tcp_tint,
        "tcp_gutter" => &mut p.tcp_gutter,
        "tcp_column" => &mut p.tcp_column,
        "tcp_rule" => &mut p.tcp_rule,
        "tcp_field" => &mut p.tcp_field,
        "tcp_button" => &mut p.tcp_button,
        "tcp_combo" => &mut p.tcp_combo,
        "tcp_meter_well" => &mut p.tcp_meter_well,
        "playhead" => &mut p.playhead,
        "edit_cursor" => &mut p.edit_cursor,
        _ => return None,
    })
}

/// The art palette's colour named `field`.
fn chrome_field<'a>(
    c: &'a mut daw_ui::theme::Chrome,
    field: &str,
) -> Option<&'a mut daw_ui::theme::Color> {
    Some(match field {
        "hardware" => &mut c.hardware,
        "hardware_edge" => &mut c.hardware_edge,
        "hardware_mark" => &mut c.hardware_mark,
        "surface" => &mut c.surface,
        "surface_raised" => &mut c.surface_raised,
        "surface_sunken" => &mut c.surface_sunken,
        "border" => &mut c.border,
        "text" => &mut c.text,
        "text_dim" => &mut c.text_dim,
        "text_faint" => &mut c.text_faint,
        "accent" => &mut c.accent,
        "selected" => &mut c.selected,
        _ => return None,
    })
}

/// `base` recoloured by the loaded theme: every colour the painters draw
/// with that the theme sets, through its spec token, is the theme's.
///
/// So what is still painted rather than themed — the rack's curves and
/// knobs, the live strips, the arrangement — wears the theme's colours as
/// soon as it has them: a theme that sets `$mute` reddens every mute the
/// painters draw too, not only the ones it draws itself. A token the theme
/// leaves out falls back as the spec says, and to `base` (the built-in
/// look) when nothing it sets reaches it.
#[must_use]
pub fn palette(base: &Palette) -> Palette {
    palette_from(base, loaded().as_deref())
}

/// [`palette`] for `theme`, not the loaded one.
#[must_use]
pub fn palette_from(base: &Palette, theme: Option<&Theme>) -> Palette {
    let mut out = base.clone();
    if theme.is_none() {
        return out;
    }
    let tokens = daw_theme::Tokens::new(theme);
    let colour = |name: &str| {
        tokens.color(name).map(|c| {
            let [r, g, b, a] = c.to_rgba8();
            vello::peniko::Color::from_rgba8(r, g, b, a)
        })
    };
    for (field, token) in PALETTE {
        if let (Some(c), Some(at)) = (colour(token), palette_field(&mut out, field)) {
            *at = c;
        }
    }
    for (field, token) in CHROME {
        if let (Some(c), Some(at)) = (colour(token), chrome_field(&mut out.chrome, field)) {
            *at = crate::tcp::to_theme(c);
        }
    }
    if let Some(v) = tokens.number("tint.strength") {
        out.track_tint = crate::num::narrow(v.clamp(0.0, 1.0));
    }
    if let Some(v) = tokens.number("clip.alpha") {
        out.clip_alpha = Some(crate::num::narrow(v.clamp(0.0, 1.0)));
    }
    out.clips = CLIPS.iter().filter_map(|t| colour(t)).collect();
    out
}

/// Values for a view's own fields (`name`, value), every other field
/// missing — for the roles that are not a strip.
#[must_use]
pub fn values(set: &[(&str, Value)]) -> StripFields {
    let mut values = vec![Value::Missing; SCHEMA.names().count()];
    for (name, v) in set {
        if let Some(at) = SCHEMA
            .slot(name)
            .and_then(|s| usize::try_from(s).ok())
            .and_then(|s| values.get_mut(s))
        {
            *at = v.clone();
        }
    }
    StripFields(values)
}

/// Bumped whenever [`current`] changes: what a recording compares to
/// know it is stale.
#[must_use]
pub fn version() -> u64 {
    VERSION.load(Ordering::Acquire)
}

/// Use `theme` (or none) from now on.
pub fn set(theme: Option<Arc<Theme>>) {
    #[cfg(not(test))]
    {
        *THEME.write().unwrap_or_else(PoisonError::into_inner) = theme;
    }
    #[cfg(test)]
    THEME.with(|t| *t.write().unwrap_or_else(PoisonError::into_inner) = theme);
    VERSION.fetch_add(1, Ordering::AcqRel);
}

/// Back to the built-in look: no theme, nothing watched.
pub fn clear() {
    with_source(|s| *s = None);
    set(None);
}

/// Load the theme layered from `paths` (base first; files, or folders of
/// them) in `mode`, and keep watching them.
///
/// # Errors
/// Why it did not compile; the previous theme stays.
pub fn load(paths: Vec<PathBuf>, mode: Option<String>) -> Result<(), String> {
    load_chosen(Chosen {
        pack: None,
        theme: None,
        mode,
        layers: paths,
    })
}

/// Load theme `theme` of `pack`, and keep watching its files: every view
/// restyles when they change.
///
/// # Errors
/// Why it did not compile; the previous theme stays.
pub fn load_pack(pack: &daw_theme::Pack, theme: &daw_theme::ThemeDef) -> Result<(), String> {
    load_chosen(Chosen {
        pack: Some(pack.name.clone()),
        theme: Some(theme.name.clone()),
        mode: theme.mode.clone(),
        layers: theme.layers.clone(),
    })
}

fn load_chosen(chosen: Chosen) -> Result<(), String> {
    let watch = daw_theme::Watch::new(chosen.layers.clone());
    let result = compile(&watch.files(), chosen.mode.as_deref());
    let loaded = Loaded {
        chosen,
        watch,
        checked: Instant::now(),
        reported: result.as_ref().err().cloned(),
    };
    // A theme that does not compile is not chosen: the last good one
    // stays, and goes on being watched.
    if result.is_ok() {
        with_source(|source| *source = Some(loaded));
    }
    result.map(|theme| set(Some(Arc::new(theme))))
}

/// Load `FTS_THEME`, once, if it is set.
pub fn from_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Some(paths) = std::env::var_os("FTS_THEME") else {
            return;
        };
        let mode = std::env::var("FTS_THEME_MODE").ok();
        if let Err(why) = load(std::env::split_paths(&paths).collect(), mode) {
            tracing::warn!(theme.error = %why, "theme did not load");
        }
    });
}

/// Recompile the theme if any of its files changed since the last look,
/// or a folder gained or lost one (looked at no more than every
/// [`POLL`]). True when [`current`] changed.
pub fn poll() -> bool {
    let fresh = with_source(|source| {
        let loaded = source.as_mut()?;
        if loaded.checked.elapsed() < POLL {
            return None;
        }
        loaded.checked = Instant::now();
        if !loaded.watch.changed() {
            return None;
        }
        match compile(&loaded.watch.files(), loaded.chosen.mode.as_deref()) {
            Ok(theme) => {
                loaded.reported = None;
                Some(theme)
            }
            Err(why) => {
                if loaded.reported.as_deref() != Some(why.as_str()) {
                    tracing::warn!(theme.error = %why, "theme did not reload; keeping the last good one");
                    loaded.reported = Some(why);
                }
                None
            }
        }
    });
    fresh.map(|theme| set(Some(Arc::new(theme)))).is_some()
}

fn compile(files: &[PathBuf], mode: Option<&str>) -> Result<Theme, String> {
    if files.is_empty() {
        return Err("no theme files".into());
    }
    daw_theme::pack::compile(files, mode).map_err(|e| e.to_string())
}

#[cfg(test)]
mod reload_tests {
    use super::*;

    const DARK: &str = r##"<Document><Variables modes="dark light"><Var name="mute" dark="#111111" light="#eeeeee" /></Variables></Document>"##;

    fn mute(p: &Palette) -> [u8; 4] {
        p.mute.to_rgba8().to_u8_array()
    }

    /// A pack's theme loads, its tokens recolour the palette, and an edit
    /// to its file restyles the views (the version moves) without a reload
    /// being asked for.
    #[test]
    fn a_pack_loads_and_reloads_when_its_file_changes() {
        let dir = std::env::temp_dir().join(format!("skin-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("themes/t")).expect("dirs");
        std::fs::write(dir.join("themes/t/tokens.frame"), DARK).expect("write");
        std::fs::write(
            dir.join("pack.styx"),
            "name demo\nthemes {\n    t-light { layers (themes/t), mode light }\n}\n",
        )
        .expect("manifest");
        let pack = daw_theme::Pack::open(&dir).expect("pack");
        let theme = pack.theme(None).expect("theme");
        load_pack(&pack, theme).expect("loads");
        let base = Palette::from_theme(&daw_ui::theming::Theme::dark());
        assert_eq!(mute(&palette(&base)), [0xee, 0xee, 0xee, 0xff]);
        assert_eq!(chosen().and_then(|c| c.theme).as_deref(), Some("t-light"));

        let before = version();
        std::fs::write(
            dir.join("themes/t/tokens.frame"),
            DARK.replace("#eeeeee", "#123456"),
        )
        .expect("edit");
        let file = std::fs::File::options()
            .write(true)
            .open(dir.join("themes/t/tokens.frame"))
            .expect("open");
        file.set_modified(std::time::SystemTime::UNIX_EPOCH)
            .expect("mtime");
        std::thread::sleep(POLL + Duration::from_millis(50));
        assert!(poll(), "the edit is seen");
        assert!(version() > before);
        assert_eq!(mute(&palette(&base)), [0x12, 0x34, 0x56, 0xff]);

        clear();
        assert_eq!(
            mute(&palette(&base)),
            mute(&base),
            "cleared, the built-in look"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

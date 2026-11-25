//! The MIDI editor, opened headlessly: the view the window docks
//! (`midi_editor::expression::Expression`), on the golden session's takes
//! the way the window opens one from the engine's, and on the editor's own
//! audio documents for what only an audio take shows.

use daw_theme::gallery::View;
use midi_editor::expression::Expression;
use daw_view_kit::mousemap::Mods;

use crate::golden::Golden;

/// Bars of the tracked kit the stacked lanes show.
const KIT_BARS: usize = 4;

/// The editor on what `view` shows, `size` big.
#[must_use]
pub fn open(golden: &Golden, view: View, size: (f64, f64)) -> Expression {
    let mut editor = match view {
        View::Vocal => vocal(size),
        View::Stack => Expression::audio_kit(KIT_BARS, (0.0, 0.0), size),
        View::Drums => from_take(&golden.drums, true, size),
        _ => from_take(&golden.melody, false, size),
    };
    super::editor_states::apply(&mut editor, golden, view);
    editor
}

/// The editor on a MIDI take, on a kit's lanes or a piano roll.
fn from_take(take: &crate::golden::Take, drums: bool, size: (f64, f64)) -> Expression {
    Expression::from_take(&take.snapshot, take.item.clone(), drums, (0.0, 0.0), size)
}

/// The editor on a sung take: the editor's own phrase of zoned notes
/// (`demo::Scene::Zones`), read as pitched audio.
fn vocal(size: (f64, f64)) -> Expression {
    let viewport = editor::Viewport::new(size.0.max(1.0), size.1.max(1.0));
    let mut doc = editor::paint::demo::editor(editor::paint::demo::Scene::Zones, viewport);
    doc.set_mode(editor::Mode::Vocals);
    Expression::from_audio(doc, (0.0, 0.0), size)
}

/// Open the editor's context menu where a right-click in the roll leaves
/// it, with the pointer on its first item.
pub fn open_menu(view: &mut Expression, (w, h): (f64, f64)) {
    let (x, y) = (w * 0.35, h * 0.3);
    view.press(x, y, Mods::default(), 2);
    view.release(x, y, Mods::default());
    view.moved(x + 24.0, y + 14.0, Mods::default());
}

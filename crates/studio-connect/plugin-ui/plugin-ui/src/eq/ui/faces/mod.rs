//! One front panel per EQ model.
//!
//! Selecting a model in the shell rail swaps the whole surface: `Main` is the
//! FTS curve editor, and the other four are the unit's own front panel, drawn
//! from the shared hardware kit in [`fts_audio_ui::hardware`].
//!
//! The EQ needs no mapping layer to do it. Unlike the compressor — where a
//! hardware control writes several engine params through `comp-profiles` — the
//! EQ's models already have their own parameters (`pultec_*`, `neve_*`,
//! `api_*`, `ssl_*`), because the DSP for each model is its own circuit rather
//! than a re-skin of one. So a panel control is one parameter, and
//! [`params_map`] is the whole translation.

pub mod params_map;
pub mod rack;
pub mod units;

use fts_audio_ui::shell::ShellItem;

// The model table, its ids and badges are the param tree's (the plugin's
// stack reads them with no editor): re-exported at their old address.
pub use crate::eq::ui::params::{
    CATEGORIES, EqCategory, category_of, model_badge, model_for_id, model_id, model_name,
    resolved_model, store_model_id,
};

/// The panel for a `model` value, if it has one — re-exported so the editor
/// does not have to know that designs live in [`units`].
pub use units::design_for as design_for_model;

/// The editor form a loaded session should open at. An unknown or missing id
/// means Responsive, which is the size the face asks for anyway.
pub fn resolved_form(params: &crate::eq::ui::params::FtsEqParams) -> fts_audio_ui::EditorForm {
    fts_audio_ui::EditorForm::from_id(&params.editor_form.read()).unwrap_or_default()
}

pub fn store_form(params: &crate::eq::ui::params::FtsEqParams, form: fts_audio_ui::EditorForm) {
    *params.editor_form.write() = form.id().to_string();
}

/// The editor size for a model *and* a chosen form: the form decides, except
/// for Responsive, which defers to the face.
#[must_use]
pub fn editor_size_for(model: i32, form: fts_audio_ui::EditorForm) -> (u32, u32) {
    form.editor_size(fts_audio_ui::shell::RAIL_W, preferred_editor_size(model))
}

/// The rail, badged with the active model when its family is the active one.
#[must_use]
pub fn rail_items(model: i32) -> Vec<ShellItem> {
    let active = category_of(model);
    CATEGORIES
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let badge = match active {
                Some((ci, vi)) if ci == index => {
                    category.badges.get(vi).copied().unwrap_or(category.badge)
                }
                _ => category.badge,
            };
            ShellItem::new(category.id, category.label).with_badge(badge)
        })
        .collect()
}

/// The `model` value a rail click selects: clicking the active family cycles
/// the models inside it, clicking another lands on its first.
#[must_use]
pub fn rail_click_target(model: i32, clicked_category: usize) -> i32 {
    let Some(category) = CATEGORIES.get(clicked_category) else {
        return model;
    };
    match category_of(model) {
        Some((current, variant)) if current == clicked_category => {
            category.models[(variant + 1) % category.models.len()]
        }
        _ => category.models[0],
    }
}

/// The editor size a model wants, in logical px, including the shell rail.
///
/// The curve editor wants height; a rack unit is a wide, short drawing and
/// given a tall window just paints black above and below itself.
#[must_use]
pub fn preferred_editor_size(model: i32) -> (u32, u32) {
    match units::design_for(model) {
        Some(design) => (
            (design.w + fts_audio_ui::shell::RAIL_W + 52.0) as u32,
            (design.h + 48.0) as u32,
        ),
        None => (
            crate::eq::ui::control_view::EDITOR_W,
            crate::eq::ui::control_view::EDITOR_H,
        ),
    }
}

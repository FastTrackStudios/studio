//! The manifest held to the pixels: is a token a shot says it shows really
//! on screen in it?
//!
//! For each token of the spec, each shot that lists it is drawn twice, under
//! a theme that sets nothing and under one that sets only that token to a
//! sentinel; the first shot whose pixels differ shows it. A token only a
//! pack's own pages read (`text.bright`, `radius.control`, …) changes
//! nothing the views draw themselves, so it is reported as seen through a
//! pack's pages only — which this check, having no pages, cannot draw.

use std::collections::HashMap;
use std::sync::Arc;

use daw_theme::gallery::{SHOTS, Shot};
use daw_theme::spec::{self, Kind};
use frame::theme::runtime::Theme;

use super::render::{self, Planned, Stage};
use crate::golden::Golden;

/// What the check found for one token.
#[derive(Clone, Debug)]
pub struct Seen {
    pub token: &'static str,
    /// The first shot whose pixels the token changed.
    pub shot: Option<&'static str>,
    /// The shots that list it.
    pub listed: Vec<&'static str>,
}

/// A value for `kind` no host draws with by default.
const fn sentinel(token: &str, kind: Kind) -> &'static str {
    match kind {
        Kind::Color => "#ff00ff",
        Kind::Flag => "true",
        Kind::Font => "&quot;Courier&quot;",
        Kind::Number => match token.as_bytes() {
            [b't', b'i', b'n', b't', ..] => "1",
            [b'c', b'l', b'i', b'p', ..] => "0.15",
            _ => "17",
        },
    }
}

/// A theme of `vars` alone (`<Var …/>` elements), compiled against the spec.
///
/// # Errors
/// The markup did not compile.
fn tokens_theme(vars: &str) -> Result<Theme, String> {
    let src = format!("<Document><Variables modes=\"dark\">{vars}</Variables></Document>");
    Theme::from_markup(&src, spec::schema(), Some("dark")).map_err(|d| format!("{d:?}"))
}

/// Show `theme` and give the stage the shots draw with under it.
fn stage_for(theme: Theme) -> Result<Stage, super::Error> {
    daw_view_kit::skin::set(Some(Arc::new(theme)));
    let base = arrangement_view::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
    Ok(Stage {
        palette: daw_view_kit::skin::palette(&base),
        font: daw_view_kit::text::Font::embedded().map_err(|e| super::Error::Theme {
            theme: "font".to_owned(),
            why: e.to_string(),
        })?,
        layout: daw_view_kit::layout::Layout::default(),
    })
}

/// Hold every token of the spec to the shots that list it.
///
/// # Errors
/// A sentinel theme did not compile.
pub fn verify(golden: &Golden, planned: &Planned) -> Result<Vec<Seen>, super::Error> {
    let fail = |why: String| super::Error::Theme {
        theme: "sentinel".to_owned(),
        why,
    };
    let mut plain: HashMap<&'static str, Vec<u8>> = HashMap::new();
    let mut out = Vec::new();
    for token in spec::TOKENS {
        let listed: Vec<&'static Shot> = SHOTS
            .iter()
            .filter(|s| s.tokens().any(|t| t == token.name))
            .collect();
        let mut seen = None;
        for shot in &listed {
            if !plain.contains_key(shot.name) {
                let stage = stage_for(tokens_theme("").map_err(fail)?)?;
                plain.insert(shot.name, render::shot(golden, planned, &stage, shot).rgba);
            }
            let var = format!(
                r#"<Var name="{}" value="{}" />"#,
                token.name,
                sentinel(token.name, token.kind)
            );
            // A sentinel that does not compile shows nothing: the token is
            // reported unseen rather than failing the whole check.
            let Ok(theme) = tokens_theme(&var) else {
                break;
            };
            let stage = stage_for(theme)?;
            let changed = render::shot(golden, planned, &stage, shot).rgba;
            if plain.get(shot.name).is_some_and(|p| *p != changed) {
                seen = Some(shot.name);
                break;
            }
        }
        out.push(Seen {
            token: token.name,
            shot: seen,
            listed: listed.iter().map(|s| s.name).collect(),
        });
    }
    daw_view_kit::skin::clear();
    Ok(out)
}

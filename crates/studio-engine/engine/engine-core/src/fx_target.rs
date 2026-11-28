//! The FX an action names on a track ([`target`]) and how an FX is called
//! ([`display_name`]). From engine-mix's `fx_actions` and `clap_fx`,
//! shared by the mix and listen.

use std::path::Path;

use daw::service::{Effects, FxChainContext, FxRef, FxTarget, ProjectContext};

use crate::actions::resolve;

/// `track`'s chain and the FX `which` names in it: its guid, its place
/// (`#2`, from 1), or its name (any part of it, case aside).
pub fn target(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    which: &str,
) -> Result<(FxTarget, daw_proto::fx::Fx), String> {
    let guid = resolve(daw, project, track)?;
    let context = FxChainContext::Track(guid);
    let chain = Effects::list(daw, project.clone(), context.clone());
    let which = which.trim();
    // Its guid, as the window names an FX (two of one plugin are two FX).
    let found = if let Some(fx) = chain.iter().find(|fx| fx.guid == which) {
        Some(fx.clone())
    } else if let Some(place) = which.strip_prefix('#') {
        let place: usize = place
            .parse()
            .map_err(|_| format!("{which:?} is not a place"))?;
        chain
            .get(place.checked_sub(1).ok_or("places count from #1")?)
            .cloned()
    } else {
        let lower = which.to_lowercase();
        let named: Vec<&daw_proto::fx::Fx> = chain
            .iter()
            .filter(|fx| {
                fx.name.to_lowercase().contains(&lower)
                    || fx.plugin_name.to_lowercase().contains(&lower)
            })
            .collect();
        match named.as_slice() {
            [] => None,
            [one] => Some((*one).clone()),
            many => {
                // An exact name wins over the others it is part of.
                many.iter()
                    .find(|fx| {
                        fx.name.eq_ignore_ascii_case(which)
                            || fx.plugin_name.eq_ignore_ascii_case(which)
                    })
                    .map(|fx| (*fx).clone())
                    .ok_or_else(|| {
                        format!(
                            "{which:?} could be any of {} — name one, or its #place",
                            many.iter()
                                .map(|fx| display_name(fx))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .map(Some)?
            }
        }
    };
    let fx = found.ok_or_else(|| {
        format!(
            "no {which:?} on the track — it has {}",
            chain
                .iter()
                .map(display_name)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    Ok((
        FxTarget {
            context,
            fx: FxRef::Guid(fx.guid.clone()),
        },
        fx,
    ))
}

/// The name an FX goes by in a list: a hosted plugin's own name, else its
/// chain name.
#[must_use]
pub fn display_name(fx: &daw_proto::fx::Fx) -> String {
    let path_named = Path::new(&fx.name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("clap") || e.eq_ignore_ascii_case("vst3"));
    if path_named && !fx.plugin_name.is_empty() {
        fx.plugin_name.clone()
    } else {
        fx.name.clone()
    }
}

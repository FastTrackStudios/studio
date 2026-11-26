//! What other domains ask of the mix, through the engine's [`Assembly`]
//! (engine-mix's parts, filled by studio-engine): an edit algorithm's
//! answer ([`place`], [`plan`]), a Signal FX's chain and actions
//! ([`signal_chain`], [`signal_run`]) and the drum pieces' reverb returns
//! ([`drum_returns`]).
//!
//! [`Assembly`]: crate::assembly::Assembly

use daw::service::ProjectContext;
use daw::standalone::Standalone;
use serde_json::Value;

/// The engine's parts, or the refusal a core with no engine around it says.
fn parts() -> Result<&'static crate::assembly::Assembly, String> {
    crate::assembly::get().ok_or_else(|| "no engine installed around the core".to_owned())
}

/// Where a performance's hits play, from the chosen `daw.edit` provider
/// (engine-mix's `algorithms::place`).
///
/// # Errors
///
/// No engine, no provider, or the provider failed or did not answer.
pub fn place(request: &daw_edit::PlaceRequest) -> Result<daw_edit::PlaceAnswer, String> {
    (parts()?.place)(request)
}

/// Where a kit's played hits should go, from the chosen `daw.edit`
/// provider (engine-mix's `algorithms::plan`).
///
/// # Errors
///
/// No engine, no provider, or the provider failed or did not answer.
pub fn plan(request: &daw_edit::PlanRequest) -> Result<daw_edit::PlanAnswer, String> {
    (parts()?.plan)(request)
}

/// The Signal chain on `track` (its FX `fx`, else its first Signal), as
/// data (engine-mix's `clap_host::signal::chain`).
///
/// # Errors
///
/// No engine, or no Signal there, or it did not answer.
pub fn signal_chain(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    fx: Option<&str>,
) -> Result<Value, String> {
    (parts()?.signal_chain)(daw, project, track, fx)
}

/// Signal's `action` with `args` (JSON) on `track`'s Signal (engine-mix's
/// `clap_host::signal::run`).
///
/// # Errors
///
/// No engine, or no Signal there, or the action failed.
pub fn signal_run(
    daw: &Standalone,
    project: &ProjectContext,
    target: (&str, Option<&str>),
    action: &str,
    args: &str,
) -> Result<Value, String> {
    (parts()?.signal_run)(daw, project, target, action, args)
}

/// Set the drum pieces' reverb returns (engine-mix's
/// `mix_chains::drum_returns`); `dry`, only say. One line a return.
///
/// # Errors
///
/// No engine, or a backend call failed.
pub fn drum_returns(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
    dry: bool,
) -> eyre::Result<Vec<String>> {
    let parts = parts().map_err(|e| eyre::eyre!(e))?;
    (parts.drum_returns)(daw, project, dry)
}

//! Signal's offline blocks, as another domain asks for them: SIGNAL_ANALYZE
//! and what follows an edit step. Both are engine-listen's
//! (`signal_offline`), reached through the [`crate::assembly`].

use daw::service::ProjectContext;
use daw::standalone::Standalone;
use facet::Facet;
use serde_json::Value;

/// What SIGNAL_ANALYZE takes.
#[derive(Facet, Default)]
pub struct AnalyzeArg {
    pub track: String,
    pub fx: Option<String>,
    pub block: Option<String>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub consistency: Option<f64>,
    pub range_db: Option<f64>,
    pub target_db: Option<f64>,
    pub replace_user_edits: Option<bool>,
    pub reference: Option<String>,
    pub window_s: Option<f64>,
}

/// SIGNAL_ANALYZE (engine-listen's `signal_offline::analyze`).
///
/// # Errors
/// No Signal FX or offline block, nothing to read, the plugin's refusal,
/// or no engine installed around the core.
pub fn analyze(
    daw: &Standalone,
    project: &ProjectContext,
    a: &AnalyzeArg,
) -> Result<Value, String> {
    let assembly = crate::assembly::get().ok_or("no engine installed to analyse with")?;
    (assembly.signal_analyze)(daw, project, a)
}

/// After an edit step: each watched Signal FX told its track's source
/// revision (engine-listen's `signal_offline::after_step`). Nothing with
/// no engine installed.
pub fn after_step(daw: &Standalone, project: &ProjectContext) {
    if let Some(assembly) = crate::assembly::get() {
        (assembly.signal_after_step)(daw, project);
    }
}

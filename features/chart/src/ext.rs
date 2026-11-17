//! The extension: one `linked::Extension`, linked in or served as a
//! process.

use std::sync::{Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension, Handle};
use serde_json::{Value, json};

/// Its id: its view is `studio-chart/chart`.
pub const ID: &str = "studio-chart";

/// The extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct Chart;

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

/// Its handle to the host, once it runs (a default one, which every call
/// refuses, before).
#[must_use]
pub fn handle() -> Handle {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

impl Extension for Chart {
    fn manifest(&self) -> &'static str {
        include_str!("../extension.toml")
    }

    fn build(&self, ext: &mut Builder) {
        *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(ext.handle());
        ext.view("chart", crate::view::ChartView)
            .action("STUDIO_CHART_OPEN", open);
    }
}

/// `STUDIO_CHART_OPEN`: its pane, on `note` when given.
async fn open(a: Value) -> Result<Value, String> {
    let note = a
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    setlists::host::run_on(
        &handle(),
        "EXT_VIEW_OPEN",
        json!({ "extension": ID, "view": "chart", "state": note }),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_manifest_reads_and_names_what_it_builds() {
        let m = architect_extensions::manifest::Manifest::parse(Chart.manifest())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(m.id, ID);
        assert!(m.actions.iter().any(|a| a.id == "STUDIO_CHART_OPEN"));
        assert!(m.views.iter().any(|v| v.id == "chart"));
    }
}

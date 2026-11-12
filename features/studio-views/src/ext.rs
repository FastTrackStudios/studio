//! The extension: one `linked::Extension`, compiled into Studio
//! Architect and served as a process the Architect app installs
//! (`src/main.rs`).
//!
//! Each view button's action opens or focuses its view through the app's
//! `EXT_VIEW_OPEN`, or refuses with why its view is not there. Which
//! button is lit follows the workspace: on every `workspace.changed`
//! (the manifest's hook) `STUDIO_VIEWS_FOLLOW` reads the focused view
//! (the host's `selection`) and writes each toggle's state, so the state
//! is the extension's, never a copy in the toolbar.

use std::sync::{Mutex, PoisonError};

use architect_extensions::contrib::{StateKind, StateWrite};
use architect_extensions::hostcall::HostRequest;
use architect_extensions::linked::{Builder, Extension, Handle};
use serde_json::{Value, json};

use crate::{Opens, StudioView, all, checked, moves_lit};

/// The extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct StudioViews;

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

fn handle() -> Result<Handle, String> {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or_else(|| "the Studio views extension is not running".to_owned())
}

impl Extension for StudioViews {
    fn manifest(&self) -> &'static str {
        include_str!("../extension.toml")
    }

    fn build(&self, ext: &mut Builder) {
        *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(ext.handle());
        for view in all() {
            ext.action(view.action, move |_| open(view));
        }
        ext.action("STUDIO_VIEWS_FOLLOW", |_| follow());
    }
}

/// Open or focus `view`'s view, lit; or say why it is not there.
async fn open(view: &'static StudioView) -> Result<Value, String> {
    let h = handle()?;
    let Opens::View {
        extension,
        view: id,
    } = view.opens
    else {
        let why = match view.opens {
            Opens::Missing(why) => format!("{} {why} ({})", view.label, view.issue),
            Opens::View { .. } => format!("{} cannot open", view.label),
        };
        // Said to the person too: a toolbar button's failed action is not
        // shown by the app (and it cannot be drawn disabled with why: an
        // extension cannot disable its own action).
        let _ = h
            .host(HostRequest::Notify {
                text: why.clone(),
                kind: Some("warning".to_owned()),
                title: Some(view.label.to_owned()),
            })
            .await;
        return Err(why);
    };
    h.host(HostRequest::Run {
        action: "EXT_VIEW_OPEN".to_owned(),
        args: json!({ "extension": extension, "view": id }),
        ticket: None,
    })
    .await
    .map_err(|e| e.to_string())?;
    let kind = format!("{extension}/{id}");
    write_checked(&h, &kind);
    Ok(json!({ "opened": kind }))
}

/// Light the button of the view the workspace has focused.
async fn follow() -> Result<Value, String> {
    let h = handle()?;
    let selection = h
        .host(HostRequest::Selection)
        .await
        .map_err(|e| e.to_string())?;
    let focused = selection
        .get("focused_view")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if !moves_lit(&focused) {
        return Ok(json!({ "focused_view": focused, "lit": "unchanged" }));
    }
    let lit = write_checked(&h, &focused);
    Ok(json!({ "focused_view": focused, "lit": lit }))
}

/// Every toggle's state for `focused_view`: the one lit, if any.
fn write_checked(h: &Handle, focused_view: &str) -> Option<&'static str> {
    let mut lit = None;
    for (action, on) in checked(focused_view) {
        if on {
            lit = Some(action);
        }
        h.set_state(StateWrite {
            key: action.to_owned(),
            kind: StateKind::Checked,
            text: if on { "on" } else { "off" }.to_owned(),
            ..StateWrite::default()
        });
    }
    lit
}

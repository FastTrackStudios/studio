//! The extension in a host, as Studio Architect loads it: its Views
//! toolbar at the bottom, every item an action it has; a button whose view
//! is not there refusing with why and opening nothing; the toggles'
//! state written from the workspace's focus.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::{Arc, Mutex, PoisonError};

use architect::action::ActionList;
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::hostapi::AppContext;
use architect_extensions::install::Installs;
use architect_extensions::toolbar::ToolbarPlace;
use architect_settings::Registry;
use futures::executor::block_on;
use serde_json::{Value, json};

/// The workspace's focus, as the app tells it.
#[derive(Clone, Default)]
struct Focus(Arc<Mutex<String>>);

impl Focus {
    fn set(&self, view: &str) {
        view.clone_into(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

impl AppContext for Focus {
    fn selection(&self) -> Option<Value> {
        let view = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Some(json!({ "focused_view": view }))
    }
}

struct App {
    list: ActionList,
    host: Host,
    focus: Focus,
    opened: Arc<Mutex<Vec<String>>>,
    _ext: Extensions,
    _tmp: tempfile::TempDir,
}

fn app() -> App {
    let tmp = tempfile::tempdir().unwrap();
    let app = Arc::new(ActionList::new());
    let opened = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&opened);
    let list = ActionList::new();
    let focus = Focus::default();
    let host = Host::new(list.clone(), SharedSections::new(Registry::new()));
    host.set_app_context(Arc::new(focus.clone()));
    // The window's part of the app's EXT_VIEW_OPEN: what it was asked.
    host.set_view_opener(move |open| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(open.kind.clone());
        Ok(json!({ "pane": open.kind }))
    });
    host.set_app_list((*app).clone());
    host.forward_actions_to(&app);
    let ext = Extensions::new(host.clone(), Installs::new(tmp.path().join("installs")))
        .with_linked(&studio_views::ext::StudioViews)
        .unwrap();
    ext.install_contribution_actions().unwrap();
    let _ = ext.sync();
    App {
        list,
        host,
        focus,
        opened,
        _ext: ext,
        _tmp: tmp,
    }
}

impl App {
    fn run(&self, id: &str) -> Result<Value, String> {
        block_on(self.list.run_json_async(id, b"{}"))
            .map(|out| serde_json::from_slice(&out).unwrap_or(Value::Null))
            .map_err(|e| e.to_string())
    }

    fn lit(&self) -> Vec<String> {
        studio_views::all()
            .filter(|v| {
                self.host
                    .contributions()
                    .checked("studio-views", v.action, now())
            })
            .map(|v| v.action.to_owned())
            .collect()
    }
}

fn now() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

#[test]
fn the_views_toolbar_is_at_the_bottom_every_item_an_action_it_has() {
    let app = app();
    let bars = app.host.contributions().toolbars();
    let views = bars
        .iter()
        .find(|b| b.owner == "studio-views" && b.item.id == "views")
        .unwrap_or_else(|| {
            panic!(
                "no Views toolbar among {:?}",
                bars.iter().map(|b| &b.item.id).collect::<Vec<_>>()
            )
        });
    assert_eq!(
        views.item.place(),
        Some(ToolbarPlace::Dock("bottom".to_owned()))
    );
    let actions: Vec<&str> = views
        .item
        .items
        .iter()
        .map(|i| i.action.as_str())
        .filter(|a| !a.is_empty())
        .collect();
    let all: Vec<&str> = studio_views::all().map(|v| v.action).collect();
    assert_eq!(actions, all);
    // Each button as the model draws it: its label and icon.
    for item in views.item.items.iter().filter(|i| !i.action.is_empty()) {
        let view = studio_views::by_action(&item.action).unwrap();
        assert_eq!(
            (item.label.as_str(), item.icon.as_str()),
            (view.label, view.icon)
        );
    }
    for a in actions {
        assert!(app.list.info(a).is_some(), "{a} is not an action");
    }
}

#[test]
fn a_view_that_is_not_there_refuses_with_why() {
    let app = app();
    let why = app.run("STUDIO_VIEWS_MIXER").unwrap_err();
    assert!(why.contains("Mixer needs Studio's engine"), "{why}");
    assert!(why.contains("(studio#434)"), "{why}");
    let why = app.run("STUDIO_VIEWS_CHART").unwrap_err();
    assert!(why.contains("song workspace"), "{why}");
    assert_eq!(app.opened.lock().unwrap().len(), 0);
}

#[test]
fn the_toggles_follow_the_focus() {
    let app = app();
    // No button's view is registered yet: whatever is focused, every
    // toggle is written off.
    app.focus.set("studio-views/mixer");
    let out = app.run("STUDIO_VIEWS_FOLLOW").unwrap();
    assert_eq!(out["lit"], Value::Null);
    assert_eq!(app.lit(), Vec::<String>::new());
    // Its toolbar focused (a button pressed): nothing is written.
    app.focus.set("toolbar:studio-views/views");
    let out = app.run("STUDIO_VIEWS_FOLLOW").unwrap();
    assert_eq!(out["lit"], "unchanged");
}

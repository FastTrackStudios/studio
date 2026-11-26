//! The mixer's Render and Export buttons: on the mixer's own pane,
//! each the engine's MIX_RENDER or MIX_EXPORT with nothing given (the
//! engine says where the file goes), and within what the extension may
//! run, so pressing one reaches the engine. What they write is held by
//! the engine's tests and by the Unbreakable run over MCP
//! (`daw-extension`'s `tests/unbreakable`, steps `render_button` and
//! `export`).

use architect_extensions::manifest::Manifest;
use studio_mixer::MANIFEST;

fn manifest() -> Manifest {
    Manifest::parse(MANIFEST).expect("extension.toml parses")
}

#[test]
fn the_mixer_pane_has_a_render_and_an_export_button() {
    let m = manifest();
    let bar = m
        .contributes
        .toolbars
        .iter()
        .find(|t| t.id == "output")
        .expect("the Render and Export toolbar");
    assert_eq!(bar.at, "view:studio.mixer/mixer", "on the mixer's own pane");
    let buttons: Vec<&str> = bar.items.iter().map(|i| i.action.as_str()).collect();
    assert_eq!(buttons, ["STUDIO_MIXER_RENDER", "STUDIO_MIXER_EXPORT"]);
    for item in &bar.items {
        assert!(
            !item.label.is_empty() && !item.icon.is_empty(),
            "{}",
            item.action
        );
    }
}

#[test]
fn each_button_runs_the_engine_s_action_with_nothing_given() {
    let m = manifest();
    for (button, target) in [
        ("STUDIO_MIXER_RENDER", "MIX_RENDER"),
        ("STUDIO_MIXER_EXPORT", "MIX_EXPORT"),
    ] {
        let action = m
            .actions
            .iter()
            .find(|a| a.id == button)
            .unwrap_or_else(|| panic!("{button} is declared"));
        let preset = action.preset.as_ref().expect("a preset");
        assert_eq!(preset.action, target, "{button}");
        let args: serde_json::Value =
            serde_json::from_str(preset.args.as_deref().unwrap_or("{}")).expect("JSON");
        assert_eq!(
            args,
            serde_json::json!({}),
            "{button}: the engine's defaults"
        );
        // The host lets a preset through only to a namespace the
        // extension named (architect-extensions' gate).
        assert!(
            m.capabilities
                .actions
                .iter()
                .any(|p| p == "*" || target.starts_with(p.as_str())),
            "{target} is not among {:?}",
            m.capabilities.actions
        );
    }
}

//! Which views there are, what opens each, and which button is lit.

use studio_views::{
    NEEDS_ENGINE, Opens, SETLISTS, VIEWS, WITH_SONG_WORKSPACE, by_action, checked, lit, moves_lit,
};

#[test]
fn the_owners_six_in_the_bars_order() {
    let labels: Vec<&str> = VIEWS.iter().map(|v| v.label).collect();
    assert_eq!(
        labels,
        [
            "Arrangement",
            "Mixer",
            "Chart",
            "Chart Editor",
            "Progress Bar",
            "Editor"
        ]
    );
    // Every button an action of this extension's, each its own.
    let mut actions: Vec<&str> = VIEWS
        .iter()
        .chain(SETLISTS.iter())
        .map(|v| v.action)
        .collect();
    assert!(actions.iter().all(|a| a.starts_with("STUDIO_VIEWS_")));
    actions.sort_unstable();
    actions.dedup();
    assert_eq!(actions.len(), VIEWS.len() + SETLISTS.len());
}

#[test]
fn a_view_that_needs_the_engine_says_so() {
    for label in ["Arrangement", "Mixer", "Editor"] {
        let v = VIEWS.iter().find(|v| v.label == label).unwrap();
        assert_eq!(v.opens, Opens::Missing(NEEDS_ENGINE), "{label}");
        assert_eq!(v.view_kind(), None);
    }
    for label in ["Chart", "Chart Editor", "Progress Bar"] {
        let v = VIEWS.iter().find(|v| v.label == label).unwrap();
        assert_eq!(v.opens, Opens::Missing(WITH_SONG_WORKSPACE), "{label}");
    }
}

#[test]
fn the_focused_views_button_is_lit_and_only_it() {
    assert_eq!(
        lit("studio-setlists/setlist").map(|v| v.action),
        Some("STUDIO_VIEWS_SETLISTS")
    );
    let states = checked("studio-setlists/song");
    assert_eq!(states.iter().filter(|(_, on)| *on).count(), 1);
    assert!(states.contains(&("STUDIO_VIEWS_SONG", true)));
    assert!(states.contains(&("STUDIO_VIEWS_ARRANGEMENT", false)));
    // Another view focused (a note): none lit, every one said off.
    let states = checked("note");
    assert!(states.iter().all(|(_, on)| !on));
    assert_eq!(states.len(), VIEWS.len() + SETLISTS.len());
    assert_eq!(
        by_action("STUDIO_VIEWS_MIXER").map(|v| v.label),
        Some("Mixer")
    );
}

#[test]
fn pressing_a_toolbar_button_does_not_unlight_the_view_in_front() {
    // The toolbar's pane takes the focus when its button is pressed.
    assert!(!moves_lit("toolbar:studio-views/views"));
    assert!(moves_lit("studio-setlists/setlist"));
    assert!(moves_lit("note"));
}

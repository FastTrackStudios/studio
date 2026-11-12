//! Which views there are, what opens each, and which button is lit.

use studio_views::{
    NEEDS_ENGINE, Opens, StudioView, VIEWS, WITH_SONG_WORKSPACE, by_action, checked, checked_among,
    lit, moves_lit,
};

#[test]
fn the_owners_six_in_the_ipad_bars_order_and_words_and_no_others() {
    // The iPad bar's order and words for the four it has (Chart, Arrange,
    // Mixer, Editor), the song's two other panels after the Chart.
    let labels: Vec<&str> = studio_views::all().map(|v| v.label).collect();
    assert_eq!(
        labels,
        [
            "Chart",
            "Chart Editor",
            "Progress",
            "Arrange",
            "Mixer",
            "Editor"
        ]
    );
    // Every button an action of this extension's, each its own.
    let mut actions: Vec<&str> = VIEWS.iter().map(|v| v.action).collect();
    assert!(actions.iter().all(|a| a.starts_with("STUDIO_VIEWS_")));
    actions.sort_unstable();
    actions.dedup();
    assert_eq!(actions.len(), VIEWS.len());
    assert_eq!(
        by_action("STUDIO_VIEWS_MIXER").map(|v| v.label),
        Some("Mixer")
    );
}

#[test]
fn a_view_that_is_not_there_says_why() {
    for label in ["Arrange", "Mixer", "Editor"] {
        let v = VIEWS.iter().find(|v| v.label == label).unwrap();
        assert_eq!(v.opens, Opens::Missing(NEEDS_ENGINE), "{label}");
        assert_eq!(v.view_kind(), None);
    }
    for label in ["Chart", "Chart Editor", "Progress"] {
        let v = VIEWS.iter().find(|v| v.label == label).unwrap();
        assert_eq!(v.opens, Opens::Missing(WITH_SONG_WORKSPACE), "{label}");
    }
    // Each with the issue that makes it one.
    let issues: Vec<&str> = VIEWS.iter().map(|v| v.issue).collect();
    assert_eq!(
        issues,
        [
            "studio#435",
            "studio#436",
            "studio#437",
            "studio#433",
            "studio#434",
            "studio#438"
        ]
    );
    // None is a view yet: whatever is focused, none is lit.
    assert_eq!(lit("studio-views/mixer"), None);
    assert!(checked("note").iter().all(|(_, on)| !on));
    assert_eq!(checked("note").len(), VIEWS.len());
}

#[test]
fn the_focused_views_button_is_lit_and_only_it() {
    // Once a button's view is registered, it is lit while focused.
    let views = [
        StudioView {
            action: "STUDIO_VIEWS_MIXER",
            label: "Mixer",
            icon: "sliders-horizontal",
            opens: Opens::View {
                extension: "studio-views",
                view: "mixer",
            },
            issue: "",
        },
        VIEWS[3],
    ];
    let states = checked_among(&views, "studio-views/mixer");
    assert_eq!(
        states,
        [
            ("STUDIO_VIEWS_MIXER", true),
            ("STUDIO_VIEWS_ARRANGEMENT", false)
        ]
    );
    assert!(checked_among(&views, "note").iter().all(|(_, on)| !on));
}

#[test]
fn pressing_a_toolbar_button_does_not_unlight_the_view_in_front() {
    // The toolbar's pane takes the focus when its button is pressed.
    assert!(!moves_lit("toolbar:studio-views/views"));
    assert!(moves_lit("studio-views/mixer"));
    assert!(moves_lit("note"));
}

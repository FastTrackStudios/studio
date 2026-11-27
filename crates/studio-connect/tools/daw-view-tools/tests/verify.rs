//! The gallery's manifest held to the pixels under the built-in look: each
//! token a shot lists, set alone to a sentinel, changes that shot — except
//! the ones below, each for the reason given. The list is exact, so a view
//! that starts drawing one of them (or stops drawing another) fails here
//! and the list is updated with it.

/// Tokens no shot's pixels change under the built-in look, and why.
const UNSEEN: &[(&str, &str)] = &[
    // Read only by a pack's own pages (as `$name`, or handed to them as
    // fields): the built-in look has no pages to draw them with.
    ("text.bright", "pack pages only"),
    ("radius.control", "pack pages only"),
    ("size.label", "pack pages only"),
    ("size.small", "pack pages only"),
    ("font.ui", "pack pages only"),
    ("plate", "pack pages only"),
    ("sheen", "pack pages only"),
    ("monitor.tape", "pack pages only"),
    ("route.out", "pack pages only"),
    ("route.send", "pack pages only"),
    ("route.recv", "pack pages only"),
    ("meter.well", "pack pages only"),
    ("strip.live", "a switch a pack's page reads"),
    ("strip.touch", "a switch, and no shot is a touch layout"),
    // The kit's chrome palette (`skin::CHROME`): set, but the native
    // painters these shots run draw with the palette's own fields instead.
    ("selected", "chrome palette, not painted natively"),
    ("surface", "chrome palette, not painted natively"),
    ("surface.raised", "chrome palette, not painted natively"),
    ("border", "chrome palette, not painted natively"),
    ("combo", "mapped, not painted in these shots"),
    ("tcp.column", "mapped, not painted in these shots"),
    // Mapped to the arrangement's palette and not painted in any shot.
    ("grid.sub", "mapped, not painted in these shots"),
    ("clip.edge", "mapped, not painted in these shots"),
    ("clip.alpha", "mapped, not painted in these shots"),
    // The editor.
    (
        "editor.control_selected",
        "the editor's look has it; no painter reads it",
    ),
    (
        "editor.drum_cymbal",
        "GM crash, china and splash lanes take editor.drum_other",
    ),
    (
        "editor.surface_inset",
        "lane boxes the gallery's editors do not draw",
    ),
    (
        "editor.octave_line",
        "under the row tint at the gallery's zoom",
    ),
    ("editor.peaks", "the guide lane does not take it"),
];

#[test]
#[ignore = "known failure, studio#779: surface and surface.raised read as drawn, as the unstable fx.phases shot makes every token it lists look seen; run it with --run-ignored"]
fn every_listed_token_changes_its_shot_but_the_known_ones() {
    let (golden, planned) = daw_view_tools::gallery::stage_session().expect("stages");
    let seen = daw_view_tools::gallery::verify::verify(&golden, &planned).expect("verifies");
    let unseen: Vec<&str> = seen
        .iter()
        .filter(|s| s.shot.is_none())
        .map(|s| s.token)
        .collect();
    let expected: Vec<&str> = daw_theme::spec::TOKENS
        .iter()
        .map(|t| t.name)
        .filter(|n| UNSEEN.iter().any(|(u, _)| u == n))
        .collect();
    assert_eq!(unseen, expected);
}

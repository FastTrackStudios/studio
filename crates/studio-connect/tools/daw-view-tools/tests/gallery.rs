//! The gallery over the golden session, for the built-in look: every shot
//! renders, and renders the same bytes twice.
//!
//! One test, not several: the views draw from process-wide state (the
//! theme, the folds, the automation lanes), so the shots are drawn in one
//! place, in order.

use daw_theme::gallery::SHOTS;
use daw_view_tools::gallery;

#[test]
#[ignore = "known failure, studio#779: the fx.phases shot renders different bytes on a second render; run it with --run-ignored"]
fn every_shot_renders_and_renders_the_same_bytes_twice() {
    let (golden, planned) = gallery::stage_session().expect("the golden session stages");
    let stage = gallery::stage_theme(None).expect("the built-in look");
    let draw = || -> Vec<(&str, Vec<u8>)> {
        SHOTS
            .iter()
            .map(|shot| {
                let picture = gallery::render::shot(&golden, &planned, &stage, shot);
                assert_eq!(
                    (picture.width, picture.height),
                    (shot.size.width, shot.size.height),
                    "{}",
                    shot.name
                );
                let png = gallery::encode_png(&picture).expect("encodes");
                (shot.name, png)
            })
            .collect()
    };
    let first = draw();
    for (name, png) in &first {
        assert!(
            png.len() > 2_000,
            "{name} is all but empty ({} bytes)",
            png.len()
        );
    }
    let second = draw();
    for ((name, a), (_, b)) in first.iter().zip(&second) {
        assert!(a == b, "{name} drew different bytes the second time");
    }
}

#[test]
fn a_page_lists_what_was_drawn() {
    let looks = gallery::Looks::BuiltIn;
    let shots: Vec<_> = SHOTS.iter().collect();
    let page = daw_theme::gallery::readme(looks.title(), &looks.pictured(), &shots);
    assert!(page.contains("built-in/editor.drums.png"));
}

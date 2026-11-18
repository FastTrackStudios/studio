//! The rows rendered on the host (`dioxus-ssr`): what a screen reader is
//! told, which row is current, a song that cannot play, text escaped.

use dioxus::prelude::*;
use serde_json::json;

use super::Rows;
use crate::queue::{Now, Setlist, setlist_of};

#[component]
fn Harness(setlist: Setlist, now: Option<Now>, playable: Vec<String>) -> Element {
    rsx! { Rows { setlist, now, playable, on_press: |_| {} } }
}

fn render(setlist: Setlist, now: Option<Now>, playable: Vec<String>) -> String {
    let mut dom = VirtualDom::new_with_props(
        Harness,
        HarnessProps {
            setlist,
            now,
            playable,
        },
    );
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

const BODY: &str = "```studio-setlist\n\
    1 | Joy <b>to</b> \"the\" World | Joy.md | D · 120 bpm | Media/joy.ogg | Watts & Co | 151.0 | D |\n\
    ## Tree Lighting\n\
    2 | Silent Night | Silent.md | E · 60 bpm | | Gruber | 200.0 | E | F\n\
    ```\n";

fn list() -> Setlist {
    setlist_of(&json!({"note": {"path": "S.md", "title": "Eve <script>", "body": BODY}})).unwrap()
}

#[test]
fn the_rows_say_what_they_are_and_escape_their_text() {
    let s = list();
    let joy = s.sections[0].items[0].id.clone();
    let html = render(
        s,
        Some(Now {
            id: joy.clone(),
            time: 30.0,
            duration: 151.0,
            playing: true,
        }),
        vec![joy],
    );
    // No markup of a note's text reaches the page.
    assert!(
        !html.contains("<b>") && !html.contains("<script>"),
        "{html}"
    );
    assert!(html.contains("Joy &#60;b&#62;to&#60;/b&#62;"), "{html}");
    // The playing row: current, labelled with its number, artist and key.
    assert!(
        html.contains(r#"aria-label="1. Joy &#60;b&#62;to&#60;/b&#62; &#34;the&#34; World, Watts &#38; Co, Key of D, playing""#),
        "{html}"
    );
    assert_eq!(html.matches(r#"aria-current="true""#).count(), 1);
    assert!(html.contains("width: 19.87%"), "{html}");
    // The song with no recording: disabled, said so.
    assert!(
        html.contains(
            "2. Silent Night, Gruber, Key of F, transposed up 1 semitone from E, no recording"
        ),
        "{html}"
    );
    assert_eq!(html.matches(r#"aria-disabled="true""#).count(), 1);
    // Each named set is a list labelled by its heading.
    assert!(
        html.contains(r#"id="sl-set-1""#) && html.contains(r#"aria-labelledby="sl-set-1""#),
        "{html}"
    );
}

#[test]
fn a_row_not_queued_is_disabled_though_it_has_a_recording() {
    let html = render(list(), None, Vec::new());
    assert_eq!(html.matches(r#"aria-disabled="true""#).count(), 2);
    assert!(html.contains("cannot play here"), "{html}");
    assert!(!html.contains("aria-current"), "{html}");
}

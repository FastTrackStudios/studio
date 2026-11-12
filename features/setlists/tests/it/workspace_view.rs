//! A song's workspace mounted as the song view mounts it
//! (`setlists::workspace_view::SongWorkspace`), headless on Blitz
//! (`dioxus-test`): the panes where the owner put them, wide and on a
//! phone; the bar and the lyric line at the playhead marked; a click on a
//! bar, a section of the progress bar or a lyric line moving all of them.
#![cfg(feature = "extension")]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use dioxus::prelude::*;
use dioxus_test::DocumentTester;
use setlists::workspace::{Form, Loaded, lyrics_source};
use setlists::workspace_view::SongWorkspace;

const JOY: &str = include_str!("../fixtures/joy.kf");

#[derive(Props, Clone, PartialEq)]
struct Shown {
    form: Form,
    at: f64,
}

#[allow(non_snake_case)]
fn Workspace(props: Shown) -> Element {
    let loaded = Loaded {
        title: "Joy to the World".to_owned(),
        artist: Some("Isaac Watts".to_owned()),
        lyrics: lyrics_source(None, Some(JOY)),
        source: JOY.to_owned(),
        chart_file: None,
    };
    rsx! {
        div { style: "position:absolute; top:0; left:0; width:100%; height:100%;",
            SongWorkspace { loaded, form: props.form, at: props.at }
        }
    }
}

/// The workspace at `at`, once its chart is engraved (on a thread of its
/// own, a pause after it opens) and its lyrics have measured their pane
/// (they draw nothing at no size).
fn open(form: Form, at: f64, size: (u32, u32)) -> DocumentTester {
    let doc =
        DocumentTester::from_virtual_dom(VirtualDom::new_with_props(Workspace, Shown { form, at }))
            .with_window_size(size.0, size.1)
            .build();
    for _ in 0..400 {
        doc.drain();
        doc.relayout();
        let lyrics = doc
            .query_all("[data-at]")
            .immediately()
            .iter()
            .any(|l| l.attribute("data-bar").is_none());
        if doc.query("[data-chart-page]").immediately().is_ok() && lyrics {
            return doc;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the chart was never engraved");
}

fn panes(doc: &DocumentTester) -> Vec<String> {
    doc.query_all("[data-pane]")
        .immediately()
        .iter()
        .filter_map(|p| p.attribute("data-pane"))
        .collect()
}

/// The bar marked as being played.
fn marked_bar(doc: &DocumentTester) -> Option<String> {
    doc.query("[aria-current=\"time\"]")
        .immediately()
        .ok()
        .and_then(|b| b.attribute("data-bar"))
}

/// The lyric line lit: the prompter's large one.
fn lit_line(doc: &DocumentTester) -> Option<String> {
    doc.query_all("[data-at]")
        .immediately()
        .into_iter()
        .filter(|l| l.attribute("data-bar").is_none())
        .find(|l| {
            l.attribute("style")
                .is_some_and(|s| s.contains("font-weight:800"))
        })
        .map(|l| l.inner_html())
}

fn click(doc: &DocumentTester, selector: &str) {
    doc.query(selector).immediately().unwrap().click();
    doc.drain();
    doc.drain();
}

#[test]
fn wide_the_progress_over_the_editor_and_lyrics_beside_the_chart() {
    let doc = open(Form::Wide, 12.5, (1600, 1000));
    assert!(
        doc.query("[data-workspace=\"columns\"]")
            .immediately()
            .is_ok()
    );
    assert_eq!(panes(&doc), ["Progress", "Editor", "Lyrics", "Chart"]);
    // The editor hidden: the lyrics have the left column.
    click(&doc, "[data-toggle=\"editor\"]");
    assert_eq!(panes(&doc), ["Progress", "Lyrics", "Chart"]);
}

#[test]
fn a_phone_stacks_them_and_keeps_the_editor_behind_its_toggle() {
    let doc = open(Form::Narrow, 12.5, (390, 1400));
    assert!(
        doc.query("[data-workspace=\"stack\"]")
            .immediately()
            .is_ok()
    );
    assert_eq!(panes(&doc), ["Progress", "Chart", "Lyrics"]);
    click(&doc, "[data-toggle=\"editor\"]");
    assert_eq!(panes(&doc), ["Progress", "Editor", "Chart", "Lyrics"]);
}

#[test]
fn the_bar_and_the_line_at_the_playhead_are_marked_and_a_click_seeks() {
    let doc = open(Form::Wide, 12.5, (1600, 1000));
    // 12.5 s: bar 10 (bar 1 at 1 s, 1.2 s a bar), the verse's second line.
    assert_eq!(marked_bar(&doc).as_deref(), Some("10"));
    assert!(
        lit_line(&doc).is_some_and(|l| l.contains("Let earth receive her King")),
        "{:?}",
        lit_line(&doc)
    );

    // A bar of the chorus: it is marked, and its line lit.
    click(&doc, "[data-bar=\"14\"]");
    assert_eq!(marked_bar(&doc).as_deref(), Some("14"));
    assert!(
        lit_line(&doc).is_some_and(|l| l.contains("Let every heart")),
        "{:?}",
        lit_line(&doc)
    );

    // A lyric line (the second verse's first): the bar it is sung on.
    let line = doc
        .query_all("[data-at]")
        .immediately()
        .into_iter()
        .filter(|l| l.attribute("data-bar").is_none())
        .find(|l| l.inner_html().contains("Joy to the earth"))
        .unwrap();
    line.click();
    doc.drain();
    doc.drain();
    assert_eq!(marked_bar(&doc).as_deref(), Some("21"));

    // The progress bar's first section (the intro): the first bar.
    let sections = doc.query_all("[title=\"Intro\"]").immediately();
    sections[0].click();
    doc.drain();
    doc.drain();
    assert_eq!(marked_bar(&doc).as_deref(), Some("1"));
}

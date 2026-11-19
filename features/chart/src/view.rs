//! The Chart pane: the engraved chart of the song playing, a page at a
//! time, the bar and the beat at the playhead marked, over Session's
//! progress bar (`session::ui::components::progress`, through
//! `setlists::workspace_view::Progress`). A bar or a section pressed plays
//! from there (`studio.setlists`' `seek`; while Studio's engine is the
//! clock, Studio is played in Studio and the pane says it follows it). On
//! a phone (a pane under 600 px) the page runs across its width and
//! scrolls.

use std::sync::Arc;

use dioxus::prelude::*;
use setlists::now::Reader;
use setlists::panes::{
    FILL, NARROW, Playhead, clock_said, engrave_async, item_of, said, shown, title_of, use_note,
    use_playhead,
};
use setlists::workspace_view::{ChartPages, Engraved, Palette, Progress, clock_text};

/// The chart of the song playing, following the playhead bar by bar and
/// beat by beat.
#[allow(non_snake_case)]
pub fn ChartView() -> Element {
    let palette = Palette::of(&architect_theme::use_resolved());
    let note = use_note();
    let playhead = use_playhead(Reader::Service(crate::ext::handle()));
    let state = playhead.state.read().clone();
    match shown(state.as_ref(), note.as_deref()) {
        None => said(
            &palette,
            "Nothing playing. Play a song of a setlist (its Setlist tab, the Now Playing bar, or Studio) and its chart follows here.",
        ),
        Some((path, arrangement, following)) => rsx! {
            ChartOf { key: "{path}#{arrangement:?}", path, arrangement, following, playhead, palette }
        },
    }
}

#[component]
fn ChartOf(
    path: String,
    arrangement: Option<String>,
    following: bool,
    playhead: Playhead,
    palette: Palette,
) -> Element {
    let (p, a) = (path.clone(), arrangement.clone());
    // Read again after the person lets it read the vault.
    let mut attempt = use_signal(|| 0_u32);
    let engraved = use_resource(move || {
        let (path, arrangement) = (p.clone(), a.clone());
        let _ = attempt();
        async move {
            let h = crate::ext::handle();
            let loaded = setlists::ext::load_on(
                &h,
                &path,
                &title_of(&path),
                &[],
                arrangement.as_deref(),
                false,
            )
            .await;
            if loaded.source().trim().is_empty() {
                let why = empty_why(&loaded.chart);
                return Err((loaded.title, why));
            }
            match engrave_async(loaded.source().to_owned()).await {
                Ok((song, pages, _)) => {
                    Ok((loaded.title, Engraved(Arc::new(song), Arc::new(pages))))
                }
                Err(why) => Err((loaded.title, format!("The chart does not engrave: {why}"))),
            }
        }
    });
    let (size, mut node) = setlists::workspace_view::use_measured();
    let t = if following { (playhead.t)() } else { 0.0 };
    let id = item_of(&path, arrangement.as_deref());
    let seeking = playhead.clone();
    let seek = use_callback(move |at: f64| seeking.seek(id.clone(), at));
    let p = palette.clone();
    let body = match &*engraved.read() {
        None => said(&p, "Engraving the chart…"),
        Some(Err((_, why))) if why.contains("refused (folders)") => rsx! {
            div { style: "{FILL} display:flex; flex-direction:column; align-items:center; justify-content:center; \
                          gap:12px; padding:24px; box-sizing:border-box; text-align:center; color:{p.ink_2}; \
                          font-size:13px; background:{p.desk};",
                "data-pane-empty": "folders",
                span { "This vault is outside the folders Chart may read." }
                button {
                    style: "{p.button()}",
                    "data-allow": "vault",
                    onclick: move |_| {
                        spawn(async move {
                            let h = crate::ext::handle();
                            if setlists::host::allow_vault_on(&h, "Let Chart read this vault's charts").await.is_ok() {
                                attempt += 1;
                            }
                        });
                    },
                    "Let Chart read this vault"
                }
            }
        },
        Some(Err((_, why))) => said(&p, why),
        Some(Ok((title, song))) => {
            let (_, end) = song.0.span();
            let across = size().0 > 0.0 && size().0 < NARROW;
            let scroll = if across { "overflow-y:scroll;" } else { "" };
            rsx! {
                div { style: "flex:none; display:flex; flex-direction:column; gap:6px; padding:8px 10px 6px;",
                    div { style: "display:flex; align-items:center; gap:8px; font-size:13px; color:{p.ink};",
                        span { style: "flex:1; min-width:0; overflow:hidden; white-space:nowrap; font-weight:700;", "{title}" }
                        span { style: "flex:none; color:{p.ink_2}; font-size:11px;", "data-clock": "{clock_said(following)}", "{clock_said(following)}" }
                        span { style: "flex:none; color:{p.ink_2}; font-variant-numeric:tabular-nums;",
                            "{clock_text(t)} / {clock_text(end)}" }
                    }
                    Progress { song: Some(song.clone()), now: t, on_seek: seek, labels: !across }
                }
                div { style: "position:relative; flex:1; min-height:0; {scroll}",
                    ChartPages { song: song.clone(), now: t, on_seek: seek, across, palette: Some(p.clone()) }
                }
            }
        }
    };
    rsx! {
        div {
            "data-studio-chart": "{title_of(&path)}",
            style: "{FILL} display:flex; flex-direction:column; background:{palette.desk}; color:{palette.ink}; \
                    font-family:system-ui, sans-serif;",
            onmounted: move |e| node.set(Some(e.data())),
            {body}
        }
    }
}

/// Why a song shows no chart: it names none, or its `.kf` did not read.
fn empty_why(chart: &setlists::workspace::ChartAccess) -> String {
    use setlists::workspace::ChartFrom;
    match (&chart.from, &chart.file) {
        (ChartFrom::NoteCopy { why }, Some(file)) => {
            format!("The chart {file} could not be read: {why}")
        }
        (ChartFrom::File, Some(file)) => format!("The chart {file} is empty."),
        _ => "This song has no chart.".to_owned(),
    }
}

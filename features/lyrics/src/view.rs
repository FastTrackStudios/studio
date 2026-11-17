//! The Lyrics pane: the timed lyrics of the song playing as Session's
//! stage display (`lyrics_ui`, its Performer view: dark in any theme),
//! the line sung marked. A line pressed plays from there
//! (`studio.setlists`' `seek`; refused while Studio's engine is the clock).

use std::sync::Arc;

use dioxus::prelude::*;
use lyrics_ui::view::{LyricsChoice, LyricsView as Lyrics, View as LyricsMode};
use setlists::now::Reader;
use setlists::panes::{
    FILL, Playhead, engrave_async, item_of, said, shown, title_of, use_note, use_playhead,
};
use setlists::workspace_view::{Palette, words_of};

/// The lyrics of the song playing, as Session's stage display.
#[allow(non_snake_case)]
pub fn LyricsView() -> Element {
    use_context_provider(|| LyricsChoice::of(LyricsMode::Performer));
    let palette = Palette::of(&architect_theme::use_resolved());
    let note = use_note();
    let playhead = use_playhead(Reader::Service(crate::ext::handle()));
    let state = playhead.state.read().clone();
    match shown(state.as_ref(), note.as_deref()) {
        None => said(
            &palette,
            "Nothing playing. Play a song of a setlist and its lyrics follow here.",
        ),
        Some((path, arrangement, following)) => rsx! {
            LyricsOf { key: "{path}#{arrangement:?}", path, arrangement, following, playhead }
        },
    }
}

#[component]
fn LyricsOf(
    path: String,
    arrangement: Option<String>,
    following: bool,
    playhead: Playhead,
) -> Element {
    let (p, a) = (path.clone(), arrangement.clone());
    // The lyrics never wait on the chart: drawn as soon as they are read,
    // drawn again against the chart's sections once it is engraved.
    // Read again after the person lets it read the vault.
    let mut attempt = use_signal(|| 0_u32);
    let loaded = use_resource(move || {
        let (path, arrangement) = (p.clone(), a.clone());
        let _ = attempt();
        async move {
            let h = crate::ext::handle();
            setlists::ext::load_on(
                &h,
                &path,
                &title_of(&path),
                &[],
                arrangement.as_deref(),
                false,
            )
            .await
        }
    });
    let song = use_resource(move || async move {
        let text = loaded.read().as_ref().map(|l| l.source().to_owned())?;
        if text.trim().is_empty() {
            return None;
        }
        engrave_async(text).await.ok().map(|(s, _, _)| Arc::new(s))
    });
    let t = if following { (playhead.t)() } else { 0.0 };
    let id = item_of(&path, arrangement.as_deref());
    let seeking = playhead.clone();
    let seek = use_callback(move |at: f64| seeking.seek(id.clone(), at));
    let Some(loaded) = loaded.read().clone() else {
        return rsx! { div { style: "{FILL} background:#07080b;" } };
    };
    let engraved = song.read().clone().flatten();
    let words = loaded.lyrics.as_ref().map(|l| {
        words_of(
            Some(l),
            engraved.as_deref(),
            &loaded.title,
            loaded.artist.as_deref(),
        )
    });
    // The vault outside the folders it may read: its `.lrc` was refused
    // (what shows is its chart's lyric lane, if any).
    let refused = matches!(
        &loaded.chart.from,
        setlists::workspace::ChartFrom::NoteCopy { why } if why.contains("refused (folders)")
    );
    let empty = "No timed lyrics for this song: give it an LRC, or its chart a timed lyric lane ([lyrics lrc]).";
    rsx! {
        div { "data-studio-lyrics": "{loaded.title}", style: "{FILL} background:#07080b;",
            if refused {
                div {
                    style: "position:absolute; left:0; right:0; bottom:0; z-index:2; display:flex; align-items:center; \
                            justify-content:center; gap:10px; padding:8px; background:#15161a; color:#8b9099; \
                            font-size:12px; font-family:system-ui, sans-serif;",
                    "data-pane-empty": "folders",
                    span { "This vault is outside the folders Lyrics may read: its .lrc is not shown." }
                    button {
                        style: "height:24px; padding:0 10px; border-radius:5px; border:1px solid #2a2c31; \
                                background:#1f2026; color:#e5e7eb; font-size:12px; cursor:pointer;",
                        "data-allow": "vault",
                        onclick: move |_| {
                            spawn(async move {
                                let h = crate::ext::handle();
                                if setlists::host::allow_vault_on(&h, "Let Lyrics read this vault's lyrics").await.is_ok() {
                                    attempt += 1;
                                }
                            });
                        },
                        "Let Lyrics read this vault"
                    }
                }
            }
            match words {
                Some(words) => rsx! {
                    Lyrics { words, at: t, on_seek: seek, start_size: (640.0, 360.0), empty: empty.to_owned() }
                },
                None => rsx! {
                    div { style: "{FILL} display:flex; align-items:center; justify-content:center; padding:24px; \
                                  box-sizing:border-box; text-align:center; color:#8b9099; font-size:13px; \
                                  font-family:system-ui, sans-serif;",
                        "{empty}" }
                },
            }
        }
    }
}

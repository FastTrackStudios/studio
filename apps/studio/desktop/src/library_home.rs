//! The library (`library::ui`) as Studio's welcome screen. The library
//! knows nothing of engines or of the DAW: this hands it the window's size
//! in points, kept current through a rotation, the app's pieces of it
//! (the account in its bar, the ways in at the top of Home), and turns what
//! a press opens into where it goes ([`destination`]): a part into the rig
//! view, a setlist or a session straight into the DAW.

use dioxus::prelude::*;
use library::Library;
use library::model::{RigKind, SpaceId};
#[cfg(target_os = "macos")]
use library::ui::BarInset;
use library::ui::{LibraryApp, Open, Viewport};

/// Where something opened in the library goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    /// The rig view, on the rig of this kind (`None`: the one played last).
    Rig(Option<RigKind>),
    /// The DAW, on a setlist of a space's (an org's) library, by its title.
    Setlist(String, SpaceId),
    /// The DAW, on a session, by its title (found on this device).
    Session(String),
    /// Nothing the library knows by that id.
    Nowhere,
}

/// Where `open` goes, read from the library it was opened in.
#[must_use]
pub fn destination(lib: &Library, open: Open) -> Destination {
    match open {
        Open::Part(id) => match lib.parts.iter().find(|p| p.id == id) {
            Some(part) => Destination::Rig(lib.rig(part.rig).map(|r| r.kind)),
            None => Destination::Nowhere,
        },
        Open::Rehearse(id) => lib.setlist(id).map_or(Destination::Nowhere, |s| {
            Destination::Setlist(s.title.clone(), s.space)
        }),
        Open::Session(id) => lib.session(id).map_or(Destination::Nowhere, |s| {
            Destination::Session(s.title.clone())
        }),
    }
}

/// The welcome screen: the library, at the window's size.
#[component]
pub fn LibraryHome() -> Element {
    let window = dioxus_native::use_window();
    // A Mac draws its window buttons over the bar's left end.
    #[cfg(target_os = "macos")]
    use_context_provider(|| BarInset(crate::front::MAC_WINDOW_BUTTONS + 4));
    let mut size = use_context_provider(|| Viewport(Signal::new((1194.0, 834.0))));
    use_future(move || {
        let window = window.clone();
        async move {
            loop {
                let px = window.surface_size();
                if px.height > 0 {
                    let scale = window.scale_factor().max(0.1);
                    let pt = (
                        (f64::from(px.width) / scale) as f32,
                        (f64::from(px.height) / scale) as f32,
                    );
                    let was = *size.0.peek();
                    if (was.0 - pt.0).abs() > 1.0 || (was.1 - pt.1).abs() > 1.0 {
                        size.0.set(pt);
                    }
                }
                futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
            }
        }
    });
    rsx! { LibraryApp {} }
}

// ── The ways in, at the top of Home ─────────────────────────────────────────
//
// What the earlier homes had and the library did not: a new project, joining
// a session happening elsewhere by its live link (or a REAPER bridge's
// address), the public demo, and the projects opened lately. Each is handed
// to the DAW as an `Entry` (`session_desktop::native::handoff`) and goes
// straight into it.

#[cfg(feature = "session-app")]
pub use ways_in::{Doors, WaysIn};

#[cfg(feature = "session-app")]
mod ways_in {
    use dioxus::prelude::*;
    use session_desktop::native::handoff::{self, Entry};

    const SHEET: &str = "#0e0e11";
    const RULE: &str = "#222228";
    const RULE_STRONG: &str = "#2b2b31";
    const INK: &str = "#e4e4e7";
    const INK_3: &str = "#80808a";
    const PRIMARY: &str = "#2563eb";
    const FONT: &str =
        "-apple-system, 'SF Pro Text', system-ui, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif";

    /// How many of the projects opened lately Home lists.
    const RECENT: usize = 6;

    /// What a way in opens: into the DAW.
    #[derive(Clone, Copy)]
    pub struct Doors(pub Callback<Entry>);

    /// New project, join by a link or the demo, and the recent projects.
    #[component]
    pub fn WaysIn() -> Element {
        let Doors(enter) = use_context::<Doors>();
        let mut link = use_signal(String::new);
        let join = move |text: String| {
            let text = text.trim().to_owned();
            if !text.is_empty() {
                enter.call(Entry::Join(text));
            }
        };
        let empty = link().trim().is_empty();
        let join_opacity = if empty { "0.45" } else { "1" };
        let recent = use_hook(handoff::recent);
        let account = use_context::<handoff::WelcomeAccount>();
        let library = match account.lists() {
            handoff::WelcomeLists::Out => Some((
                "Bring your library".to_owned(),
                "Sign in with FastTrackStudio: your orgs' setlists and songs show here.".to_owned(),
                true,
            )),
            handoff::WelcomeLists::Reading => Some((
                "Reading your library…".to_owned(),
                "Your orgs' setlists and songs, from Task.".to_owned(),
                false,
            )),
            handoff::WelcomeLists::Failed(why) => {
                Some(("Could not read your library".to_owned(), why, false))
            }
            handoff::WelcomeLists::Ready(_) => None,
        };
        rsx! {
            if let Some((title, detail, sign_in)) = library {
                div {
                    style: "margin-top: 16px; display: flex; align-items: center; gap: 16px; padding: 16px 18px; \
                            border: 1px solid {RULE}; border-radius: 12px; background: {SHEET}; font-family: {FONT}; color: {INK};",
                    div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                        span { style: "font-size: 16px; font-weight: 750;", "{title}" }
                        span { style: "font-size: 13px; color: {INK_3}; line-height: 1.4;", "{detail}" }
                    }
                    if sign_in {
                        button {
                            class: "lp",
                            style: "flex-shrink: 0; height: 44px; padding: 0 20px; border: none; border-radius: 8px; \
                                    background: {PRIMARY}; color: #ffffff; font-family: {FONT}; font-size: 15px; \
                                    font-weight: 750; cursor: pointer;",
                            onclick: move |_| account.sign_in(),
                            "Sign in"
                        }
                    }
                }
            }
            div {
                style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 14px; \
                        padding: 16px 0 8px; font-family: {FONT}; color: {INK};",
                // A song from nothing, and the demo anyone can join.
                div { style: "display: flex; flex-direction: column; gap: 10px; min-width: 0;",
                    Door {
                        title: "New project".to_owned(),
                        detail: "An untitled song, in Organize, the click ready".to_owned(),
                        primary: true,
                        on_press: move |()| enter.call(Entry::New),
                    }
                    Door {
                        title: "The demo".to_owned(),
                        detail: "A live set anyone can join".to_owned(),
                        primary: false,
                        on_press: move |()| join("demo".to_owned()),
                    }
                }
                // A session happening elsewhere.
                div {
                    style: "min-width: 0; display: flex; flex-direction: column; gap: 10px; padding: 16px 18px; \
                            border: 1px solid {RULE}; border-radius: 12px; background: {SHEET}; box-sizing: border-box;",
                    span { style: "font-size: 16px; font-weight: 750;", "Join with a link" }
                    span { style: "font-size: 13px; color: {INK_3}; line-height: 1.4;",
                        "A live link someone shared, or a REAPER bridge's address."
                    }
                    div { style: "display: flex; gap: 8px;",
                        div { style: "position: relative; flex: 1; min-width: 0; height: 44px;",
                            // Blitz draws no `placeholder`: the hint is a
                            // span under the field while it is empty.
                            if empty {
                                span {
                                    style: "position: absolute; top: 0; left: 12px; height: 44px; display: flex; \
                                            align-items: center; font-size: 15px; color: {INK_3}; pointer-events: none; \
                                            white-space: nowrap; overflow: hidden;",
                                    "Paste a link"
                                }
                            }
                            input {
                                r#type: "text",
                                "aria-label": "Live link or address",
                                value: "{link}",
                                style: "position: absolute; top: 0; left: 0; width: 100%; height: 44px; padding: 0 12px; \
                                        border: 1px solid {RULE_STRONG}; border-radius: 8px; background: transparent; \
                                        color: {INK}; font-family: {FONT}; font-size: 15px; box-sizing: border-box;",
                                oninput: move |e| link.set(e.value()),
                                onfocus: move |_| library::ui::set_field_focused(true),
                                onblur: move |_| library::ui::set_field_focused(false),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter {
                                        join(link());
                                    }
                                    // The field's keys are its own: `?`
                                    // types a ?, not the shortcuts.
                                    if !e.modifiers().contains(Modifiers::META)
                                        && !e.modifiers().contains(Modifiers::CONTROL)
                                    {
                                        e.stop_propagation();
                                    }
                                },
                            }
                        }
                        button {
                            class: "lp",
                            disabled: empty,
                            style: "flex-shrink: 0; height: 44px; padding: 0 18px; border: none; border-radius: 8px; \
                                    background: {PRIMARY}; color: #ffffff; font-family: {FONT}; font-size: 15px; \
                                    font-weight: 750; cursor: pointer; opacity: {join_opacity};",
                            onclick: move |_| join(link()),
                            "Join"
                        }
                    }
                }
                // Songs on this machine, into the org's library.
                session_desktop::native::publish::Publish {
                    on_join: move |link: String| join(link),
                    on_typing: move |on: bool| library::ui::set_field_focused(on),
                }
                // What was opened lately, one press from opening again.
                if !recent.is_empty() {
                    div {
                        style: "min-width: 0; display: flex; flex-direction: column; border: 1px solid {RULE}; \
                                border-radius: 12px; background: {SHEET}; overflow: hidden;",
                        span { style: "padding: 14px 18px 8px; font-size: 12px; font-weight: 700; letter-spacing: 0.1em; \
                                       text-transform: uppercase; color: {INK_3};", "Opened lately" }
                        for song in recent.into_iter().take(RECENT) {
                            button {
                                key: "{song.opened.display()}",
                                class: "lp",
                                style: "width: 100%; min-height: 48px; display: flex; align-items: center; gap: 12px; \
                                        padding: 6px 18px; border: none; border-top: 1px solid {RULE}; background: transparent; \
                                        color: {INK}; font-family: {FONT}; text-align: left; cursor: pointer; \
                                        justify-content: flex-start; box-sizing: border-box;",
                                onclick: {
                                    let path = song.opened.clone();
                                    move |_| enter.call(Entry::Path(path.clone()))
                                },
                                span { style: "flex: 1; min-width: 0; font-size: 15px; font-weight: 650; overflow: hidden; white-space: nowrap;",
                                    "{song.name()}"
                                }
                                span { style: "flex-shrink: 0; font-size: 13px; color: {INK_3}; white-space: nowrap;",
                                    {match song.opened_alternative() {
                                        Some(alt) => format!("{alt} · {}", handoff::ago(song.at)),
                                        None => handoff::ago(song.at),
                                    }}
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// One press-to-open card: a title and a line under it.
    #[component]
    fn Door(title: String, detail: String, primary: bool, on_press: EventHandler<()>) -> Element {
        let (bg, edge) = if primary {
            ("rgba(37, 99, 235, 0.14)", "#1e3a8a")
        } else {
            (SHEET, RULE)
        };
        rsx! {
            button {
                class: "lp",
                "aria-label": "{title}",
                style: "width: 100%; min-height: 64px; display: flex; flex-direction: column; align-items: flex-start; \
                        justify-content: center; gap: 3px; padding: 10px 18px; border: 1px solid {edge}; \
                        border-radius: 12px; background: {bg}; color: {INK}; font-family: {FONT}; text-align: left; \
                        cursor: pointer; box-sizing: border-box;",
                onclick: move |_| on_press.call(()),
                span { style: "font-size: 16px; font-weight: 750;", "{title}" }
                span { style: "max-width: 100%; font-size: 13px; color: {INK_3}; overflow: hidden; white-space: nowrap;", "{detail}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use library::fixture;
    use library::ui::Open;

    use super::{Destination, destination};

    #[test]
    fn a_part_plays_on_its_rig() {
        let l = fixture::library();
        let part = l.parts.first().expect("the fixture has parts");
        let kind = l.rig(part.rig).map(|r| r.kind);
        assert_eq!(destination(&l, Open::Part(part.id)), Destination::Rig(kind));
    }

    #[test]
    fn a_setlist_and_a_session_open_in_the_daw_by_title() {
        let l = fixture::library();
        let set = l.setlists.first().expect("the fixture has setlists");
        assert_eq!(
            destination(&l, Open::Rehearse(set.id)),
            Destination::Setlist(set.title.clone(), set.space)
        );
        let session = l.sessions.first().expect("the fixture has sessions");
        assert_eq!(
            destination(&l, Open::Session(session.id)),
            Destination::Session(session.title.clone())
        );
    }

    #[test]
    fn what_the_library_does_not_have_goes_nowhere() {
        let mut l = fixture::library();
        let set = l.setlists.remove(0);
        assert_eq!(
            destination(&l, Open::Rehearse(set.id)),
            Destination::Nowhere
        );
    }
}

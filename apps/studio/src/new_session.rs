//! The New session sheet: a session made three ways, as the engine's
//! `SESSION_NEW` makes it. From scratch, with a title, key, tempo and form,
//! saved in the home's `Songs/<title>/`. From a folder of multitracks or
//! stems, a track per file. From a REAPER project, kept as it is. Once
//! made, the song goes in the org's library and the engine has it open:
//! the arrangement and mixer show it.

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::icons::{I, Icon};
use crate::state::use_app;

/// The way a session is made.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Way {
    Scratch,
    Folder,
    Rpp,
}

impl Way {
    const ALL: [Self; 3] = [Self::Scratch, Self::Folder, Self::Rpp];

    fn label(self) -> &'static str {
        match self {
            Self::Scratch => "From scratch",
            Self::Folder => "From a folder",
            Self::Rpp => "From a REAPER project",
        }
    }
}

/// What the sheet's fields hold.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Asked {
    pub title: String,
    pub key: String,
    pub tempo: String,
    /// The sections, one to a line or comma between (`Intro 4, Verse 8`).
    pub form: String,
    /// The folder or the `.RPP`.
    pub path: String,
}

/// `SESSION_NEW`'s arguments for `way` from what was typed; a song from
/// scratch with a title is saved as `<home>/Songs/<title>/<title>.session`.
///
/// # Errors
///
/// What is needed is not there (a title, a path), or the tempo is not a
/// number.
pub fn args(way: Way, asked: &Asked, home: Option<&std::path::Path>) -> Result<Value, String> {
    let text = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    let tempo = match text(&asked.tempo) {
        Some(t) => Some(
            t.parse::<f64>()
                .map_err(|_| format!("{t} is not a tempo: a number of beats a minute"))?,
        ),
        None => None,
    };
    let mut out = serde_json::Map::new();
    let mut put = |k: &str, v: Value| {
        out.insert(k.to_string(), v);
    };
    match way {
        Way::Scratch => {
            let title = text(&asked.title).ok_or("a new song needs its title")?;
            put("title", json!(title));
            if let Some(key) = text(&asked.key) {
                put("key", json!(key));
            }
            if let Some(bpm) = tempo {
                put("tempo", json!(bpm));
            }
            let form: Vec<String> = asked.form.split([',', '\n']).filter_map(text).collect();
            if !form.is_empty() {
                put("form", json!(form));
            }
            if let Some(home) = home {
                let file = home
                    .join("Songs")
                    .join(&title)
                    .join(format!("{title}.session"));
                put("to", json!(file.display().to_string()));
            }
        }
        Way::Folder | Way::Rpp => {
            let path = text(&asked.path).ok_or(if way == Way::Folder {
                "which folder? its path"
            } else {
                "which REAPER project? its path"
            })?;
            put("from", json!(path));
            if way == Way::Folder {
                if let Some(title) = text(&asked.title) {
                    put("title", json!(title));
                }
                if let Some(key) = text(&asked.key) {
                    put("key", json!(key));
                }
                if let Some(bpm) = tempo {
                    put("tempo", json!(bpm));
                }
            }
        }
    }
    Ok(Value::Object(out))
}

/// The key as the library writes it, from the KEY track's (`Eb major` is
/// `Eb`, `C# minor` is `C#m`); another mode as it is.
#[must_use]
#[cfg_attr(not(feature = "native"), allow(dead_code))]
pub fn library_key(key: &str) -> String {
    match key.rsplit_once(' ') {
        Some((root, "major")) => root.to_string(),
        Some((root, "minor")) => format!("{root}m"),
        _ => key.to_string(),
    }
}

/// What the engine said it made: its title, file, key and tempo.
#[derive(Clone, PartialEq, Debug)]
struct Made {
    title: String,
    file: Option<String>,
    key: Option<String>,
    bpm: Option<f64>,
}

impl Made {
    #[cfg_attr(not(feature = "native"), allow(dead_code))]
    fn of(info: &Value) -> Self {
        Self {
            title: info["title"].as_str().unwrap_or_default().to_string(),
            file: info["file"].as_str().map(str::to_string),
            key: info["key"].as_str().map(library_key),
            bpm: info["bpm"].as_f64(),
        }
    }
}

/// Make the session in the engine, off the UI thread.
#[cfg(feature = "native")]
async fn make(args: Value) -> Result<Made, String> {
    // The song's own folder, for a song from scratch.
    if let Some(dir) = args["to"]
        .as_str()
        .and_then(|t| std::path::Path::new(t).parent())
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let (tx, rx) = futures_channel::oneshot::channel();
    std::thread::Builder::new()
        .name("studio-new-session".into())
        .spawn(move || {
            let _ = tx.send(crate::engine::new_session(args));
        })
        .map_err(|e| format!("no thread to make it on: {e}"))?;
    let info = rx.await.map_err(|_| "the engine went away".to_string())??;
    Ok(Made::of(&info))
}

/// A page makes no session: the engine that makes one is the desktop's.
#[cfg(not(feature = "native"))]
async fn make(_args: Value) -> Result<Made, String> {
    Err("a new session is made in the desktop app".to_string())
}

#[component]
pub fn NewSessionSheet() -> Element {
    let app = use_app();
    let lib = crate::songs::use_library();
    let mut way = use_signal(|| Way::Scratch);
    let asked = use_signal(Asked::default);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let field = move |label: &'static str,
                      hint: &'static str,
                      get: fn(&Asked) -> &String,
                      set: fn(&mut Asked, String)| {
        let mut asked = asked;
        rsx! {
            label { class: "fieldrow",
                span { "{label}" }
                input {
                    value: "{get(&asked())}",
                    placeholder: "{hint}",
                    aria_label: "{label}",
                    oninput: move |v| asked.with_mut(|a| set(a, v.value())),
                }
            }
        }
    };
    let go = move |_| {
        let home = crate::storage::home();
        let args = match args(way(), &asked(), home.as_deref()) {
            Ok(a) => a,
            Err(e) => {
                error.set(Some(e));
                return;
            }
        };
        let typed_key = Some(asked().key.trim().to_string()).filter(|k| !k.is_empty());
        busy.set(true);
        error.set(None);
        let lib = lib.clone();
        spawn(async move {
            let made = make(args).await;
            busy.set(false);
            match made {
                Ok(m) => {
                    let id = session_sync::slug::slugify(&m.title);
                    if lib.get(&id).is_none() {
                        lib.put(&crate::songs::Entry {
                            id: id.clone(),
                            title: m.title.clone(),
                            artist: None,
                            key: typed_key.or(m.key.clone()),
                            tempo: m.bpm.map(|b| b.round() as u32),
                            tags: Vec::new(),
                            energy: 3,
                            ready: String::new(),
                            shared_with: Vec::new(),
                        });
                    }
                    app.close();
                    app.say(match &m.file {
                        Some(f) => format!("{}: a new session, {f}", m.title),
                        None => format!("{}: a new session, not saved yet", m.title),
                    });
                    navigator().push(crate::Route::SongPage { id });
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let w = way();
    let note = match w {
        Way::Scratch => {
            "The form's sections go in order from the song's start, each a kind and its bars. Saved in your Songs folder."
        }
        Way::Folder => {
            "Each audio file in the folder, and in folders inside it, becomes a track. The session is saved in the folder."
        }
        Way::Rpp => {
            "Its tracks, items, markers and tempo come with it. The session is saved beside it."
        }
    };
    rsx! {
        div { class: "bsheet", role: "dialog", aria_label: "New session",
            div { class: "bhead",
                b { "New session" }
            }
            div { class: "seg wrap",
                for k in Way::ALL {
                    button {
                        key: "{k.label()}",
                        class: "fopt",
                        aria_pressed: "{w == k}",
                        onclick: move |_| {
                            way.set(k);
                            error.set(None);
                        },
                        "{k.label()}"
                    }
                }
            }
            div { class: "settings",
                match w {
                    Way::Scratch => rsx! {
                        {field("Title", "The song's name", |a| &a.title, |a, v| a.title = v)}
                        {field("Key", "Eb, C#m, D minor", |a| &a.key, |a, v| a.key = v)}
                        {field("Tempo", "120", |a| &a.tempo, |a, v| a.tempo = v)}
                        {field("Form", "Intro 4, Verse 8, Chorus 8", |a| &a.form, |a, v| a.form = v)}
                    },
                    Way::Folder => rsx! {
                        {field("Folder", "The folder of stems", |a| &a.path, |a, v| a.path = v)}
                        {field("Title", "The folder's name", |a| &a.title, |a, v| a.title = v)}
                        {field("Key", "Eb, C#m, D minor", |a| &a.key, |a, v| a.key = v)}
                        {field("Tempo", "120", |a| &a.tempo, |a, v| a.tempo = v)}
                    },
                    Way::Rpp => rsx! {
                        {field("Project", "The .RPP file", |a| &a.path, |a, v| a.path = v)}
                    },
                }
            }
            div { class: "s1", style: "margin-top:8px", "{note}" }
            if let Some(e) = error() {
                div { class: "s1", role: "alert", style: "color:#f87171;margin-top:8px", "{e}" }
            }
            button {
                class: "primary wide",
                style: "margin-top:12px",
                disabled: busy(),
                onclick: go,
                Icon { name: I::Plus, size: 16 }
                if busy() { " Making…" } else { " Make the session" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn asked(title: &str, key: &str, tempo: &str, form: &str, path: &str) -> Asked {
        Asked {
            title: title.into(),
            key: key.into(),
            tempo: tempo.into(),
            form: form.into(),
            path: path.into(),
        }
    }

    #[test]
    fn from_scratch_it_is_saved_in_the_songs_folder() {
        let a = asked(" Morning Song ", "Eb", "96", "Intro 4, Verse 8\nChorus", "");
        assert_eq!(
            args(Way::Scratch, &a, Some(Path::new("/home/me/Studio"))),
            Ok(json!({
                "title": "Morning Song",
                "key": "Eb",
                "tempo": 96.0,
                "form": ["Intro 4", "Verse 8", "Chorus"],
                "to": "/home/me/Studio/Songs/Morning Song/Morning Song.session",
            }))
        );
    }

    #[test]
    fn from_scratch_it_needs_a_title_and_a_tempo_that_is_a_number() {
        assert!(args(Way::Scratch, &asked("", "", "", "", ""), None).is_err());
        assert!(args(Way::Scratch, &asked("Song", "", "fast", "", ""), None).is_err());
    }

    #[test]
    fn from_a_folder_it_takes_only_what_is_typed() {
        assert_eq!(
            args(
                Way::Folder,
                &asked("", "", "100", "Verse", "/stems/Night Drive"),
                None
            ),
            Ok(json!({ "from": "/stems/Night Drive", "tempo": 100.0 }))
        );
        assert!(args(Way::Folder, &asked("", "", "", "", " "), None).is_err());
    }

    #[test]
    fn from_a_project_it_is_only_the_project() {
        assert_eq!(
            args(
                Way::Rpp,
                &asked("X", "Eb", "90", "Verse", "/songs/Waltz.RPP"),
                None
            ),
            Ok(json!({ "from": "/songs/Waltz.RPP" }))
        );
    }

    #[test]
    fn the_library_writes_a_key_short() {
        assert_eq!(library_key("Eb major"), "Eb");
        assert_eq!(library_key("C# minor"), "C#m");
        assert_eq!(library_key("D dorian"), "D dorian");
    }
}

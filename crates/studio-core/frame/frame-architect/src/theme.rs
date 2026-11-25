//! A host's theme on the Action List: what an agent (over MCP), a script
//! or the palette inspects, edits and captures a running app's look with.
//!
//! [`register`] puts these on a list, over one
//! [`ThemeHost`](frame::theme::host::ThemeHost):
//!
//! | id | effect | |
//! |---|---|---|
//! | `THEME_INSPECT` | read-only | the theme: files (bytes, BLAKE3, draft), roles, tokens, errors |
//! | `THEME_ERRORS` | read-only | why the last compile failed, every problem, located |
//! | `THEME_APPLY` | mutating, asks | load another theme (layered paths, a mode) |
//! | `THEME_EDIT` | mutating, asks | replace one of its files' markup, unsaved |
//! | `THEME_RELOAD` | mutating | recompile from the files now |
//! | `THEME_SAVE` | destructive | write the drafts to their files |
//! | `THEME_REVERT` | destructive | every file back as loaded, byte for byte; drafts dropped |
//! | `VIEW_CAPTURE` | read-only | one role drawn on the CPU (Vello), as a PNG |
//!
//! A change that does not compile changes nothing: the last good theme
//! stays on screen, and the action fails with every problem (as
//! `THEME_ERRORS` then lists them). "Asks": once the app turns
//! confirmation on (`ActionList::require_confirmation`), a caller from
//! outside gets a ticket for a person to approve before the call runs —
//! for the destructive ones by their effect, for apply and edit by
//! `set_confirm`.
//!
//! None of it touches an engine: a theme is how a view is drawn, so an
//! edit, a failed reload or a capture never reaches the audio path.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use architect::action::{ActionError, ActionList, ActionMeta, Effect};
use base64::Engine as _;
use facet::Facet;
use frame::theme::host::{ThemeError, ThemeHost, ThemeInfo};
use frame::theme::runtime::{Rgba, Value};

const fn meta(
    id: &'static str,
    display_name: &'static str,
    description: &'static str,
) -> ActionMeta {
    ActionMeta {
        id,
        trait_name: "",
        method_name: "",
        display_name,
        description,
        category: "Frame",
        group: "Theme",
        shortcut: "",
        toggleable: false,
        undo: false,
    }
}

pub static INSPECT: ActionMeta = meta(
    "THEME_INSPECT",
    "Inspect the theme",
    "The theme the app draws with: its layers, every file it comes to (bytes, BLAKE3, whether an unsaved edit is over it), its mode and modes, the roles it draws (authored size, live), its tokens, and why the last compile failed, if it did.",
);
pub static ERRORS: ActionMeta = meta(
    "THEME_ERRORS",
    "Theme errors",
    "Why the last compile of the theme failed — of its files, or of an edit — every problem, each with its kind (NoFiles, Io, Parse, Compile, Request), file, line and column, role and node as far as known. Empty when it compiled.",
);
pub static APPLY: ActionMeta = meta(
    "THEME_APPLY",
    "Apply a theme",
    "Loads the theme layered from `paths` (base first; each a .frame file or a folder of them) in `mode` (the documents' first when left out), and watches them. Asks a person first. When it does not compile, the last good theme stays and this fails with every problem. The files as they are become what THEME_REVERT restores.",
);
pub static EDIT: ActionMeta = meta(
    "THEME_EDIT",
    "Edit a theme file",
    "Replaces `file` (one of THEME_INSPECT's files) with `markup`, unsaved, and draws with the theme it makes. Asks a person first. All or nothing: when it does not compile, the last good theme and the draft before it stay, and this fails with every problem. THEME_SAVE writes it.",
);
pub static RELOAD: ActionMeta = meta(
    "THEME_RELOAD",
    "Reload the theme",
    "Recompiles the theme from its files (and unsaved edits) now, changed or not. When it does not compile, the last good theme stays and this fails with every problem.",
);
pub static SAVE: ActionMeta = meta(
    "THEME_SAVE",
    "Save the theme",
    "Writes unsaved edits (THEME_EDIT) to their files when unchanged since editing began. Conflicts keep their drafts. Asks a person first. Answers the files written.",
);
pub static REVERT: ActionMeta = meta(
    "THEME_REVERT",
    "Revert the theme",
    "Puts every one of the theme's files back as it was when the theme was loaded (THEME_APPLY, or the app's start), byte for byte, drops unsaved edits and draws with the theme they make. Asks a person first. Answers the files rewritten.",
);
pub static CAPTURE: ActionMeta = meta(
    "VIEW_CAPTURE",
    "Capture a themed view",
    "Draws one instance of `role` from the current theme on the CPU (Vello) and answers it as a PNG (base64), with the BLAKE3 of its premultiplied RGBA8 pixels. `width`/`height` are logical px (the role's authored size when left out), `scale` device px per logical px (1). `fields` are the host data by schema name, as text: true/false, a number, #rrggbb[aa], or text. Host-painted slots (%name) are listed in `slots` and left empty.",
);

/// What `THEME_APPLY` takes.
#[derive(Debug, Clone, Facet)]
pub struct ApplyArgs {
    pub paths: Vec<String>,
    pub mode: Option<String>,
}

/// What `THEME_EDIT` takes.
#[derive(Debug, Clone, Facet)]
pub struct EditArgs {
    pub file: String,
    pub markup: String,
}

/// What a change answers: the theme's version after it, and the files it
/// wrote (save, revert).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Changed {
    pub version: u64,
    pub files: Vec<String>,
}

/// What `VIEW_CAPTURE` takes.
#[derive(Debug, Clone, Facet)]
pub struct CaptureArgs {
    pub role: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub scale: Option<f64>,
    #[facet(default)]
    pub fields: BTreeMap<String, String>,
}

/// What `VIEW_CAPTURE` answers.
#[derive(Debug, Clone, Facet)]
pub struct Captured {
    /// Device px.
    pub width: u32,
    pub height: u32,
    pub png_base64: String,
    /// BLAKE3 of the premultiplied RGBA8 pixels, hex: equal pixels, equal hash.
    pub blake3: String,
    /// The host slots shown, which a capture leaves empty.
    pub slots: Vec<String>,
}

/// Every problem, one per line, as a failed action says them.
fn failed(what: &str, errors: &[ThemeError]) -> String {
    let mut out = format!("{what}; the last good theme stays (THEME_ERRORS lists these):");
    for e in errors {
        out.push_str("\n- ");
        out.push_str(&e.to_string());
    }
    out
}

/// A field's value as an agent writes it.
fn value(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(n) = text.parse::<f64>() {
                Value::Num(n)
            } else if text.starts_with('#')
                && let Some(c) = Rgba::from_hex(text)
            {
                Value::Color(c)
            } else {
                Value::text(text)
            }
        }
    }
}

/// Put the theme actions on `list`, over `host`; apply and edit ask a
/// person first once the list asks anyone ([`ActionList::require_confirmation`]).
///
/// # Errors
/// [`ActionError::Duplicate`] when one of the ids is already on the list.
pub fn register(list: &ActionList, host: &Arc<ThemeHost>) -> Result<(), ActionError> {
    let h = host.clone();
    list.register(
        &INSPECT,
        Effect::ReadOnly,
        move |(): ()| -> Result<ThemeInfo, String> { Ok(h.inspect()) },
    )?;

    let h = host.clone();
    list.register(
        &ERRORS,
        Effect::ReadOnly,
        move |(): ()| -> Result<Vec<ThemeError>, String> { Ok(h.errors()) },
    )?;

    let h = host.clone();
    list.register(&APPLY, Effect::Mutating, move |a: ApplyArgs| {
        h.load(a.paths.into_iter().map(PathBuf::from).collect(), a.mode)
            .map_err(|e| failed("the theme did not compile", &e))?;
        Ok(Changed {
            version: h.version(),
            files: Vec::new(),
        })
    })?;
    list.set_confirm(APPLY.id, |_| true);

    let h = host.clone();
    list.register(&EDIT, Effect::Mutating, move |a: EditArgs| {
        h.edit(&PathBuf::from(a.file), a.markup)
            .map_err(|e| failed("the edit did not compile", &e))?;
        Ok(Changed {
            version: h.version(),
            files: Vec::new(),
        })
    })?;
    list.set_confirm(EDIT.id, |_| true);

    let h = host.clone();
    list.register(&RELOAD, Effect::Mutating, move |(): ()| {
        h.reload()
            .map_err(|e| failed("the theme did not compile", &e))?;
        Ok(Changed {
            version: h.version(),
            files: Vec::new(),
        })
    })?;

    let h = host.clone();
    list.register(&SAVE, Effect::Destructive, move |(): ()| {
        let files = h
            .save()
            .map_err(|e| failed("not every edit was saved", &e))?;
        Ok(Changed {
            version: h.version(),
            files,
        })
    })?;

    let h = host.clone();
    list.register(&REVERT, Effect::Destructive, move |(): ()| {
        let files = h
            .revert()
            .map_err(|e| failed("the theme was not reverted", &e))?;
        Ok(Changed {
            version: h.version(),
            files,
        })
    })?;

    let h = host.clone();
    list.register(&CAPTURE, Effect::ReadOnly, move |a: CaptureArgs| {
        let set: Vec<(&str, Value)> = a
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), value(v)))
            .collect();
        let fields = h.fields(&set);
        let size = match (a.width, a.height) {
            (Some(w), Some(h)) => Some((w, h)),
            (None, None) => None,
            _ => return Err("give both `width` and `height`, or neither".to_owned()),
        };
        let shot = h
            .capture(&a.role, size, a.scale.unwrap_or(1.0), &fields)
            .map_err(|e| e.to_string())?;
        let png = shot.pixels.png()?;
        Ok(Captured {
            width: shot.pixels.width,
            height: shot.pixels.height,
            png_base64: base64::engine::general_purpose::STANDARD.encode(png),
            blake3: blake3::hash(&shot.pixels.rgba).to_hex().to_string(),
            slots: shot.slots,
        })
    })?;
    Ok(())
}

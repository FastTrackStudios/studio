//! Theme packs for the views, chosen while Studio runs (daw-theme, daw#52).
//!
//! The arrangement, the mixer and the MIDI editor all draw from the one
//! theme loaded in this process (`daw_view_kit::skin`), so switching it
//! restyles every view at once; its files are watched, so editing a pack
//! restyles them too. Four actions:
//!
//! - `THEME_LIST`: the installed packs, their themes, the one shown.
//! - `THEME_SET {pack, theme}`: show `theme` of `pack`. `pack` is an
//!   installed pack's name, a first-party one (`fts`, `reaper`), a GitHub
//!   link (`owner/repo[@rev]`) or a folder; a link is installed first.
//!   `pack: "none"` goes back to the built-in look.
//! - `THEME_INSTALL {link}`: install a pack without showing it.
//! - `THEME_CHECK {pack?, theme?}`: hold a theme (the shown one when left
//!   out) to the spec, and say which of each view's pages it draws.
//!
//! The installed packs and the choice are kept in
//! `<config>/fts/studio-themes.json`; a linked pack is fetched into
//! `<cache>/fts/theme-packs/<owner>/<repo>/<commit>`, pinned there.

use std::path::PathBuf;
use std::sync::Arc;

use architect::action::{ActionError, ActionFuture, ActionList, ActionMeta, Effect};
use daw_theme::library::{Choice, Library};
use daw_theme::{Pack, Source};
use facet::Facet;
use serde_json::{Value, json};

/// The packs Studio knows by name before any is installed.
const FIRST_PARTY: [(&str, &str); 2] = [
    ("fts", "FastTrackStudios/studio-fts-theme"),
    ("reaper", "FastTrackStudios/studio-reaper-theme"),
];

fn library_file() -> Option<PathBuf> {
    crate::storage::config_dir().map(|d| d.join("fts").join("studio-themes.json"))
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("fts")
        .join("theme-packs")
}

fn load_library() -> Result<(Library, PathBuf), String> {
    let file = library_file().ok_or("no config folder")?;
    Library::load(&file)
        .map(|l| (l, file))
        .map_err(|e| e.to_string())
}

/// Show the theme the library says was chosen last, unless `FTS_THEME`
/// names one for this run. Called once, as the app's actions are made.
pub fn restore() {
    if std::env::var_os("FTS_THEME").is_some() {
        return;
    }
    let Ok((lib, _)) = load_library() else {
        return;
    };
    let Some(choice) = lib.chosen.clone() else {
        return;
    };
    if let Err(why) = show(&lib, &choice.pack, choice.theme.as_deref()) {
        tracing::warn!(theme.pack = %choice.pack, theme.error = %why, "the chosen theme did not load");
    }
}

/// Show `theme` of the installed pack `name`.
fn show(lib: &Library, name: &str, theme: Option<&str>) -> Result<Value, String> {
    let pack = lib
        .pack(name)
        .ok_or_else(|| format!("no pack `{name}` installed"))?;
    let def = pack.theme(theme).map_err(|e| e.to_string())?;
    daw_view_kit::skin::load_pack(&pack, def)?;
    Ok(json!({ "pack": pack.name, "theme": def.name, "mode": def.mode }))
}

/// `pack` as an installed pack's name, installing it first when it is a
/// link, a folder or a first-party name not installed yet.
fn installed(lib: &mut Library, pack: &str) -> Result<String, String> {
    if lib.packs.iter().any(|e| e.name == pack) {
        return Ok(pack.to_owned());
    }
    let link = FIRST_PARTY
        .iter()
        .find(|(n, _)| *n == pack)
        .map_or(pack, |(_, l)| *l);
    install(lib, link)
}

fn install(lib: &mut Library, link: &str) -> Result<String, String> {
    let source = Source::parse(link).map_err(|e| e.to_string())?;
    let got = daw_theme::install(&source, &cache_dir()).map_err(|e| e.to_string())?;
    lib.add(link, &got);
    Ok(got.pack.name)
}

fn describe(pack: &Pack) -> Value {
    json!({
        "name": pack.name,
        "title": pack.title,
        "path": pack.root,
        "themes": pack.themes.iter().map(|t| json!({
            "name": t.name, "title": t.title, "mode": t.mode,
        })).collect::<Vec<_>>(),
    })
}

fn list_packs() -> Result<Value, String> {
    let (lib, _) = load_library()?;
    let packs: Vec<Value> = lib
        .packs
        .iter()
        .map(|e| {
            let mut v = lib
                .pack(&e.name)
                .map_or_else(|| json!({ "name": e.name }), |p| describe(&p));
            v["link"] = json!(e.link);
            v["commit"] = json!(e.commit);
            v
        })
        .collect();
    let shown = daw_view_kit::skin::chosen()
        .map(|c| json!({ "pack": c.pack, "theme": c.theme, "mode": c.mode, "layers": c.layers }));
    Ok(json!({
        "spec": daw_theme::spec::VERSION,
        "shown": shown,
        "packs": packs,
        "first_party": FIRST_PARTY.iter().map(|(n, l)| json!({ "name": n, "link": l })).collect::<Vec<_>>(),
    }))
}

#[derive(Facet)]
struct SetArgs {
    pack: String,
    #[facet(default)]
    theme: Option<String>,
}

fn set(a: SetArgs) -> Result<Value, String> {
    if a.pack.is_empty() || a.pack == "none" {
        daw_view_kit::skin::clear();
        let (mut lib, file) = load_library()?;
        lib.chosen = None;
        lib.save(&file).map_err(|e| e.to_string())?;
        return Ok(json!({ "pack": null }));
    }
    let (mut lib, file) = load_library()?;
    let name = installed(&mut lib, &a.pack)?;
    let shown = show(&lib, &name, a.theme.as_deref())?;
    lib.chosen = Some(Choice {
        pack: name,
        theme: shown["theme"].as_str().map(str::to_owned),
    });
    lib.save(&file).map_err(|e| e.to_string())?;
    Ok(shown)
}

#[derive(Facet)]
struct InstallArgs {
    link: String,
}

fn install_action(a: InstallArgs) -> Result<Value, String> {
    let (mut lib, file) = load_library()?;
    let name = install(&mut lib, &a.link)?;
    lib.save(&file).map_err(|e| e.to_string())?;
    let pack = lib.pack(&name).ok_or("installed, and then not found")?;
    let commit = lib
        .packs
        .iter()
        .find(|e| e.name == name)
        .and_then(|e| e.commit.clone());
    let mut v = describe(&pack);
    v["commit"] = json!(commit);
    Ok(v)
}

#[derive(Facet)]
struct CheckArgs {
    #[facet(default)]
    pack: Option<String>,
    #[facet(default)]
    theme: Option<String>,
}

fn check(a: CheckArgs) -> Result<Value, String> {
    let (mut lib, file) = load_library()?;
    let shown = daw_view_kit::skin::chosen();
    let (name, theme) = match (a.pack, shown) {
        (Some(p), _) => (installed(&mut lib, &p)?, a.theme),
        (None, Some(c)) if c.pack.is_some() => (c.pack.unwrap_or_default(), a.theme.or(c.theme)),
        _ => return Err("no theme is shown; name a `pack`".into()),
    };
    lib.save(&file).map_err(|e| e.to_string())?;
    let pack = lib.pack(&name).ok_or_else(|| format!("no pack `{name}`"))?;
    let def = pack.theme(theme.as_deref()).map_err(|e| e.to_string())?;
    let report = daw_theme::check(def);
    let views = [
        arrangement_view::theme::USES,
        mixer_view::theme::USES,
        daw_view_kit::skin::uses::EDITOR,
    ];
    Ok(json!({
        "pack": pack.name,
        "theme": def.name,
        "passes": report.is_clean(),
        "errors": report.errors,
        "unknown_tokens": report.unknown_tokens,
        "unknown_pages": report.unknown_parts,
        "pages": report.pages,
        "missing_tokens": report.missing_tokens.iter().map(|(n, to)| json!({ "token": n, "falls_back_to": to })).collect::<Vec<_>>(),
        "missing_components": report.missing_components,
        "views": report.coverage(&views).iter().map(|c| json!({
            "view": c.view, "themed": c.themed, "native": c.native,
        })).collect::<Vec<_>>(),
    }))
}

macro_rules! meta {
    ($name:ident, $id:literal, $display:literal, $desc:literal) => {
        static $name: ActionMeta = ActionMeta {
            id: $id,
            trait_name: "",
            method_name: "",
            display_name: $display,
            description: $desc,
            category: "Theme",
            group: "Studio",
            shortcut: "",
            toggleable: false,
            undo: false,
        };
    };
}

meta!(
    LIST,
    "THEME_LIST",
    "Theme packs",
    "The installed theme packs, each pack's themes, the one the views show (`shown`), and the first-party packs Studio installs by name."
);
meta!(
    SET,
    "THEME_SET",
    "Show a theme",
    "Shows `theme` of `pack` in every view (the arrangement, the mixer, the MIDI editor), at once and from then on. `pack` is an installed pack's name, `fts` or `reaper`, a GitHub link (`owner/repo`, `owner/repo@rev`, a github.com URL) or a folder; a link is installed first, pinned to its commit. `theme` may be left out when the pack has one. `pack: \"none\"` goes back to the built-in look."
);
meta!(
    INSTALL,
    "THEME_INSTALL",
    "Install a theme pack",
    "Installs the pack at `link` (`owner/repo[@rev]`, a github.com URL or a folder) without showing it; answers its name, themes and commit."
);
meta!(
    CHECK,
    "THEME_CHECK",
    "Check a theme",
    "Holds a theme to daw-theme's spec: `pack` and `theme`, or the one shown when left out. Answers whether it passes, any unknown tokens or pages, every token it leaves out and what that falls back to, and for each view the pages the theme draws and those the view draws itself."
);

fn failed(id: &str, message: String) -> ActionError {
    ActionError::Failed {
        id: id.to_owned(),
        message,
    }
}

/// Put one action on `list`: its arguments read as `A` and answered by
/// `run`, now (none of them waits on the window).
fn register<A: Facet<'static>>(
    list: &ActionList,
    meta: &'static ActionMeta,
    effect: Effect,
    takes: bool,
    run: fn(A) -> Result<Value, String>,
) {
    let id = meta.id;
    let handler = Arc::new(move |args: &[u8]| -> Result<ActionFuture, ActionError> {
        let args = if args.is_empty() {
            b"{}".as_slice()
        } else {
            args
        };
        let a: A = facet_json::from_slice(args).map_err(|e| ActionError::BadArgs {
            id: id.to_owned(),
            message: e.to_string(),
            expected: None,
        })?;
        let mut span = crate::trace::Span::root("studio.theme");
        span.set("theme.action", id.to_owned());
        let answer = run(a).inspect_err(|e| span.fail(e.clone()));
        let bytes = answer
            .map_err(|e| failed(id, e))
            .and_then(|v| serde_json::to_vec(&v).map_err(|e| failed(id, e.to_string())));
        Ok(Box::pin(async move { bytes }) as ActionFuture)
    });
    let params = takes.then_some(A::SHAPE);
    if let Err(e) = list.register_dynamic_async(meta, effect, params, None, handler) {
        tracing::warn!(action.id = id, error = %e, "an action could not be registered");
    }
}

#[derive(Facet)]
struct NoArgs {}

/// The theme actions, on the app's list.
pub fn install_actions(list: &ActionList) {
    register::<NoArgs>(list, &LIST, Effect::ReadOnly, false, |_| list_packs());
    register::<SetArgs>(list, &SET, Effect::Mutating, true, set);
    register::<InstallArgs>(list, &INSTALL, Effect::Mutating, true, install_action);
    register::<CheckArgs>(list, &CHECK, Effect::ReadOnly, true, check);
}

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
//! - `THEME_GALLERY {pack, theme?, shots?, out}`: draw a pack's showcase
//!   shots from the golden session into `out` (`daw-view-tools gallery`,
//!   in a process of its own: the gallery sets the views' theme, folds and
//!   lanes, which would restyle this window).
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
const FIRST_PARTY: &[(&str, &str)] = daw_theme::source::FIRST_PARTY;

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
    GALLERY,
    "THEME_GALLERY",
    "Draw a theme pack's gallery",
    "Draws `pack`'s showcase shots (daw-theme's gallery) from the golden session into the folder `out`: `<theme>/<shot>.png` for each theme (or only `theme`) and shot (or only `shots`), and `README.md` when every shot is drawn, ready to commit as a pack repository's `gallery/`. `pack` as THEME_SET takes it, or `none` for the built-in look. Runs `daw-view-tools gallery` in a process of its own, so the window's look is untouched; answers what it wrote and how long each theme took."
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
        let args = decoded(takes, args);
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
struct GalleryArgs {
    pack: String,
    #[facet(default)]
    theme: Option<String>,
    #[facet(default)]
    shots: Vec<String>,
    out: String,
}

/// The gallery tool: `FTS_VIEW_TOOLS`, else beside Studio's own binary,
/// else on the `PATH`.
fn view_tools() -> PathBuf {
    std::env::var_os("FTS_VIEW_TOOLS")
        .map(PathBuf::from)
        .or_else(|| {
            let beside = std::env::current_exe()
                .ok()?
                .with_file_name("daw-view-tools");
            beside.is_file().then_some(beside)
        })
        .unwrap_or_else(|| PathBuf::from("daw-view-tools"))
}

/// The `daw-view-tools gallery` arguments for `a`.
fn gallery_command(a: &GalleryArgs) -> Vec<String> {
    let mut args = vec!["gallery".to_owned(), "--pack".to_owned(), a.pack.clone()];
    if let Some(theme) = &a.theme {
        args.extend(["--theme".to_owned(), theme.clone()]);
    }
    for shot in &a.shots {
        args.extend(["--shot".to_owned(), shot.clone()]);
    }
    args.extend(["--out".to_owned(), a.out.clone()]);
    args
}

fn gallery(a: GalleryArgs) -> Result<Value, String> {
    let tool = view_tools();
    let out = std::process::Command::new(&tool)
        .args(gallery_command(&a))
        .output()
        .map_err(|e| format!("{}: {e}", tool.display()))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

#[derive(Facet)]
struct NoArgs {}

/// The bytes an action's arguments are read from. The Action List hands an
/// action that takes nothing `null` (and a caller may send `null` or nothing
/// to one whose arguments all have defaults): both read as `{}`, never as a
/// struct made of a scalar.
fn decoded(takes: bool, args: &[u8]) -> &[u8] {
    let blank = args.iter().all(u8::is_ascii_whitespace) || args.trim_ascii() == b"null";
    if !takes || blank { b"{}" } else { args }
}

/// The theme actions, on the app's list.
pub fn install_actions(list: &ActionList) {
    register::<NoArgs>(list, &LIST, Effect::ReadOnly, false, |_| list_packs());
    register::<SetArgs>(list, &SET, Effect::Mutating, true, set);
    register::<InstallArgs>(list, &INSTALL, Effect::Mutating, true, install_action);
    register::<CheckArgs>(list, &CHECK, Effect::ReadOnly, true, check);
    register::<GalleryArgs>(list, &GALLERY, Effect::Mutating, true, gallery);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future as _;

    /// Run `id` the way MCP and the control socket do: through the Action
    /// List, which normalizes the arguments first. The theme actions answer
    /// at once, so one poll finishes the future.
    fn run(list: &ActionList, id: &str, args: &[u8]) -> Result<Value, ActionError> {
        let mut work = std::pin::pin!(list.run_json_async(id, args));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        match work.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(out) => {
                out.map(|bytes| serde_json::from_slice(&bytes).unwrap_or(Value::Null))
            }
            std::task::Poll::Pending => panic!("{id} did not answer at once"),
        }
    }

    /// The decoder's own complaint: arguments read as the wrong shape, the
    /// bug where `THEME_LIST` could not be called at all.
    fn misread(e: &ActionError) -> bool {
        let text = e.to_string();
        text.contains("got scalar") || text.contains("expected struct start")
    }

    fn list() -> ActionList {
        let list = ActionList::new();
        install_actions(&list);
        list
    }

    #[test]
    fn theme_list_runs_with_no_args_empty_args_or_null() {
        let list = list();
        for args in [&b""[..], b"{}", b"null", b" "] {
            let got = run(&list, "THEME_LIST", args);
            match got {
                Ok(v) => assert_eq!(v["spec"], json!(daw_theme::spec::VERSION)),
                Err(e) => panic!("THEME_LIST with {:?}: {e}", String::from_utf8_lossy(args)),
            }
        }
    }

    #[test]
    fn every_theme_action_reads_its_args_whatever_the_caller_sends() {
        let list = list();
        for id in [
            "THEME_LIST",
            "THEME_SET",
            "THEME_INSTALL",
            "THEME_CHECK",
            "THEME_GALLERY",
        ] {
            for args in [&b""[..], b"{}", b"null"] {
                if let Err(e) = run(&list, id, args) {
                    assert!(
                        !misread(&e),
                        "{id} with {:?} misread its arguments: {e}",
                        String::from_utf8_lossy(args)
                    );
                }
            }
        }
    }

    #[test]
    fn an_action_with_defaults_runs_on_no_args() {
        // Nothing shown and no pack named: the action ran and said so,
        // rather than refusing its arguments.
        let list = list();
        for args in [&b""[..], b"{}", b"null"] {
            match run(&list, "THEME_CHECK", args) {
                Err(ActionError::BadArgs { message, .. }) => panic!(
                    "THEME_CHECK refused {:?}: {message}",
                    String::from_utf8_lossy(args)
                ),
                Ok(_) | Err(_) => {}
            }
        }
    }

    #[test]
    fn a_required_arg_left_out_is_named_not_misread() {
        let list = list();
        for (id, field) in [
            ("THEME_SET", "pack"),
            ("THEME_INSTALL", "link"),
            ("THEME_GALLERY", "pack"),
        ] {
            match run(&list, id, b"{}") {
                Err(e @ ActionError::BadArgs { .. }) => {
                    assert!(!misread(&e), "{id}: {e}");
                    assert!(e.to_string().contains(field), "{id} names `{field}`: {e}");
                }
                other => panic!("{id} with no `{field}` answered {other:?}"),
            }
        }
    }

    #[test]
    fn decoded_args() {
        assert_eq!(decoded(false, b"null"), b"{}");
        assert_eq!(decoded(false, br#"{"x":1}"#), b"{}");
        assert_eq!(decoded(true, b""), b"{}");
        assert_eq!(decoded(true, b" null "), b"{}");
        assert_eq!(decoded(true, br#"{"pack":"fts"}"#), br#"{"pack":"fts"}"#);
    }

    #[test]
    fn a_gallery_call_is_the_command_line_the_tool_takes() {
        let a = GalleryArgs {
            pack: "fts".into(),
            theme: Some("console".into()),
            shots: vec!["mixer.overview".into(), "editor.drums".into()],
            out: "gallery".into(),
        };
        assert_eq!(
            gallery_command(&a),
            [
                "gallery",
                "--pack",
                "fts",
                "--theme",
                "console",
                "--shot",
                "mixer.overview",
                "--shot",
                "editor.drums",
                "--out",
                "gallery",
            ]
        );
    }
}

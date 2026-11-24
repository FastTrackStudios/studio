//! The engine's edit algorithms: Architect extensions of the engine.
//!
//! The DAW owns the edit surface; *how* an edit is decided is an algorithm
//! (`daw_edit`'s contract: where a performance's hits play once moved,
//! …). The engine has none of its own: an extension provides the service
//! (`[provides] "daw.edit" = "0.1.0"`), and the engine asks the chosen one,
//! as Architect services are asked (its action for a method,
//! `RHYTHM_PLACE` for `PLACE`). First-party (rhythm, pitch) and
//! third-party algorithms are loaded the same way.
//!
//! For development, an algorithm runs from its checkout: `ENGINE_EXT_DEV
//! {"folder": "<crate>"}` builds its process program (`cargo build`),
//! swaps it in once a build is good, and builds and swaps it again on
//! every save to any local crate it builds from, while the engine, its
//! song and its audio go on. A build that fails keeps the last good copy.
//! The folders are kept (`<config>/fts/engine-extensions/dev/overrides.json`)
//! and come back at every start until `ENGINE_EXT_DEV_END`.
//! `ENGINE_EXT_LIST` says what is loaded, what provides what and which
//! provider is chosen; `ENGINE_EXT_USE` chooses one (Studio's choice).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use architect::action::{ActionCaller, ActionError, ActionList, ActionMeta, Effect, RawJson};
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Event, Host, SharedSections};
use architect_extensions::install::Installs;
use facet::Facet;
use serde_json::{Value as Json, json};

static EXTENSIONS: OnceLock<Option<Extensions>> = OnceLock::new();

/// How long an algorithm may take to answer before the built-in answers.
const ANSWER_WITHIN: Duration = Duration::from_secs(20);

/// The engine's algorithm extensions, started the first time they are
/// asked for (`None` where they cannot run: no config folder).
pub fn extensions() -> Option<&'static Extensions> {
    EXTENSIONS.get_or_init(start).as_ref()
}

/// Starts them now (the engine's start), with their actions on its list.
pub fn start_now() {
    let _ = extensions();
}

fn root() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("engine-extensions"))
}

fn start() -> Option<Extensions> {
    let root = root()?;
    let list = crate::actions::list().clone();
    let host = Host::new(list.clone(), SharedSections::new(architect_settings::Registry::new()));
    let host = match architect_extensions::process::ProcessRunner::new() {
        Ok(runner) => host.with_runner(runner),
        Err(error) => {
            tracing::warn!(error = %error, "algorithms: no process extensions");
            host
        }
    };
    let mut ext =
        Extensions::new(host, Installs::new(root.join("installed"))).with_dev_root(root.join("dev"));
    if let Some(target) = std::env::var_os("FTS_ENGINE_EXT_TARGET") {
        ext = ext.with_build_target(PathBuf::from(target));
    }
    tell(&ext.sync());
    let overrides = Overrides::at(&root.join("dev"));
    for dir in overrides.folders().into_iter().chain(
        std::env::var_os("FTS_ENGINE_EXTENSIONS")
            .map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
            .unwrap_or_default(),
    ) {
        if let Err(error) = develop(&ext, &dir) {
            tracing::warn!(ext.dev = %dir.display(), error = %error, "algorithms: an extension did not start");
        }
    }
    if let Err(error) = register(&list, &ext, overrides) {
        tracing::warn!(error = %error, "algorithms: their actions did not register");
    }
    watch(ext.clone());
    Some(ext)
}

/// Runs the extension in `dir` as a development copy: built from source
/// when it is a crate, as it is when a built package. Its id.
fn develop(ext: &Extensions, dir: &Path) -> Result<String, String> {
    let id = architect_extensions::package::Package::read(dir)
        .map_err(|e| e.to_string())?
        .manifest
        .id;
    let events = if dir.join("Cargo.toml").is_file() {
        ext.dev_crate(dir)?
    } else {
        ext.dev(dir.to_path_buf())
    };
    tracing::info!(ext.id = %id, ext.dev = %dir.display(), "algorithms: an extension runs from its checkout");
    tell(&events);
    Ok(id)
}

/// Every second: rebuilds what changed and swaps in what built.
fn watch(ext: Extensions) {
    let spawned = std::thread::Builder::new()
        .name("engine-ext-dev".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                tell(&ext.reload_changed());
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(error = %error, "algorithms: development copies are not watched");
    }
}

fn tell(events: &[Event]) {
    for event in events {
        match event {
            Event::Failed { id, error } => {
                tracing::warn!(ext.id = %id, error = %error, "algorithms: an extension did not load");
            }
            Event::Loaded(change) => tracing::info!(ext.id = %change.id, "algorithms: extension loaded"),
            Event::Unloaded { id } => tracing::info!(ext.id = %id, "algorithms: extension unloaded"),
        }
    }
}

/// [`daw_edit::METHOD_PLACE`]: where a performance's hits play, from the
/// chosen `daw.edit` provider. The engine has no placement of its own.
///
/// # Errors
///
/// No extension provides `daw.edit` (run rhythm: `ENGINE_EXT_DEV` on its
/// `crates/rhythm-ext`, or install it), or the provider failed or did not
/// answer in time.
pub fn place(request: &daw_edit::PlaceRequest) -> Result<daw_edit::PlaceAnswer, String> {
    ask(daw_edit::METHOD_PLACE, request)?.ok_or_else(|| {
        "no edit algorithm is loaded: placing hits needs an extension providing daw.edit (rhythm: ENGINE_EXT_DEV {\"folder\": \"<rhythm>/crates/rhythm-ext\"})".to_owned()
    })
}

/// The chosen `daw.edit` provider's answer to `method` with `request`:
/// `None` when no extension provides the service.
fn ask<Q: serde::Serialize, A: serde::de::DeserializeOwned>(
    method: &str,
    request: &Q,
) -> Result<Option<A>, String> {
    let Some(ext) = extensions() else { return Ok(None) };
    let host = ext.host();
    let wanted = semver::VersionReq::parse("^0.1").map_err(|e| e.to_string())?;
    let Some((provider, _)) = host.provider(daw_edit::SERVICE, &wanted) else {
        return Ok(None);
    };
    let loaded = host.get(&provider).ok_or_else(|| format!("{provider} is not loaded"))?;
    let action =
        architect_extensions::interop::method_action(&loaded.manifest.action_prefix(), method);
    let args = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let runtime = crate::open::runtime().ok_or("no engine runtime")?;
    let (tx, rx) = std::sync::mpsc::channel();
    let call = action.clone();
    runtime.spawn(async move {
        let answer = crate::actions::list().run_json_async(&call, &args).await;
        let _ = tx.send(answer);
    });
    let out = rx
        .recv_timeout(ANSWER_WITHIN)
        .map_err(|_| format!("{action} did not answer within {} s", ANSWER_WITHIN.as_secs()))?
        .map_err(|e| format!("{action}: {e}"))?;
    let answer = serde_json::from_slice(&out).map_err(|e| format!("{action}'s answer: {e}"))?;
    tracing::info!(algo.provider = %provider, algo.action = %action, "algorithms: answered by an extension");
    Ok(Some(answer))
}

/// The development folders kept across starts: `overrides.json`, a list.
#[derive(Clone)]
struct Overrides {
    file: PathBuf,
}

impl Overrides {
    fn at(dev: &Path) -> Self {
        Self {
            file: dev.join("overrides.json"),
        }
    }

    fn folders(&self) -> Vec<PathBuf> {
        std::fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect()
    }

    fn set(&self, dir: &Path, on: bool) -> Result<(), String> {
        let mut all = self.folders();
        all.retain(|d| d != dir);
        if on {
            all.push(dir.to_path_buf());
        }
        let text: Vec<String> = all.iter().map(|d| d.display().to_string()).collect();
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(&text).map_err(|e| e.to_string())?;
        std::fs::write(&self.file, json).map_err(|e| e.to_string())
    }
}

const fn meta(id: &'static str, name: &'static str, description: &'static str) -> ActionMeta {
    ActionMeta {
        id,
        trait_name: "",
        method_name: id,
        display_name: name,
        description,
        category: "Algorithms",
        group: "Engine",
        shortcut: "",
        toggleable: false,
        undo: false,
    }
}

static DEV: ActionMeta = meta(
    "ENGINE_EXT_DEV",
    "Run an algorithm from its checkout",
    "Runs the engine extension in `folder` (an algorithm's crate, e.g. rhythm's crates/rhythm-ext, or a built package): built as a process with cargo, loaded once a build is good, rebuilt and swapped in on every save to any local crate it builds from while the engine and its song go on. A failed build keeps the last good one. Kept across starts until ENGINE_EXT_DEV_END.",
);
static DEV_END: ActionMeta = meta(
    "ENGINE_EXT_DEV_END",
    "Stop running an algorithm from its checkout",
    "Ends the development copy in `folder`: unloaded now and at the next start (another provider of its service answers, else edits that need one are refused).",
);
static LIST: ActionMeta = meta(
    "ENGINE_EXT_LIST",
    "The engine's algorithms",
    "What the engine has loaded: each extension (id, version, where it runs from, its build), the services they provide with their providers, and which one is chosen for each.",
);
static USE: ActionMeta = meta(
    "ENGINE_EXT_USE",
    "Choose an algorithm",
    "Chooses extension `provider` to answer `service` (`daw.edit` unless given); no `provider` goes back to the engine's own choice (the newest loaded).",
);

#[derive(Debug, Facet)]
struct Folder {
    folder: String,
}

#[derive(Debug, Facet)]
struct UseArgs {
    #[facet(default)]
    service: Option<String>,
    #[facet(default)]
    provider: Option<String>,
}

fn register(list: &ActionList, ext: &Extensions, overrides: Overrides) -> Result<(), ActionError> {
    let (me, kept) = (ext.clone(), overrides.clone());
    list.register_with(&DEV, Effect::Mutating, move |_: &ActionCaller, a: Folder| {
        let dir = PathBuf::from(&a.folder);
        let id = develop(&me, &dir)?;
        kept.set(&dir, true)?;
        Ok::<_, String>(RawJson::from(
            json!({ "folder": a.folder, "id": id, "build": me.build_of(&id) }).to_string(),
        ))
    })?;
    let (me, kept) = (ext.clone(), overrides);
    list.register_with(&DEV_END, Effect::Mutating, move |_: &ActionCaller, a: Folder| {
        let dir = PathBuf::from(&a.folder);
        tell(&me.end_dev(&dir));
        kept.set(&dir, false)?;
        Ok::<_, String>(RawJson::from(json!({ "folder": a.folder }).to_string()))
    })?;
    let me = ext.clone();
    list.register_with(&LIST, Effect::ReadOnly, move |_: &ActionCaller, (): ()| {
        let host = me.host();
        let loaded: Vec<Json> = host
            .loaded()
            .into_iter()
            .map(|l| {
                json!({
                    "id": l.manifest.id,
                    "version": l.manifest.version.to_string(),
                    "dir": l.dir.as_ref().map(|d| d.display().to_string()),
                    "provides": l.manifest.provides.keys().collect::<Vec<_>>(),
                    "build": me.build_of(&l.manifest.id),
                })
            })
            .collect();
        let wanted = semver::VersionReq::parse("^0.1").ok();
        let chosen = wanted
            .as_ref()
            .and_then(|w| host.provider(daw_edit::SERVICE, w))
            .map(|(id, version)| json!({ "id": id, "version": version.to_string() }));
        Ok::<_, String>(RawJson::from(
            json!({
                "extensions": loaded,
                "services": { daw_edit::SERVICE: { "contract": daw_edit::VERSION, "in_use": chosen } },
            })
            .to_string(),
        ))
    })?;
    let me = ext.clone();
    list.register_with(&USE, Effect::Mutating, move |_: &ActionCaller, a: UseArgs| {
        let service = a.service.unwrap_or_else(|| daw_edit::SERVICE.to_owned());
        me.host().use_provider(&service, a.provider.as_deref());
        Ok::<_, String>(RawJson::from(
            json!({ "service": service, "provider": a.provider }).to_string(),
        ))
    })?;
    Ok(())
}

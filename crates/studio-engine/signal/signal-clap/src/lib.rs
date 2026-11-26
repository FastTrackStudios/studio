//! FTS Signal as a CLAP, without its editor: `signal-core` wrapped as a
//! nice-plug plugin, with its `signal/1` extension and its state.
//!
//! Two builds share this code:
//!
//! - **The bundle** (`plugins/signal/signal`, `signal-plugin`):
//!   [`FtsSignal`] with its Dioxus editor, exported as a `.clap`/`.vst3`
//!   for any DAW.
//! - **Studio's engine**: [`FtsSignal`] with no editor, hosted in-process
//!   through [`entry::ENTRY`] by the same CLAP host as any plugin, so a
//!   song's Signal needs no plugin installed and is always this build.
//!
//! See [`plugin`] for the wrapper and [`sampler`] for the desktop sampler
//! block.

pub mod block_set;
pub mod config;
pub mod drum;
pub mod ext;
pub mod offline;
pub mod plugin;
pub mod sampler;
pub mod sleep;
pub mod slots;
pub mod strip;

pub use plugin::{FtsSignal, NoEditor, Shared, SignalEditor};

/// The names Signal goes by: the development build's and the release's.
const NAMES: [&str; 2] = ["Signal Dev", "FTS Signal"];

/// Whether an FX named `name` is Signal: `Signal Dev`, `FTS Signal`, a
/// `CLAP: Signal Dev (FastTrackStudio)` name, or a `.clap` bundle's path of
/// either from any machine (`/home/cody/.clap/Signal Dev.clap`).
#[must_use]
pub fn is_signal(name: &str) -> bool {
    let name = name.trim();
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let name = name
        .strip_suffix(".clap")
        .or_else(|| name.strip_suffix(".CLAP"))
        .unwrap_or(name);
    let name = name.strip_prefix("CLAP:").map_or(name, str::trim_start);
    // A host's vendor in brackets: `Signal Dev (FastTrackStudio)`.
    let name = match name.rfind(" (") {
        Some(at) if name.ends_with(')') => &name[..at],
        _ => name,
    };
    NAMES.iter().any(|n| n.eq_ignore_ascii_case(name.trim()))
}

#[cfg(test)]
mod tests {
    use super::is_signal;

    #[test]
    fn signal_is_known_by_each_of_its_names() {
        for name in [
            "Signal Dev",
            "FTS Signal",
            "signal dev",
            "Signal Dev.clap",
            "/home/cody/.clap/Signal Dev.clap",
            "/Users/rat/Library/Audio/Plug-Ins/CLAP/FTS Signal.clap",
            "CLAP: Signal Dev (FastTrackStudio)",
            "CLAP: FTS Signal",
        ] {
            assert!(is_signal(name), "{name}");
        }
        for name in [
            "FTS EQ",
            "Signal Sampler",
            "/home/cody/.clap/Signal Sampler.clap",
            "FTS Signal Rig",
            "Signal Dev.vst3",
            "",
        ] {
            assert!(!is_signal(name), "{name}");
        }
    }
}

/// Signal's CLAP entry, for a host in this process: what a `.clap` bundle's
/// `clap_entry` symbol is, with no symbol exported (the engine binary
/// hosts other bundles, each with its own). The plugin has no editor.
pub mod entry {
    use std::ffi::{CStr, c_char, c_void};
    use std::sync::{Arc, OnceLock};

    use nice_plug::wrapper::clap::{
        CLAP_PLUGIN_FACTORY_ID, CLAP_VERSION, PluginDescriptor, Wrapper, clap_host, clap_plugin,
        clap_plugin_descriptor, clap_plugin_entry, clap_plugin_factory,
    };

    use crate::FtsSignal;

    /// The plugin this entry makes: Signal with no editor.
    type Hosted = FtsSignal;

    fn descriptor() -> &'static PluginDescriptor {
        static DESCRIPTOR: OnceLock<PluginDescriptor> = OnceLock::new();
        DESCRIPTOR.get_or_init(PluginDescriptor::for_plugin::<Hosted>)
    }

    unsafe extern "C" fn plugin_count(_factory: *const clap_plugin_factory) -> u32 {
        1
    }

    unsafe extern "C" fn plugin_descriptor(
        _factory: *const clap_plugin_factory,
        index: u32,
    ) -> *const clap_plugin_descriptor {
        if index == 0 {
            descriptor().clap_plugin_descriptor()
        } else {
            std::ptr::null()
        }
    }

    unsafe extern "C" fn create_plugin(
        _factory: *const clap_plugin_factory,
        host: *const clap_host,
        plugin_id: *const c_char,
    ) -> *const clap_plugin {
        // SAFETY: the host's id, a C string, as CLAP has it.
        if plugin_id.is_null() || unsafe { CStr::from_ptr(plugin_id) } != descriptor().clap_id() {
            return std::ptr::null();
        }
        // As nice-plug's own export: the Arc is taken back and dropped in
        // the wrapper's `destroy()`.
        // SAFETY: `host` is the host's, valid while the instance lives.
        unsafe {
            (*Arc::into_raw(Wrapper::<Hosted>::new(host)))
                .clap_plugin
                .as_ptr()
        }
    }

    static FACTORY: clap_plugin_factory = clap_plugin_factory {
        get_plugin_count: Some(plugin_count),
        get_plugin_descriptor: Some(plugin_descriptor),
        create_plugin: Some(create_plugin),
    };

    unsafe extern "C" fn init(_plugin_path: *const c_char) -> bool {
        true
    }

    unsafe extern "C" fn deinit() {}

    unsafe extern "C" fn get_factory(factory_id: *const c_char) -> *const c_void {
        // SAFETY: the host's id, a C string, as CLAP has it.
        if !factory_id.is_null() && unsafe { CStr::from_ptr(factory_id) } == CLAP_PLUGIN_FACTORY_ID
        {
            std::ptr::from_ref(&FACTORY).cast()
        } else {
            std::ptr::null()
        }
    }

    /// The entry: hand it to a CLAP host as a bundle's would be.
    pub static ENTRY: clap_plugin_entry = clap_plugin_entry {
        clap_version: CLAP_VERSION,
        init: Some(init),
        deinit: Some(deinit),
        get_factory: Some(get_factory),
    };
}

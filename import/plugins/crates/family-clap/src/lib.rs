//! A Signal block family as one CLAP: FTS EQ, FTS Reverb, FTS Delay, FTS
//! Comp, FTS Saturate, FTS Gate (signal#26).
//!
//! The same family Signal compiles in, so it loads in any DAW, inside
//! Signal as the block's CLAP form (a development override), and nulls
//! against the compiled-in block.
//!
//! - **The same parameters whatever the algorithm.** [`SLOTS`] control
//!   slots (`c001`…), each 0–1 normalized on the host's side, filled in
//!   order by the loaded algorithm's controls. A slot takes its control's
//!   name ([`FloatParam::with_dynamic_name`]) and prints its value
//!   (`+3.0 dB`); when the algorithm changes the plugin asks the host to
//!   rescan, so a host that honors it (Studio's) shows the new names, and
//!   automation lanes stay where they are.
//! - **The algorithm** is the plugin's state (`algorithm`, and the file of
//!   one that was downloaded, `algorithm_file`), and changes through
//!   Signal's `signal/1` extension: `SIGNAL_ALGORITHMS`, `SIGNAL_ALGORITHM
//!   {"algorithm": "ssl-e"}`, `SIGNAL_ALGORITHM_LOAD {"styx": "…"}`.
//! - **The values run as the compiled-in block runs them:** the slot's
//!   modulated normalized value, denormalized by the control's own spec;
//!   split at every parameter event (sample-accurate); a change of
//!   algorithm builds the new block on the main thread and swaps it in at a
//!   block boundary, through a lock-free mailbox.

use std::any::Any;
use std::collections::BTreeMap;
use std::ffi::{CStr, c_void};
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock, Weak};

use daw_clap_ext::plugin::{SignalPlugin, SignalPluginLookup, SignalVtable};
use daw_clap_ext::{ExtError, ModSourceInfo};
use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::*;
use nice_plug::wrapper::clap::{
    clap_extension_state, clap_plugin, clap_request_rescan, param_clap_id,
};
use plugin_processors::algorithms::{self, Algorithm, FamilyBlock};
use plugin_processors::{BlockProcessor, ModSourceSpec, ValueText};

/// Host-visible control slots, the same for every algorithm of every
/// family: enough for the FTS EQ engine's whole surface (170).
pub const SLOTS: usize = 192;

/// Which family a CLAP is, and its identity in hosts.
pub trait FamilyDef: Send + Sync + 'static {
    /// "FTS EQ".
    const NAME: &'static str;
    /// `eq`.
    const FAMILY: &'static str;
    const CLAP_ID: &'static str;
    const VST3_ID: [u8; 16];
    const DESCRIPTION: &'static str;
}

fn slot_id(i: usize) -> &'static str {
    static IDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    IDS.get_or_init(|| {
        (1..=SLOTS)
            .map(|n| &*Box::leak(format!("c{n:03}").into_boxed_str()))
            .collect()
    })
    .get(i)
    .copied()
    .unwrap_or("c000")
}

// ── The audio side's mailbox ────────────────────────────────────────────

/// A new block for the audio thread, and the old one back for the main
/// thread to drop: no lock, no free on the audio thread.
#[derive(Default)]
struct Mailbox {
    pending: AtomicPtr<FamilyBlock>,
    retired: AtomicPtr<FamilyBlock>,
}

impl Mailbox {
    /// Main thread.
    fn post(&self, block: FamilyBlock) {
        self.collect();
        let old = self
            .pending
            .swap(Box::into_raw(Box::new(block)), Ordering::AcqRel);
        if !old.is_null() {
            // Never installed: ours to drop.
            // SAFETY: only `post` and `take` touch `pending`, by swap.
            drop(unsafe { Box::from_raw(old) });
        }
    }

    /// Main thread: drop what the audio side handed back.
    fn collect(&self) {
        let old = self.retired.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !old.is_null() {
            // SAFETY: the audio side gave it up.
            drop(unsafe { Box::from_raw(old) });
        }
    }

    /// Audio thread: the new block, when there's room to hand the old back.
    fn take(&self) -> Option<Box<FamilyBlock>> {
        if !self.retired.load(Ordering::Acquire).is_null() {
            return None;
        }
        let p = self.pending.swap(std::ptr::null_mut(), Ordering::AcqRel);
        // SAFETY: posted by `post`, now ours.
        (!p.is_null()).then(|| unsafe { Box::from_raw(p) })
    }

    /// Audio thread.
    fn retire(&self, old: Box<FamilyBlock>) {
        let prev = self.retired.swap(Box::into_raw(old), Ordering::AcqRel);
        debug_assert!(prev.is_null(), "take() only runs with room to retire");
    }
}

impl Drop for Mailbox {
    fn drop(&mut self) {
        for p in [
            self.pending.load(Ordering::Acquire),
            self.retired.load(Ordering::Acquire),
        ] {
            if !p.is_null() {
                // SAFETY: the plugin is going; nothing else holds them.
                drop(unsafe { Box::from_raw(p) });
            }
        }
    }
}

// ── Shared state ────────────────────────────────────────────────────────

/// What the parameters' text, `signal/1` and the main thread work from.
/// Never the plugin itself, which is locked while it processes.
pub struct Shared {
    family: &'static str,
    current: RwLock<Arc<Algorithm>>,
    /// The file of a downloaded algorithm, kept with the session so it
    /// opens on a machine that doesn't have it.
    algorithm_file: Mutex<Option<String>>,
    /// Sample rate and max block of the current activation.
    config: Mutex<Option<(f32, usize)>>,
    mailbox: Mailbox,
    sources: &'static [ModSourceSpec],
    source_values: Vec<AtomicU32>,
    /// Each control's value as the block last ran it (f32 bits): what the
    /// views show, and what a test compares with the compiled-in block.
    values: Vec<AtomicU32>,
    revision: AtomicU64,
    /// The `clap_plugin` the host knows this instance by (seen on its first
    /// `signal/1` call), for rescans.
    plugin: AtomicPtr<clap_plugin>,
    rescan: unsafe fn(*const clap_plugin),
    params: OnceLock<Weak<FamilyParams>>,
}

impl Shared {
    fn current(&self) -> Arc<Algorithm> {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Load `algo`: build its block for the running config (main thread),
    /// and, unless the values were just restored with it, put every slot at
    /// the new controls' defaults. Then ask the host to rescan.
    fn switch(&self, algo: Arc<Algorithm>, reset_values: bool) {
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = algo.clone();
        // Not run yet under this algorithm.
        for v in &self.values {
            v.store(f32::NAN.to_bits(), Ordering::Release);
        }
        let config = *self.config.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((sr, max_block)) = config {
            match FamilyBlock::new(algo.clone(), sr, max_block) {
                Ok(block) => self.mailbox.post(block),
                Err(e) => {
                    nice_plug::nice_log!("{}: algorithm {} not built: {e}", self.family, algo.id)
                }
            }
        }
        if reset_values {
            if let Some(params) = self.params.get().and_then(Weak::upgrade) {
                for (i, slot) in params.slots.iter().enumerate() {
                    let n = algo.controls.get(i).map_or(0.0, |c| c.normalize(c.default));
                    // SAFETY: the slot lives as long as `params`, held here.
                    unsafe { slot.as_ptr()._internal_set_normalized_value(n) };
                }
            }
        }
        self.revision.fetch_add(1, Ordering::AcqRel);
        let plugin = self.plugin.load(Ordering::Acquire);
        if !plugin.is_null() {
            // SAFETY: the host's own pointer for this instance.
            unsafe { (self.rescan)(plugin) };
        }
    }

    fn describe(&self) -> serde_json::Value {
        let a = self.current();
        serde_json::json!({
            "family": self.family,
            "algorithm": a.id,
            "name": a.name,
            "engine": a.engine,
            "author": a.author,
            "look": a.look,
            "controls": a.controls.iter().enumerate().map(|(i, c)| {
                let value = self.values.get(i).map(|v| f32::from_bits(v.load(Ordering::Acquire))).filter(|v| v.is_finite());
                serde_json::json!({"id": c.id, "name": c.name, "slot": slot_id(i), "value": value})
            }).collect::<Vec<_>>(),
        })
    }
}

impl SignalPlugin for Shared {
    fn list_actions(&self) -> String {
        serde_json::json!([
            {"id": "SIGNAL_ALGORITHMS", "description": "The family's algorithms"},
            {"id": "SIGNAL_ALGORITHM", "description": "Load an algorithm: {\"algorithm\": id}"},
            {"id": "SIGNAL_ALGORITHM_LOAD", "description": "Load an algorithm file: {\"styx\": text}"},
        ])
        .to_string()
    }

    fn run_action(&self, id: &str, args_json: &str) -> Result<String, ExtError> {
        let args: serde_json::Value =
            serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
        match id {
            "SIGNAL_ALGORITHMS" => Ok(serde_json::Value::Array(
                algorithms::family(self.family)
                    .iter()
                    .map(|a| serde_json::json!({"id": a.id, "name": a.name, "engine": a.engine, "author": a.author, "controls": a.controls.len()}))
                    .collect(),
            )
            .to_string()),
            "SIGNAL_ALGORITHM" => {
                let want = args.get("algorithm").and_then(serde_json::Value::as_str).ok_or_else(|| ExtError::Failed("needs {\"algorithm\": id}".into()))?;
                let algo = algorithms::find(self.family, Some(want)).ok_or_else(|| ExtError::NotFound(format!("{} algorithm {want}", self.family)))?;
                *self.algorithm_file.lock().unwrap_or_else(PoisonError::into_inner) = None;
                self.switch(algo, true);
                Ok(self.describe().to_string())
            }
            "SIGNAL_ALGORITHM_LOAD" => {
                let text = args.get("styx").and_then(serde_json::Value::as_str).ok_or_else(|| ExtError::Failed("needs {\"styx\": text}".into()))?;
                let def = algorithms::parse_styx(text).map_err(|e| ExtError::Failed(e.to_string()))?;
                if def.family != self.family {
                    return Err(ExtError::Failed(format!("a {} algorithm, not {}", def.family, self.family)));
                }
                let algo = algorithms::register(&def).map_err(|e| ExtError::Failed(e.to_string()))?;
                *self.algorithm_file.lock().unwrap_or_else(PoisonError::into_inner) = Some(text.to_owned());
                self.switch(algo, true);
                Ok(self.describe().to_string())
            }
            other => Err(ExtError::NotFound(other.into())),
        }
    }

    fn chain(&self) -> String {
        self.describe().to_string()
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn param_stable_id(&self, param_id: u32) -> Option<String> {
        let slot = (0..SLOTS).find(|i| param_clap_id(slot_id(*i)) == param_id)?;
        self.current().controls.get(slot).map(|c| c.id.to_owned())
    }

    fn mod_sources(&self) -> Vec<ModSourceInfo> {
        self.sources
            .iter()
            .map(|s| ModSourceInfo {
                id: s.id.into(),
                name: s.name.into(),
                unit: String::new(),
            })
            .collect()
    }

    fn mod_source_value(&self, index: usize) -> f64 {
        self.source_values.get(index).map_or(0.0, |v| {
            f64::from(f32::from_bits(v.load(Ordering::Acquire)))
        })
    }

    fn set_session(&self, _session: Option<(&str, &str)>) -> bool {
        false
    }
}

// ── Parameters ──────────────────────────────────────────────────────────

/// The slots, and the algorithm as persisted state.
pub struct FamilyParams {
    slots: Vec<FloatParam>,
    shared: Arc<Shared>,
}

impl FamilyParams {
    fn new(shared: Arc<Shared>) -> Self {
        let first = shared.current();
        let slots = (0..SLOTS)
            .map(|i| {
                let default = first
                    .controls
                    .get(i)
                    .map_or(0.0, |c| c.normalize(c.default));
                let (s_name, s_text, s_parse) = (shared.clone(), shared.clone(), shared.clone());
                FloatParam::new(
                    format!("Control {}", i + 1),
                    default,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                )
                .with_dynamic_name(Arc::new(move || {
                    s_name
                        .current()
                        .controls
                        .get(i)
                        .map_or_else(|| format!("(unused {})", i + 1), |c| c.name.to_owned())
                }))
                .with_value_to_string(Arc::new(move |n| {
                    s_text
                        .current()
                        .controls
                        .get(i)
                        .map_or_else(String::new, |c| {
                            ValueText::of(c, c.denormalize(n)).as_str().to_owned()
                        })
                }))
                .with_string_to_value(Arc::new(move |text| {
                    let a = s_parse.current();
                    let c = a.controls.get(i)?;
                    c.parse(text).map(|v| c.normalize(v))
                }))
            })
            .collect();
        Self { slots, shared }
    }
}

// SAFETY: the pointers are to `self.slots`, never resized after
// construction, alive as long as nice-plug's `Arc<FamilyParams>`.
unsafe impl Params for FamilyParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.slots
            .iter()
            .enumerate()
            .map(|(i, p)| (slot_id(i).to_owned(), p.as_ptr(), String::new()))
            .collect()
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        out.insert(
            "algorithm".into(),
            serde_json::to_string(self.shared.current().id).unwrap_or_default(),
        );
        if let Some(file) = self
            .shared
            .algorithm_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            out.insert(
                "algorithm_file".into(),
                serde_json::to_string(file).unwrap_or_default(),
            );
        }
        out
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        let field = |k: &str| {
            serialized
                .get(k)
                .and_then(|v| serde_json::from_str::<String>(v).ok())
        };
        if let Some(file) = field("algorithm_file") {
            // A downloaded algorithm travels with the session.
            if let Ok(def) = algorithms::parse_styx(&file) {
                if def.family == self.shared.family && algorithms::register(&def).is_ok() {
                    *self
                        .shared
                        .algorithm_file
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(file);
                }
            }
        }
        if let Some(id) = field("algorithm") {
            match algorithms::find(self.shared.family, Some(&id)) {
                // The values were restored with it: keep them.
                Some(algo) => self.shared.switch(algo, false),
                None => nice_plug::nice_log!(
                    "{}: algorithm {id} isn't here; keeping {}",
                    self.shared.family,
                    self.shared.current().id
                ),
            }
        }
    }
}

// ── The plugin ──────────────────────────────────────────────────────────

pub struct FamilyPlugin<D: FamilyDef> {
    params: Arc<FamilyParams>,
    shared: Arc<Shared>,
    block: Option<Box<FamilyBlock>>,
    /// What the block was last set to, per control (f32 bits).
    applied: Vec<u32>,
    _def: PhantomData<D>,
}

impl<D: FamilyDef> Default for FamilyPlugin<D> {
    fn default() -> Self {
        let first = algorithms::find(D::FAMILY, None).unwrap_or_else(|| {
            algorithms::family(D::FAMILY)
                .into_iter()
                .next()
                .unwrap_or_else(|| unreachable_family(D::FAMILY))
        });
        let sources = plugin_processors::sources_of(D::FAMILY);
        let shared = Arc::new(Shared {
            family: D::FAMILY,
            current: RwLock::new(first),
            algorithm_file: Mutex::new(None),
            config: Mutex::new(None),
            mailbox: Mailbox::default(),
            sources,
            source_values: sources.iter().map(|_| AtomicU32::new(0)).collect(),
            // NaN until the block has run: `null` in the chain's JSON.
            values: (0..SLOTS)
                .map(|_| AtomicU32::new(f32::NAN.to_bits()))
                .collect(),
            revision: AtomicU64::new(0),
            plugin: AtomicPtr::new(std::ptr::null_mut()),
            rescan: rescan_fn::<D>,
            params: OnceLock::new(),
        });
        let params = Arc::new(FamilyParams::new(shared.clone()));
        let _ = shared.params.set(Arc::downgrade(&params));
        Self {
            params,
            shared,
            block: None,
            applied: vec![u32::MAX; SLOTS],
            _def: PhantomData,
        }
    }
}

#[allow(
    clippy::panic,
    reason = "a family with no algorithm at all is a build without its engine: nothing to run"
)]
fn unreachable_family(family: &str) -> Arc<Algorithm> {
    panic!("no {family} algorithms in this build")
}

/// # Safety
///
/// `plugin` is nice-plug's `clap_plugin` for `FamilyPlugin<D>`.
unsafe fn rescan_fn<D: FamilyDef>(plugin: *const clap_plugin) {
    unsafe { clap_request_rescan::<FamilyPlugin<D>>(plugin, true, true) };
}

impl<D: FamilyDef> Plugin for FamilyPlugin<D> {
    const NAME: &'static str = D::NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        _layout: &AudioIOLayout,
        config: &BufferConfig,
        cx: &mut impl ActivateContext<Self>,
    ) -> bool {
        let max_block = usize::try_from(config.max_buffer_size)
            .unwrap_or(1024)
            .max(1);
        *self
            .shared
            .config
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some((config.sample_rate, max_block));
        self.shared.mailbox.collect();
        match FamilyBlock::new(self.shared.current(), config.sample_rate, max_block) {
            Ok(block) => {
                cx.set_latency_samples(block.latency());
                self.block = Some(Box::new(block));
                self.applied.iter_mut().for_each(|a| *a = u32::MAX);
                true
            }
            Err(_) => false,
        }
    }

    fn reset(&mut self) {
        if let Some(b) = self.block.as_mut() {
            b.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _cx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // A new algorithm, swapped in at this block boundary.
        if let Some(new) = self.shared.mailbox.take() {
            if let Some(old) = self.block.replace(new) {
                self.shared.mailbox.retire(old);
            }
            self.applied.iter_mut().for_each(|a| *a = u32::MAX);
        }
        let Some(block) = self.block.as_mut() else {
            return ProcessStatus::Normal;
        };
        // Each control at its slot's modulated position, through the
        // control's own spec: the arithmetic the compiled-in block does.
        let specs = block.params();
        for (i, ((spec, slot), applied)) in specs
            .iter()
            .zip(&self.params.slots)
            .zip(self.applied.iter_mut())
            .enumerate()
        {
            let v = spec.denormalize(slot.modulated_normalized_value());
            if v.to_bits() != *applied {
                *applied = v.to_bits();
                block.set_param(i, v);
                if let Some(slot) = self.shared.values.get(i) {
                    slot.store(v.to_bits(), Ordering::Release);
                }
            }
        }
        if let [l, r, ..] = buffer.as_slice() {
            block.process(l, r);
        }
        for (i, v) in self.shared.source_values.iter().enumerate() {
            v.store(block.mod_source(i).to_bits(), Ordering::Release);
        }
        ProcessStatus::Normal
    }
}

/// Finds a family CLAP's shared state from the raw plugin pointer, and
/// remembers the pointer for rescans.
pub struct Lookup<D>(PhantomData<D>);

impl<D: FamilyDef> SignalPluginLookup for Lookup<D> {
    unsafe fn lookup(plugin: *const clap_plugin) -> Option<Arc<dyn SignalPlugin>> {
        // SAFETY: nice-plug's own `clap_plugin` for `FamilyPlugin<D>`.
        let state = unsafe { clap_extension_state::<FamilyPlugin<D>>(plugin) }?;
        let shared = state.downcast::<Shared>().ok()?;
        shared.plugin.store(plugin.cast_mut(), Ordering::Release);
        Some(shared as Arc<dyn SignalPlugin>)
    }
}

impl<D: FamilyDef> ClapPlugin for FamilyPlugin<D> {
    const CLAP_ID: &'static str = D::CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(D::DESCRIPTION);
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];

    fn clap_extension(id: &CStr) -> Option<*const c_void> {
        (id == daw_clap_ext::sys::FTS_EXT_SIGNAL).then(SignalVtable::<Lookup<D>>::as_ptr)
    }

    fn extension_state(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        Some(self.shared.clone())
    }
}

impl<D: FamilyDef> Vst3Plugin for FamilyPlugin<D> {
    const VST3_CLASS_ID: [u8; 16] = D::VST3_ID;
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx];
}

/// A family CLAP crate: its [`FamilyDef`] and its CLAP/VST3 exports.
#[macro_export]
macro_rules! family_clap {
    ($def:ident, name: $name:literal, family: $family:literal, clap_id: $id:literal, vst3_id: $vst3:literal, description: $desc:literal) => {
        pub struct $def;
        impl $crate::FamilyDef for $def {
            const NAME: &'static str = $name;
            const FAMILY: &'static str = $family;
            const CLAP_ID: &'static str = $id;
            const VST3_ID: [u8; 16] = *$vst3;
            const DESCRIPTION: &'static str = $desc;
        }
        #[cfg(feature = "export")]
        ::nice_plug::nice_export_clap!($crate::FamilyPlugin<$def>);
        #[cfg(feature = "export")]
        ::nice_plug::nice_export_vst3!($crate::FamilyPlugin<$def>);
    };
}

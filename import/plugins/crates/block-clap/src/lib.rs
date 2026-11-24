//! A Signal block as its own CLAP: any `plugin_processors` effect wrapped
//! in nice-plug, the form a block takes in another DAW and as its
//! development copy inside Signal (signal#26).
//!
//! The CLAP form is the compiled-in block, so the two null:
//! - **Parameters** are the effect's table: the same stable ids (they are
//!   the CLAP parameter ids' source), ranges and text, and every one is
//!   modulatable. Linear ranges, so nice-plug's normalized values and
//!   modulation offsets are the arithmetic `ParamSpec::modulated` does.
//! - **Sample-accurate**: nice-plug splits the block at every parameter
//!   event, and the processor is set between pieces, as Signal's chain
//!   does. No smoothing outside the engine.
//! - **`com.fasttrackstudio.signal/1`**: the stable id of each CLAP
//!   parameter, and the effect's modulation sources (gain reduction, the
//!   gate), read after each process, so Signal's macros and modulation
//!   routes work the same on either backend.
//!
//! A block crate is three lines: a [`BlockDef`] and
//! `nice_export_clap!(BlockPlugin<Def>)`.

use std::any::Any;
use std::ffi::{CStr, c_void};
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use daw_clap_ext::ExtError;
use daw_clap_ext::ModSourceInfo;
use daw_clap_ext::plugin::{SignalPlugin, SignalPluginLookup, SignalVtable};
use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::*;
use nice_plug::wrapper::clap::{clap_extension_state, clap_plugin, param_clap_id};
use plugin_processors::{BlockProcessor, ModSourceSpec, ParamSpec, ValueText};

/// Which effect a block CLAP is, and its identity.
pub trait BlockDef: Send + Sync + 'static {
    /// What a host lists: "FTS Comp Block".
    const NAME: &'static str;
    /// The `plugin_processors` effect: "comp".
    const BLOCK: &'static str;
    const CLAP_ID: &'static str;
    const DESCRIPTION: &'static str;
}

/// The effect's parameter table as nice-plug parameters.
pub struct BlockParams {
    specs: &'static [ParamSpec],
    params: Vec<FloatParam>,
}

impl BlockParams {
    fn new(specs: &'static [ParamSpec]) -> Self {
        let params = specs
            .iter()
            .map(|spec| {
                let spec = *spec;
                FloatParam::new(spec.name, spec.default, FloatRange::Linear { min: spec.min, max: spec.max })
                    .with_value_to_string(Arc::new(move |v| ValueText::of(&spec, v).as_str().to_owned()))
                    .with_string_to_value(Arc::new(move |s| spec.parse(s)))
            })
            .collect();
        Self { specs, params }
    }
}

// SAFETY: the pointers are to `self.params`, which is never resized after
// construction and lives as long as the `Arc<BlockParams>` nice-plug holds.
unsafe impl Params for BlockParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.specs.iter().zip(&self.params).map(|(s, p)| (s.id.to_owned(), p.as_ptr(), String::new())).collect()
    }
}

/// What `signal/1` answers from: shared with the plugin, never the plugin
/// itself (it is locked while it processes).
struct Shared {
    specs: &'static [ParamSpec],
    sources: &'static [ModSourceSpec],
    /// Each source's value after the last process, as f32 bits.
    source_values: Vec<AtomicU32>,
    revision: AtomicU64,
    block: &'static str,
}

impl SignalPlugin for Shared {
    fn list_actions(&self) -> String {
        "[]".into()
    }

    fn run_action(&self, id: &str, _args_json: &str) -> Result<String, ExtError> {
        Err(ExtError::NotFound(id.into()))
    }

    fn chain(&self) -> String {
        format!("{{\"block\":\"{}\"}}", self.block)
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn param_stable_id(&self, param_id: u32) -> Option<String> {
        self.specs.iter().find(|s| param_clap_id(s.id) == param_id).map(|s| s.id.to_owned())
    }

    fn mod_sources(&self) -> Vec<ModSourceInfo> {
        self.sources
            .iter()
            .map(|s| ModSourceInfo {
                id: s.id.into(),
                name: s.name.into(),
                unit: match s.unit {
                    plugin_processors::Unit::Db => "dB".into(),
                    plugin_processors::Unit::Hz => "Hz".into(),
                    plugin_processors::Unit::Ms => "ms".into(),
                    plugin_processors::Unit::Percent => "%".into(),
                    _ => String::new(),
                },
            })
            .collect()
    }

    fn mod_source_value(&self, index: usize) -> f64 {
        self.source_values.get(index).map_or(0.0, |v| f64::from(f32::from_bits(v.load(Ordering::Acquire))))
    }

    fn set_session(&self, _session: Option<(&str, &str)>) -> bool {
        false
    }
}

/// A block CLAP.
pub struct BlockPlugin<D: BlockDef> {
    params: Arc<BlockParams>,
    shared: Arc<Shared>,
    block: Option<Box<dyn BlockProcessor>>,
    /// What the processor was last set to, per parameter (f32 bits).
    applied: Vec<u32>,
    _def: PhantomData<D>,
}

impl<D: BlockDef> Default for BlockPlugin<D> {
    fn default() -> Self {
        let specs = plugin_processors::params_of(D::BLOCK).unwrap_or(&[]);
        let sources = plugin_processors::sources_of(D::BLOCK);
        Self {
            params: Arc::new(BlockParams::new(specs)),
            shared: Arc::new(Shared {
                specs,
                sources,
                source_values: sources.iter().map(|_| AtomicU32::new(0)).collect(),
                revision: AtomicU64::new(0),
                block: D::BLOCK,
            }),
            block: None,
            applied: vec![u32::MAX; specs.len()],
            _def: PhantomData,
        }
    }
}

impl<D: BlockDef> Plugin for BlockPlugin<D> {
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
    /// Split at every parameter event: what Signal's chain does.
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, _cx: &mut impl ActivateContext<Self>) -> bool {
        let max_block = usize::try_from(config.max_buffer_size).unwrap_or(1024).max(1);
        match plugin_processors::build(D::BLOCK, config.sample_rate, max_block) {
            Some(Ok(p)) => {
                self.block = Some(p);
                self.applied.iter_mut().for_each(|a| *a = u32::MAX);
                true
            }
            _ => false,
        }
    }

    fn reset(&mut self) {
        if let Some(b) = self.block.as_mut() {
            b.reset();
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _cx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let Some(block) = self.block.as_mut() else { return ProcessStatus::Normal };
        // The parameters as they run (modulated), set where they changed:
        // `set_param` is idempotent, this only saves the work.
        for (i, ((spec, param), applied)) in
            self.params.specs.iter().zip(&self.params.params).zip(self.applied.iter_mut()).enumerate()
        {
            let v = spec.clamp(param.value());
            if v.to_bits() != *applied {
                *applied = v.to_bits();
                block.set_param(i, v);
            }
        }
        let io = buffer.as_slice();
        if let [l, r, ..] = io {
            block.process(l, r);
        }
        for (i, v) in self.shared.source_values.iter().enumerate() {
            v.store(block.mod_source(i).to_bits(), Ordering::Release);
        }
        ProcessStatus::Normal
    }
}

/// Finds a block CLAP's `signal/1` state from the raw plugin pointer.
pub struct Lookup<D>(PhantomData<D>);

impl<D: BlockDef> SignalPluginLookup for Lookup<D>
where
    BlockPlugin<D>: ClapPlugin,
{
    unsafe fn lookup(plugin: *const clap_plugin) -> Option<Arc<dyn SignalPlugin>> {
        // SAFETY: nice-plug's own `clap_plugin` for `BlockPlugin<D>`.
        let state = unsafe { clap_extension_state::<BlockPlugin<D>>(plugin) }?;
        state.downcast::<Shared>().ok().map(|s| s as Arc<dyn SignalPlugin>)
    }
}

impl<D: BlockDef> ClapPlugin for BlockPlugin<D> {
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

/// Define a block CLAP crate's [`BlockDef`] and export it.
#[macro_export]
macro_rules! block_clap {
    ($def:ident, name: $name:literal, block: $block:literal, id: $id:literal, description: $desc:literal) => {
        pub struct $def;
        impl $crate::BlockDef for $def {
            const NAME: &'static str = $name;
            const BLOCK: &'static str = $block;
            const CLAP_ID: &'static str = $id;
            const DESCRIPTION: &'static str = $desc;
        }
        #[cfg(feature = "export")]
        ::nice_plug::nice_export_clap!($crate::BlockPlugin<$def>);
    };
}

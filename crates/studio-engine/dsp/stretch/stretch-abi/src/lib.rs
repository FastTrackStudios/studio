//! Minimal C ABI for swappable real-time time-stretch & pitch-shift engines.
//!
//! Designed to live in `FastTrackStudios/daw` (`daw-stretch-abi`) so that
//! first-party and third-party stretch engines compile as independent dynamic
//! libraries (`.so` / `.dylib` / `.dll`) with zero dependency on `daw` or
//! `studio` internals.

use core::ffi::{CStr, c_char, c_void};

pub const ABI_VERSION: u32 = 1;
pub const VTABLE_SYMBOL: &[u8] = b"daw_stretch_vtable\0";

pub mod caps {
    pub const TIME_STRETCH: u32 = 1 << 0;
    pub const PITCH_SHIFT: u32 = 1 << 1;
    pub const FORMANT_PRESERVE: u32 = 1 << 2;
    pub const STEREO_PHASE_LOCK: u32 = 1 << 3;
    pub const TRANSIENT_AWARE: u32 = 1 << 4;
}

pub type ReadFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    start_frame: i64,
    left: *mut f32,
    right: *mut f32,
    frames: usize,
);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StretchSource {
    pub ctx: *mut c_void,
    pub read: ReadFn,
}

impl StretchSource {
    /// Read frames from the host-provided source callback.
    ///
    /// # Safety
    /// `self.ctx` and `self.read` must be valid for the duration of the call.
    #[inline]
    pub unsafe fn read_frames(&self, start_frame: i64, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        unsafe {
            (self.read)(
                self.ctx,
                start_frame,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                n,
            )
        };
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StretchInfo {
    pub abi_version: u32,
    pub capabilities: u32,
    pub id: *const c_char,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub version: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StretchVTable {
    pub abi_version: u32,
    pub info: unsafe extern "C" fn() -> StretchInfo,
    pub create: unsafe extern "C" fn(sample_rate: f64) -> *mut c_void,
    pub destroy: unsafe extern "C" fn(handle: *mut c_void),
    pub seek: unsafe extern "C" fn(
        handle: *mut c_void,
        source_frame: f64,
        ratio: f64,
        pitch_semitones: f32,
        source: StretchSource,
    ),
    pub render: unsafe extern "C" fn(
        handle: *mut c_void,
        out_left: *mut f32,
        out_right: *mut f32,
        frames: usize,
        ratio: f64,
        pitch_semitones: f32,
        source: StretchSource,
    ),
    pub latency_frames: unsafe extern "C" fn(handle: *const c_void) -> usize,
}

pub type VTableFn = unsafe extern "C" fn() -> *const StretchVTable;

/// Safe Rust representation of [`StretchInfo`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineMetadata {
    pub abi_version: u32,
    pub capabilities: u32,
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
}

impl EngineMetadata {
    /// # Safety
    /// All string pointers in `info` must be valid NUL-terminated UTF-8 C strings.
    pub unsafe fn from_raw(info: StretchInfo) -> Self {
        let to_string = |p: *const c_char| -> String {
            if p.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            }
        };
        Self {
            abi_version: info.abi_version,
            capabilities: info.capabilities,
            id: to_string(info.id),
            name: to_string(info.name),
            vendor: to_string(info.vendor),
            version: to_string(info.version),
        }
    }
}

/// Safe host-side source trait bridged across the C ABI.
pub trait AudioSource {
    fn read(&mut self, start_frame: i64, left: &mut [f32], right: &mut [f32]);
}

/// Convenience in-memory stereo audio source for tests and offline renders.
pub struct SliceSource<'a> {
    pub left: &'a [f32],
    pub right: &'a [f32],
}

impl AudioSource for SliceSource<'_> {
    fn read(&mut self, start_frame: i64, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let len = self.left.len().min(self.right.len()) as i64;
        for i in 0..n {
            let idx = start_frame + i as i64;
            if idx >= 0 && idx < len {
                let u = idx as usize;
                left[i] = self.left[u];
                right[i] = self.right[u];
            } else {
                left[i] = 0.0;
                right[i] = 0.0;
            }
        }
    }
}

unsafe extern "C" fn read_trampoline<S: AudioSource>(
    ctx: *mut c_void,
    start_frame: i64,
    left: *mut f32,
    right: *mut f32,
    frames: usize,
) {
    let src = unsafe { &mut *(ctx.cast::<S>()) };
    let l = unsafe { core::slice::from_raw_parts_mut(left, frames) };
    let r = unsafe { core::slice::from_raw_parts_mut(right, frames) };
    src.read(start_frame, l, r);
}

#[cfg(feature = "host")]
pub mod host {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// A dynamically loaded stretch engine plugin (`.so` / `.dylib` / `.dll`).
    pub struct LoadedPlugin {
        _lib: libloading::Library,
        vtable: StretchVTable,
        meta: EngineMetadata,
        path: PathBuf,
    }

    // SAFETY: The vtable contains static function pointers valid for the lifetime of `_lib`.
    unsafe impl Send for LoadedPlugin {}
    unsafe impl Sync for LoadedPlugin {}

    impl LoadedPlugin {
        /// Load a stretch engine dynamic library from `path`.
        ///
        /// # Safety
        /// Loading an external dynamic library executes its native code.
        pub unsafe fn load(path: impl AsRef<Path>) -> Result<Arc<Self>, String> {
            let path = path.as_ref().to_path_buf();
            let lib = unsafe { libloading::Library::new(&path) }
                .map_err(|e| format!("failed to load {}: {e}", path.display()))?;
            let get_vtable: libloading::Symbol<'_, VTableFn> = unsafe { lib.get(VTABLE_SYMBOL) }
                .map_err(|e| {
                    format!("{} missing daw_stretch_vtable symbol: {e}", path.display())
                })?;
            let ptr = unsafe { get_vtable() };
            if ptr.is_null() {
                return Err(format!("{} returned null vtable", path.display()));
            }
            let vtable = unsafe { *ptr };
            if vtable.abi_version != ABI_VERSION {
                return Err(format!(
                    "{} ABI version mismatch: plugin={} host={}",
                    path.display(),
                    vtable.abi_version,
                    ABI_VERSION
                ));
            }
            let meta = unsafe { EngineMetadata::from_raw((vtable.info)()) };
            Ok(Arc::new(Self {
                _lib: lib,
                vtable,
                meta,
                path,
            }))
        }

        pub fn metadata(&self) -> &EngineMetadata {
            &self.meta
        }

        pub fn path(&self) -> &Path {
            &self.path
        }

        pub fn create_voice(self: &Arc<Self>, sample_rate: f64) -> Result<DynamicVoice, String> {
            let handle = unsafe { (self.vtable.create)(sample_rate) };
            if handle.is_null() {
                return Err(format!("engine {} returned null voice", self.meta.id));
            }
            Ok(DynamicVoice {
                plugin: Arc::clone(self),
                handle,
            })
        }
    }

    /// An instantiated stereo stretch voice backed by a dynamically loaded plugin.
    pub struct DynamicVoice {
        plugin: Arc<LoadedPlugin>,
        handle: *mut c_void,
    }

    unsafe impl Send for DynamicVoice {}

    impl DynamicVoice {
        pub fn engine_id(&self) -> &str {
            &self.plugin.meta.id
        }

        pub fn latency_frames(&self) -> usize {
            unsafe { (self.plugin.vtable.latency_frames)(self.handle) }
        }

        pub fn seek<S: AudioSource>(
            &mut self,
            source_frame: f64,
            ratio: f64,
            pitch_semitones: f32,
            source: &mut S,
        ) {
            let raw = StretchSource {
                ctx: (source as *mut S).cast::<c_void>(),
                read: read_trampoline::<S>,
            };
            unsafe {
                (self.plugin.vtable.seek)(self.handle, source_frame, ratio, pitch_semitones, raw);
            }
        }

        pub fn render<S: AudioSource>(
            &mut self,
            out_left: &mut [f32],
            out_right: &mut [f32],
            ratio: f64,
            pitch_semitones: f32,
            source: &mut S,
        ) {
            let frames = out_left.len().min(out_right.len());
            let raw = StretchSource {
                ctx: (source as *mut S).cast::<c_void>(),
                read: read_trampoline::<S>,
            };
            unsafe {
                (self.plugin.vtable.render)(
                    self.handle,
                    out_left.as_mut_ptr(),
                    out_right.as_mut_ptr(),
                    frames,
                    ratio,
                    pitch_semitones,
                    raw,
                );
            }
        }
    }

    impl Drop for DynamicVoice {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                unsafe { (self.plugin.vtable.destroy)(self.handle) };
                self.handle = core::ptr::null_mut();
            }
        }
    }

    /// Registry of discovered stretch engines — mirrors REAPER's algorithm picker.
    #[derive(Default)]
    pub struct EngineRegistry {
        plugins: Vec<Arc<LoadedPlugin>>,
    }

    impl EngineRegistry {
        pub fn new() -> Self {
            Self::default()
        }

        /// Register a plugin from its library path.
        ///
        /// # Safety
        /// Same as [`LoadedPlugin::load`].
        pub unsafe fn register(
            &mut self,
            path: impl AsRef<Path>,
        ) -> Result<Arc<LoadedPlugin>, String> {
            let plugin = unsafe { LoadedPlugin::load(path)? };
            self.plugins
                .retain(|p| p.metadata().id != plugin.metadata().id);
            self.plugins.push(Arc::clone(&plugin));
            Ok(plugin)
        }

        pub fn get(&self, id: &str) -> Option<&Arc<LoadedPlugin>> {
            self.plugins.iter().find(|p| p.metadata().id == id)
        }

        pub fn list(&self) -> Vec<&EngineMetadata> {
            self.plugins.iter().map(|p| p.metadata()).collect()
        }
    }
}

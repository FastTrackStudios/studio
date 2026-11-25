//! Stereo phase-locked phase vocoder implementing `stretch-abi`.

pub mod map;
pub mod num;
pub mod pv;

use core::ffi::{CStr, c_void};
use stretch_abi::{ABI_VERSION, StretchInfo, StretchSource, StretchVTable, caps};

const ID: &CStr = c"fts.stretch.pv";
const NAME: &CStr = c"FTS Phase Vocoder";
const VENDOR: &CStr = c"FastTrackStudios";
const VERSION: &CStr = c"0.1.0";

struct AbiSource(StretchSource);

impl pv::Source for AbiSource {
    fn read(&mut self, start: i64, left: &mut [f32], right: &mut [f32]) {
        unsafe { self.0.read_frames(start, left, right) };
    }
}

unsafe extern "C" fn info() -> StretchInfo {
    StretchInfo {
        abi_version: ABI_VERSION,
        capabilities: caps::TIME_STRETCH
            | caps::PITCH_SHIFT
            | caps::STEREO_PHASE_LOCK
            | caps::TRANSIENT_AWARE,
        id: ID.as_ptr(),
        name: NAME.as_ptr(),
        vendor: VENDOR.as_ptr(),
        version: VERSION.as_ptr(),
    }
}

unsafe extern "C" fn create(sample_rate: f64) -> *mut c_void {
    let cfg = pv::Config::for_sample_rate(sample_rate.max(8000.0));
    Box::into_raw(Box::new(pv::Stretcher::new(cfg))).cast::<c_void>()
}

unsafe extern "C" fn destroy(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<pv::Stretcher>()) });
    }
}

unsafe extern "C" fn seek(
    handle: *mut c_void,
    source_frame: f64,
    ratio: f64,
    pitch_semitones: f32,
    source: StretchSource,
) {
    if handle.is_null() {
        return;
    }
    let st = unsafe { &mut *(handle.cast::<pv::Stretcher>()) };
    let mut src = AbiSource(source);
    st.seek(source_frame, ratio, pv::semitones(pitch_semitones), &mut src);
}

unsafe extern "C" fn render(
    handle: *mut c_void,
    out_left: *mut f32,
    out_right: *mut f32,
    frames: usize,
    ratio: f64,
    pitch_semitones: f32,
    source: StretchSource,
) {
    if handle.is_null() || out_left.is_null() || out_right.is_null() || frames == 0 {
        return;
    }
    let st = unsafe { &mut *(handle.cast::<pv::Stretcher>()) };
    let l = unsafe { core::slice::from_raw_parts_mut(out_left, frames) };
    let r = unsafe { core::slice::from_raw_parts_mut(out_right, frames) };
    let mut src = AbiSource(source);
    st.render(l, r, ratio, pv::semitones(pitch_semitones), &mut src);
}

unsafe extern "C" fn latency_frames(_handle: *const c_void) -> usize {
    // `seek()` pre-primes the overlap-add pipeline so output aligns at frame 0.
    0
}

static VTABLE: StretchVTable = StretchVTable {
    abi_version: ABI_VERSION,
    info,
    create,
    destroy,
    seek,
    render,
    latency_frames,
};

#[unsafe(no_mangle)]
pub extern "C" fn daw_stretch_vtable() -> *const StretchVTable {
    &raw const VTABLE
}

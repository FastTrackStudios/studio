//! Time-domain WSOLA stretch & pitch-shift engine implementing `stretch-abi`.
//!
//! Uses waveform-similarity overlap-add (`wsola::stretch_planar`) for
//! time-stretching and cubic interpolation for pitch transposition, bridged
//! over the host pull callback (`StretchSource`).

pub mod wsola;

use core::ffi::{CStr, c_void};
use stretch_abi::{ABI_VERSION, StretchInfo, StretchSource, StretchVTable, caps};

const ID: &CStr = c"fts.stretch.wsola";
const NAME: &CStr = c"FTS WSOLA";
const VENDOR: &CStr = c"FastTrackStudios";
const VERSION: &CStr = c"0.1.0";

struct WsolaVoice {
    sample_rate: f64,
    cursor: f64,
    grain: usize,
    scratch_l: Vec<f32>,
    scratch_r: Vec<f32>,
}

impl WsolaVoice {
    fn new(sample_rate: f64) -> Self {
        let sr = sample_rate.max(8000.0);
        let grain = ((1024.0 * sr / 48_000.0).round() as usize).clamp(128, 4096);
        Self {
            sample_rate: sr,
            cursor: 0.0,
            grain,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn seek(&mut self, source_frame: f64) {
        self.cursor = source_frame;
    }

    fn render(
        &mut self,
        out_left: &mut [f32],
        out_right: &mut [f32],
        ratio: f64,
        pitch_semitones: f32,
        source: StretchSource,
    ) {
        let frames = out_left.len().min(out_right.len());
        if frames == 0 {
            return;
        }
        let ratio = ratio.clamp(0.125, 8.0);
        let pitch_ratio = 2_f64.powf(f64::from(pitch_semitones) / 12.0);
        // To keep target tempo `ratio` while transposing pitch by `pitch_ratio`,
        // time-stretch to `pitched_len = frames * pitch_ratio` first, then resample
        // at step `pitch_ratio` back to `frames`.
        let pitched_len = ((frames as f64) * pitch_ratio).ceil().max(1.0) as usize;
        let effective_tempo = ratio / pitch_ratio;
        let needed_in = ((pitched_len as f64) * effective_tempo).ceil() as usize + self.grain * 2;
        let start_frame = self.cursor.floor() as i64;
        let frac = (self.cursor - start_frame as f64).clamp(0.0, 1.0);

        self.scratch_l.resize(needed_in, 0.0);
        self.scratch_r.resize(needed_in, 0.0);
        unsafe {
            source.read_frames(start_frame, &mut self.scratch_l, &mut self.scratch_r);
        }

        let channels = [self.scratch_l.clone(), self.scratch_r.clone()];
        let stretched =
            wsola::stretch_planar(&channels, 0, effective_tempo, pitched_len + 2, self.grain);
        let sl = &stretched[0];
        let sr = &stretched[1];

        for i in 0..frames {
            let pos = frac + (i as f64) * pitch_ratio;
            let idx = (pos.floor() as usize).min(sl.len().saturating_sub(2));
            let t = (pos - idx as f64).clamp(0.0, 1.0) as f32;
            out_left[i] = sl[idx] + (sl[idx + 1] - sl[idx]) * t;
            out_right[i] = sr[idx] + (sr[idx + 1] - sr[idx]) * t;
        }

        self.cursor += (frames as f64) * ratio;
        let _ = self.sample_rate;
    }
}

unsafe extern "C" fn info() -> StretchInfo {
    StretchInfo {
        abi_version: ABI_VERSION,
        capabilities: caps::TIME_STRETCH | caps::PITCH_SHIFT | caps::STEREO_PHASE_LOCK,
        id: ID.as_ptr(),
        name: NAME.as_ptr(),
        vendor: VENDOR.as_ptr(),
        version: VERSION.as_ptr(),
    }
}

unsafe extern "C" fn create(sample_rate: f64) -> *mut c_void {
    Box::into_raw(Box::new(WsolaVoice::new(sample_rate))).cast::<c_void>()
}

unsafe extern "C" fn destroy(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<WsolaVoice>()) });
    }
}

unsafe extern "C" fn seek(
    handle: *mut c_void,
    source_frame: f64,
    _ratio: f64,
    _pitch_semitones: f32,
    _source: StretchSource,
) {
    if handle.is_null() {
        return;
    }
    let v = unsafe { &mut *(handle.cast::<WsolaVoice>()) };
    v.seek(source_frame);
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
    let v = unsafe { &mut *(handle.cast::<WsolaVoice>()) };
    let l = unsafe { core::slice::from_raw_parts_mut(out_left, frames) };
    let r = unsafe { core::slice::from_raw_parts_mut(out_right, frames) };
    v.render(l, r, ratio, pitch_semitones, source);
}

unsafe extern "C" fn latency_frames(_handle: *const c_void) -> usize {
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

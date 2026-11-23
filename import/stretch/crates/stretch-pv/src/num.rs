#[inline]
pub fn count_to_f32(n: usize) -> f32 {
    n as f32
}

#[inline]
pub fn count_to_f64(n: usize) -> f64 {
    n as f64
}

#[inline]
pub fn f32_to_index(x: f32) -> usize {
    if !x.is_finite() || x <= 0.0 {
        0
    } else {
        x as usize
    }
}

#[inline]
pub fn f64_to_index(x: f64) -> usize {
    if !x.is_finite() || x <= 0.0 {
        0
    } else {
        x as usize
    }
}

#[inline]
pub fn trunc_to_i64(x: f64) -> i64 {
    if !x.is_finite() {
        0
    } else {
        x.trunc() as i64
    }
}

//! Lining a piece's drum sample up with its close mic: when the sample's
//! attack lands against the recorded one, and whether it pulls the same
//! way.
//!
//! The Kit Mic starts a sample right at its note, and the note is at the
//! hit marker — the recorded attack's start. A sample with a little air
//! before its attack sounds late by that much, and one whose mic was
//! wired the other way round cancels the recording's low end when they
//! blend. Both are measured here against the piece's own recording:
//! every clean hit's attack against the sample's, the median of the
//! offsets, then the sign of how the first few milliseconds correlate.

/// How a sample sits against the recording it reinforces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    /// How much later the sample must start than its note for its attack
    /// to land on the recorded one, seconds — negative: earlier.
    pub delay: f64,
    /// It pulls the opposite way to the recording: flip it.
    pub inverted: bool,
    /// How alike the two attacks are once lined up, 0..=1 (the mean
    /// correlation's size): low says the measure is a guess.
    pub agreement: f64,
    /// The share of the hits that, alone, say the same about the
    /// polarity, 0..=1: near a half says the sign is a coin toss.
    pub consistent: f64,
    /// How many hits it was measured on.
    pub hits: usize,
}

/// Before a marker, how far back its attack may start, seconds.
const BEFORE: f64 = 0.005;
/// After a marker, how far on its attack may start, seconds.
const AFTER: f64 = 0.025;
/// How long an attack is compared for, seconds: the first half-cycle of a
/// kick, the snap of a snare.
const ATTACK: f64 = 0.004;
/// How far either side of the attacks' starts the comparison looks for
/// the best fit, seconds.
const NUDGE: f64 = 0.0005;
/// An attack starts where the level first reaches this share of its peak.
const ONSET: f32 = 0.2;

/// How `sample` (its sound from the note on, at `rate`) sits against the
/// recording `audio`, whose hits start at `at`. `None` with no hit to
/// measure or a silent sample.
#[must_use]
pub fn align(audio: &[f32], at: &[usize], sample: &[f32], rate: f64) -> Option<Alignment> {
    let secs = |s: f64| num_traits::ToPrimitive::to_usize(&(s * rate).round()).unwrap_or(0);
    let (before, after, attack, nudge) = (secs(BEFORE), secs(AFTER), secs(ATTACK), secs(NUDGE));
    let sample_attack = onset(sample.get(..after.saturating_mul(2))?)?;
    let (heard, mut offsets): (Vec<usize>, Vec<i64>) = at
        .iter()
        .filter_map(|&hit| {
            let from = hit.checked_sub(before)?;
            let window = audio.get(from..hit.checked_add(after)?)?;
            let attack_at = from + onset(window)?;
            Some((attack_at, signed(attack_at) - signed(hit)))
        })
        .unzip();
    let offset = median(&mut offsets)?;
    let fits_at = |lag: i64| correlations(audio, &heard, sample, (sample_attack, lag, attack));
    let lags = -signed(nudge)..=signed(nudge);
    let (lag, fits) = lags
        .map(|lag| (lag, fits_at(lag)))
        .max_by(|a, b| mean(&a.1).abs().total_cmp(&mean(&b.1).abs()))?;
    let fit = mean(&fits);
    let agree = fits.iter().filter(|f| (**f < 0.0) == (fit < 0.0)).count();
    let samples = offset - signed(sample_attack) - lag;
    Some(Alignment {
        delay: num_traits::ToPrimitive::to_f64(&samples).unwrap_or(0.0) / rate,
        inverted: fit < 0.0,
        agreement: fit.abs(),
        consistent: share(agree, fits.len()),
        hits: heard.len(),
    })
}

/// Where a sound's attack starts: the first sample whose size reaches
/// [`ONSET`] of the loudest. `None` for silence.
fn onset(sound: &[f32]) -> Option<usize> {
    let peak = sound.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if peak <= f32::EPSILON {
        return None;
    }
    sound.iter().position(|s| s.abs() >= peak * ONSET)
}

/// For each hit whose attack starts at `heard`, how the recording's first
/// `len` samples there correlate with the sample's from its attack `from`
/// moved by `lag` (-1..=1).
fn correlations(
    audio: &[f32],
    heard: &[usize],
    sample: &[f32],
    (from, lag, len): (usize, i64, usize),
) -> Vec<f64> {
    let Some(theirs) = usize::try_from(signed(from) + lag)
        .ok()
        .and_then(|f| sample.get(f..f.checked_add(len)?))
    else {
        return Vec::new();
    };
    heard
        .iter()
        .filter_map(|&a| audio.get(a..a.checked_add(len)?))
        .map(|ours| correlation(ours, theirs))
        .collect()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / num_traits::ToPrimitive::to_f64(&values.len()).unwrap_or(1.0)
}

fn share(part: usize, whole: usize) -> f64 {
    let to_f64 = |n: usize| num_traits::ToPrimitive::to_f64(&n).unwrap_or(0.0);
    if whole == 0 {
        0.0
    } else {
        to_f64(part) / to_f64(whole)
    }
}

/// How alike two equal-length stretches are in shape, -1..=1: 1 the
/// same, -1 the same upside down.
fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let (mut ab, mut aa, mut bb) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (f64::from(*x), f64::from(*y));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    let size = (aa * bb).sqrt();
    if size <= f64::EPSILON { 0.0 } else { ab / size }
}

fn median(values: &mut [i64]) -> Option<i64> {
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

fn signed(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

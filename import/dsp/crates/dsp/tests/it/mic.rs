//! The mic's convolver against direct-form convolution, and one modelled
//! mic's output pinned against its reference.

use dsp::mic::conv::Convolver;
use dsp::mic::{MicChain, MicModel, Settings};
use dsp_core::assert_golden;
use dsp_core::golden::signal::{noise, widen};

/// A kernel shaped like a mic's: a loud head, then a long decaying tail.
fn kernel(taps: usize) -> Vec<f64> {
    widen(&noise(taps, 5))
        .into_iter()
        .enumerate()
        .map(|(i, x)| x * (-dsp_core::count_to_f64(i) / 2_000.0).exp())
        .collect()
}

/// `y[n] = Σ h[k] x[n − k]`, summed plainly.
fn direct(h: &[f64], x: &[f64], n: usize) -> f64 {
    h.iter()
        .enumerate()
        .take(n + 1)
        .map(|(k, &hk)| hk * x[n - k])
        .sum()
}

/// Largest `|a − b|` over `range`, as a fraction of the reference's peak.
fn worst(conv: &[f64], h: &[f64], x: &[f64], range: std::ops::Range<usize>) -> f64 {
    let reference: Vec<f64> = range.clone().map(|n| direct(h, x, n)).collect();
    let peak = reference.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    range
        .zip(&reference)
        .map(|(n, r)| (conv[n] - r).abs())
        .fold(0.0, f64::max)
        / peak
}

#[test]
fn the_convolver_is_direct_convolution_with_no_latency() {
    // Long enough to reach every partition size the convolver uses.
    let h = kernel(12_000);
    let x = widen(&noise(16_384, 9));
    let mut conv = Convolver::new(h.len());
    conv.set_kernel(&h);
    let y: Vec<f64> = x.iter().map(|&s| conv.process(s)).collect();
    let err = worst(&y, &h, &x, 0..x.len());
    assert!(err < 1e-12, "max error {err:e} of peak");
}

#[test]
fn a_new_kernel_takes_over_from_the_kept_history() {
    let first = kernel(12_000);
    let second: Vec<f64> = kernel(12_000).iter().rev().map(|v| v * 0.5).collect();
    let x = widen(&noise(24_576, 13));
    let mut conv = Convolver::new(first.len());
    conv.set_kernel(&first);
    let swap = 8_000;
    let mut y: Vec<f64> = x.iter().take(swap).map(|&s| conv.process(s)).collect();
    conv.set_kernel(&second);
    y.extend(x.iter().skip(swap).map(|&s| conv.process(s)));
    // Every partition has passed a block boundary since the swap.
    let settled = swap + 8_192;
    let err = worst(&y, &second, &x, settled..x.len());
    assert!(err < 1e-12, "max error {err:e} of peak");
}

#[test]
fn convolving_after_reset_starts_from_silence() {
    let h = kernel(12_000);
    let x = widen(&noise(10_000, 17));
    let mut conv = Convolver::new(h.len());
    conv.set_kernel(&h);
    x.iter().for_each(|&s| {
        conv.process(s);
    });
    conv.reset();
    let y: Vec<f64> = x.iter().map(|&s| conv.process(s)).collect();
    let err = worst(&y, &h, &x, 0..x.len());
    assert!(err < 1e-12, "max error {err:e} of peak");
}

/// The default mic's 48 kHz model, when the private models are installed.
fn default_model() -> Option<MicModel> {
    let dir = std::env::var_os("FTS_MIC_MODELS")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".local/share/fts/mic-models"))
        })?;
    let bytes = std::fs::read(dir.join("48000/LD-47K.micm")).ok()?;
    MicModel::from_bytes(&bytes).ok()
}

/// One second of capsule-pair noise through LD-47K, a non-default pattern
/// and axis so both kernels and the crossfade are in play. Skipped where
/// the (private) models are not installed.
#[test]
fn ld47k_renders_as_recorded() {
    let Some(model) = default_model() else {
        return;
    };
    let mut chain = MicChain::new(model);
    chain.apply(
        Settings {
            pattern: 2,
            axis_deg: 30.0,
            proximity: 25.0,
            ..Settings::default()
        },
        false,
    );
    let front = widen(&noise(48_000, 3));
    let rear = widen(&noise(48_000, 4));
    let out: Vec<f64> = front
        .iter()
        .zip(&rear)
        .map(|(&f, &r)| chain.process(f, r))
        .collect();
    assert_golden!(dsp_core::golden!(), "mic/ld47k", &out);
}

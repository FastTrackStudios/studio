//! Offline rendering of a shared edit across a phase group.

use crate::{HybridPlan, SegmentKind};

/// Render interleaved, sample-aligned channels through one timeline.
/// Stretched segments use one waveform search and overlap schedule for all channels.
pub fn render_group(
    plan: &HybridPlan,
    source: &[f32],
    channels: usize,
    rate: u32,
) -> Result<Vec<f32>, String> {
    if channels == 0 || rate == 0 || !source.len().is_multiple_of(channels) {
        return Err("invalid interleaved source or sample rate".into());
    }
    let frames = source.len() / channels;
    let mut output = source.to_vec();
    let mut previous_end = 0;
    for segment in &plan.segments {
        let to_frame = |seconds: f64| -> Result<usize, String> {
            let frame = (seconds * f64::from(rate)).round();
            if !frame.is_finite() || frame < 0.0 || frame > frames as f64 {
                return Err(format!("segment endpoint {seconds} is outside the WAV"));
            }
            Ok(frame as usize)
        };
        let start = to_frame(segment.timeline_start)?;
        let end = to_frame(segment.timeline_end)?;
        if start < previous_end || end < start {
            return Err("edit segments overlap or run backwards".into());
        }
        if previous_end != 0 {
            output[previous_end * channels..start * channels].fill(0.0);
        }
        previous_end = end;
        let source_frame = |seconds: f64| -> Result<isize, String> {
            let frame = (seconds * f64::from(rate)).round();
            if !frame.is_finite() || frame.abs() > 2.0 * frames as f64 {
                return Err(format!("invalid source endpoint {seconds}"));
            }
            Ok(frame as isize)
        };
        let source_start = source_frame(segment.source_start)?;
        let source_end = source_frame(segment.source_end)?;
        if end == start {
            continue;
        }
        if source_end <= source_start {
            return Err("nonempty output segment has an empty source".into());
        }
        let input = if source_start >= 0 && source_end as usize <= frames {
            std::borrow::Cow::Borrowed(
                &source[source_start as usize * channels..source_end as usize * channels],
            )
        } else {
            let mut padded = vec![0.0; (source_end - source_start) as usize * channels];
            let first = source_start.max(0) as usize;
            let last = source_end.max(0).min(frames as isize) as usize;
            if last > first {
                let offset = (first as isize - source_start) as usize * channels;
                let count = (last - first) * channels;
                padded[offset..offset + count]
                    .copy_from_slice(&source[first * channels..last * channels]);
            }
            std::borrow::Cow::Owned(padded)
        };
        let dest = &mut output[start * channels..end * channels];
        dest.fill(0.0);
        match segment.kind {
            SegmentKind::Slip => {
                let copied = dest.len().min(input.len());
                dest[..copied].copy_from_slice(&input[..copied]);
            }
            SegmentKind::TailStretch { .. } | SegmentKind::Warp { .. } => {
                stretch(&input, dest, channels, rate);
            }
        }
        for (i, frame) in dest.chunks_exact_mut(channels).enumerate() {
            let fade = |distance: usize, seconds: f64| {
                ((distance + 1) as f64 / (seconds * f64::from(rate)).max(1.0)).min(1.0) as f32
            };
            let gain = fade(i, segment.fade_in_s) * fade(end - start - 1 - i, segment.fade_out_s);
            for sample in frame {
                *sample *= gain;
            }
        }
    }
    Ok(output)
}

fn stretch(input: &[f32], output: &mut [f32], channels: usize, rate: u32) {
    let input_frames = input.len() / channels;
    let output_frames = output.len() / channels;
    let window = (rate as usize / 50)
        .max(2)
        .min(input_frames)
        .min(output_frames);
    if window < 2 {
        for frame in output.chunks_exact_mut(channels) {
            frame.copy_from_slice(&input[..channels]);
        }
        return;
    }
    let hop = window / 2;
    let travel = output_frames - window;
    let input_travel = input_frames - window;
    let search = rate as usize / 250;
    output[..window * channels].copy_from_slice(&input[..window * channels]);

    let mut position = 0;
    while position < travel {
        let next = (position + hop).min(travel);
        let overlap = window - (next - position);
        let nominal = (next as f64 * input_travel as f64 / travel as f64).round() as usize;
        let dst = &mut output[next * channels..(next + window) * channels];
        let offset = if next == travel {
            input_travel
        } else {
            let mut best = (f64::NEG_INFINITY, nominal);
            let radius = search.min(input_travel);
            for candidate in (nominal.saturating_sub(radius)..=(nominal + radius).min(input_travel))
                .step_by((rate as usize / 12_000).max(1))
            {
                let start = candidate * channels;
                let incoming = &input[start..start + overlap * channels];
                let score = similarity(&dst[..incoming.len()], incoming);
                if score > best.0
                    || (score == best.0 && candidate.abs_diff(nominal) < best.1.abs_diff(nominal))
                {
                    best = (score, candidate);
                }
            }
            best.1
        };
        for (i, (dest, src)) in dst
            .chunks_exact_mut(channels)
            .zip(input[offset * channels..].chunks_exact(channels))
            .enumerate()
        {
            let mix = ((i + 1) as f32 / (overlap + 1) as f32).min(1.0);
            for (a, &b) in dest.iter_mut().zip(src) {
                *a = *a * (1.0 - mix) + b * mix;
            }
        }
        position = next;
    }
}

fn similarity(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut energy_a, mut energy_b) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (&x, &y) in a.iter().zip(b) {
        let (x, y) = (f64::from(x), f64::from(y));
        dot += x * y;
        energy_a += x * x;
        energy_b += y * y;
    }
    dot / (energy_a * energy_b).sqrt().max(1e-20)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioSegment, EditStrategy, PlanAudit};

    #[test]
    fn stretched_channels_share_the_same_waveform_offsets() {
        let source: Vec<_> = (0..4800)
            .flat_map(|i| {
                let a = (i as f32 * 0.13).sin();
                [a, -0.5 * a]
            })
            .collect();
        let plan = HybridPlan {
            strategy: EditStrategy::Hybrid,
            segments: vec![AudioSegment {
                source_start: 0.0,
                source_end: 0.06,
                timeline_start: 0.0,
                timeline_end: 0.10,
                fade_in_s: 0.0,
                fade_out_s: 0.0,
                kind: SegmentKind::TailStretch {
                    stretch_ratio: 0.10 / 0.06,
                },
            }],
            audit: PlanAudit {
                total_segments: 1,
                slip_segments: 0,
                tail_stretch_segments: 1,
                warp_segments: 0,
                unclosed_gaps: 0,
                max_stretch_ratio: 0.10 / 0.06,
                attacks_protected: true,
            },
        };
        let rendered = render_group(&plan, &source, 2, 48_000).unwrap();
        assert_ne!(rendered, source);
        assert!(
            rendered
                .chunks_exact(2)
                .all(|frame| -0.5 * frame[0] == frame[1])
        );
        assert_eq!(&rendered[9598..], &source[5758..5760]);
    }
}

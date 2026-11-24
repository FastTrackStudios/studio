//! The pictures a listen draws for an agent to look at: a waveform, a
//! spectrogram, a third-octave spectrum (or two, and their difference),
//! and an averaged hit with every mic of its piece overlaid.
//!
//! Each is an SVG built from plain numbers (honest, testable), drawn to a
//! PNG by [`write_png`] (resvg, the system's fonts for the labels). The
//! spectrogram's pixels go in as an embedded PNG under the SVG's axes.

use std::fmt::Write as _;
use std::path::Path;

use base64::Engine as _;

/// The picture's size, px.
const W: f64 = 1200.0;
/// Inner margins: left, right, top, bottom.
const ML: f64 = 64.0;
const MR: f64 = 16.0;
const MT: f64 = 34.0;
const MB: f64 = 40.0;
/// A line per series, then the overlays: distinct on the dark background.
const COLOURS: [&str; 8] = [
    "#4fc3f7", "#ffb74d", "#81c784", "#e57373", "#ba68c8", "#fff176", "#4db6ac", "#f06292",
];
const BACKGROUND: &str = "#15171c";
const GRID: &str = "#2c313b";
const TEXT: &str = "#c8ccd4";

/// An SVG's opening, background and title.
fn open(h: f64, title: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{h}" font-family="DejaVu Sans, Noto Sans, sans-serif" font-size="13"><rect width="100%" height="100%" fill="{BACKGROUND}"/><text x="{ML}" y="22" fill="{TEXT}" font-size="15">{}</text>"##,
        escape(title)
    )
}

/// Text safe inside SVG.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A plot area: where it is, and its two axes' ranges.
#[derive(Clone, Copy)]
struct Area {
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
    xs: (f64, f64),
    ys: (f64, f64),
    /// The x axis is logarithmic (frequency).
    log_x: bool,
}

impl Area {
    fn x(&self, v: f64) -> f64 {
        let t = if self.log_x {
            (v.max(1e-9).ln() - self.xs.0.ln()) / (self.xs.1.ln() - self.xs.0.ln())
        } else {
            (v - self.xs.0) / (self.xs.1 - self.xs.0)
        };
        self.x0 + t.clamp(0.0, 1.0) * self.w
    }
    fn y(&self, v: f64) -> f64 {
        let t = (v - self.ys.0) / (self.ys.1 - self.ys.0);
        self.y0 + self.h - t.clamp(0.0, 1.0) * self.h
    }
}

/// An area's frame, grid and tick labels; `x_ticks` and `y_ticks` are
/// (value, label) pairs; `x_name`/`y_name` the axes' names.
fn axes(
    svg: &mut String,
    a: &Area,
    (x_ticks, y_ticks): (&[(f64, String)], &[(f64, String)]),
    (x_name, y_name): (&str, &str),
) {
    let _ = write!(
        svg,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{GRID}"/>"#,
        a.x0, a.y0, a.w, a.h
    );
    for (v, label) in x_ticks {
        let x = a.x(*v);
        let _ = write!(
            svg,
            r#"<line x1="{x:.1}" y1="{}" x2="{x:.1}" y2="{}" stroke="{GRID}"/><text x="{x:.1}" y="{}" fill="{TEXT}" text-anchor="middle">{}</text>"#,
            a.y0,
            a.y0 + a.h,
            a.y0 + a.h + 16.0,
            escape(label)
        );
    }
    for (v, label) in y_ticks {
        let y = a.y(*v);
        let _ = write!(
            svg,
            r#"<line x1="{}" y1="{y:.1}" x2="{}" y2="{y:.1}" stroke="{GRID}"/><text x="{}" y="{:.1}" fill="{TEXT}" text-anchor="end">{}</text>"#,
            a.x0,
            a.x0 + a.w,
            a.x0 - 6.0,
            y + 4.0,
            escape(label)
        );
    }
    let _ = write!(
        svg,
        r#"<text x="{}" y="{}" fill="{TEXT}" text-anchor="middle">{}</text><text x="14" y="{}" fill="{TEXT}" transform="rotate(-90 14 {})" text-anchor="middle">{}</text>"#,
        a.x0 + a.w / 2.0,
        a.y0 + a.h + 32.0,
        escape(x_name),
        a.y0 + a.h / 2.0,
        a.y0 + a.h / 2.0,
        escape(y_name)
    );
}

/// A polyline through `points`.
fn line(
    svg: &mut String,
    a: &Area,
    points: impl Iterator<Item = (f64, f64)>,
    colour: &str,
    width: f64,
) {
    let mut d = String::new();
    for (i, (x, y)) in points.enumerate() {
        let _ = write!(
            d,
            "{}{:.1},{:.1}",
            if i == 0 { "M" } else { "L" },
            a.x(x),
            a.y(y)
        );
    }
    let _ = write!(
        svg,
        r#"<path d="{d}" fill="none" stroke="{colour}" stroke-width="{width}" stroke-linejoin="round"/>"#
    );
}

/// A legend: one coloured name a series, along the top right.
fn legend(svg: &mut String, names: &[String], y: f64) {
    let mut x = W - MR;
    for (i, name) in names.iter().enumerate().rev() {
        let w = 14.0 + 7.5 * name.chars().count() as f64;
        x -= w + 12.0;
        let _ = write!(
            svg,
            r#"<rect x="{x:.0}" y="{:.0}" width="10" height="10" fill="{}"/><text x="{:.0}" y="{:.0}" fill="{TEXT}">{}</text>"#,
            y - 9.0,
            COLOURS[i % COLOURS.len()],
            x + 14.0,
            y,
            escape(name)
        );
    }
}

/// Evenly spaced ticks over `range`, about `n` of them, labelled by `fmt`.
fn ticks(range: (f64, f64), n: usize, fmt: impl Fn(f64) -> String) -> Vec<(f64, String)> {
    let span = range.1 - range.0;
    if span <= 0.0 {
        return Vec::new();
    }
    let raw = span / n.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw)
        .unwrap_or(raw);
    let first = (range.0 / step).ceil() * step;
    (0..)
        .map(|i| first + f64::from(i) * step)
        .take_while(|v| *v <= range.1 + 1e-9)
        .map(|v| (v, fmt(v)))
        .collect()
}

/// The frequency ticks of a 20 Hz – 20 kHz axis.
fn hz_ticks() -> Vec<(f64, String)> {
    [20.0, 50.0, 100.0, 200.0, 500.0, 1e3, 2e3, 5e3, 1e4, 2e4]
        .iter()
        .map(|f| {
            (
                *f,
                if *f >= 1e3 {
                    format!("{}k", f / 1e3)
                } else {
                    format!("{f}")
                },
            )
        })
        .collect()
}

/// A stereo waveform (interleaved, `seconds_from` the time of its first
/// frame): each pixel column's lowest and highest sample of L (above) and
/// R (below), and their RMS.
#[must_use]
pub fn waveform(signal: &[f32], rate: u32, seconds_from: f64, title: &str) -> String {
    let h = 520.0;
    let mut svg = open(h, title);
    let frames = signal.len() / 2;
    let seconds = frames as f64 / f64::from(rate);
    let lane = (h - MT - MB - 20.0) / 2.0;
    let columns = (W - ML - MR) as usize;
    for (ch, name) in [(0usize, "L"), (1, "R")] {
        let a = Area {
            x0: ML,
            y0: MT + ch as f64 * (lane + 20.0),
            w: W - ML - MR,
            h: lane,
            xs: (seconds_from, seconds_from + seconds),
            ys: (-1.0, 1.0),
            log_x: false,
        };
        let y_ticks = [
            (-1.0, "-1".to_owned()),
            (0.0, name.to_owned()),
            (1.0, "1".to_owned()),
        ];
        let x_ticks = if ch == 1 {
            ticks(a.xs, 10, |v| format!("{v:.1}"))
        } else {
            Vec::new()
        };
        axes(
            &mut svg,
            &a,
            (&x_ticks, &y_ticks),
            (if ch == 1 { "seconds" } else { "" }, ""),
        );
        let per = (frames / columns).max(1);
        let mut top = String::new();
        let mut rms_line = Vec::new();
        for c in 0..columns.min(frames) {
            let s = c * per;
            let e = ((c + 1) * per).min(frames);
            let (mut lo, mut hi, mut sq) = (0.0f32, 0.0f32, 0.0f64);
            for f in s..e {
                let v = signal[f * 2 + ch];
                lo = lo.min(v);
                hi = hi.max(v);
                sq += f64::from(v).powi(2);
            }
            let x = a.x0 + c as f64 * a.w / columns as f64;
            let _ = write!(
                top,
                "M{x:.1},{:.1}V{:.1}",
                a.y(f64::from(hi)),
                a.y(f64::from(lo))
            );
            rms_line.push((
                seconds_from + s as f64 / f64::from(rate),
                (sq / (e - s).max(1) as f64).sqrt(),
            ));
        }
        let _ = write!(
            svg,
            r#"<path d="{top}" stroke="{}" stroke-width="1"/>"#,
            COLOURS[0]
        );
        line(&mut svg, &a, rms_line.iter().copied(), COLOURS[1], 1.0);
        line(
            &mut svg,
            &a,
            rms_line.iter().map(|(x, y)| (*x, -y)),
            COLOURS[1],
            1.0,
        );
    }
    legend(&mut svg, &["peak".into(), "RMS".into()], 22.0);
    svg.push_str("</svg>");
    svg
}

/// A spectrogram's pixels: `columns` × `rows` of dB (rows from 20 Hz at
/// the bottom to 20 kHz at the top, log-spaced), from the mid of `signal`.
#[must_use]
pub fn spectrogram_db(signal: &[f32], rate: u32, (columns, rows): (usize, usize)) -> Vec<Vec<f32>> {
    use realfft::RealFftPlanner;
    const N: usize = 4_096;
    let mid = super::measure::mid(signal);
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N);
    let window: Vec<f32> = (0..N)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos())
        .collect();
    let norm = 2.0 / window.iter().sum::<f32>();
    let bin = f64::from(rate) / N as f64;
    let row_hz = |r: usize| 20.0 * 1000f64.powf(r as f64 / rows as f64);
    let hop = (mid.len().saturating_sub(N) / columns.max(1)).max(1);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    (0..columns)
        .map(|c| {
            let at = c * hop;
            for (i, x) in input.iter_mut().enumerate() {
                *x = mid.get(at + i).copied().unwrap_or(0.0) * window[i];
            }
            if fft.process(&mut input, &mut output).is_err() {
                return vec![-150.0; rows];
            }
            (0..rows)
                .map(|r| {
                    let (lo, hi) = (row_hz(r) / bin, row_hz(r + 1) / bin);
                    let (lo, hi) = (
                        lo.floor() as usize,
                        (hi.ceil() as usize).max(lo as usize + 1),
                    );
                    let m = output[lo.min(output.len() - 1)..hi.min(output.len())]
                        .iter()
                        .map(|c| c.norm())
                        .fold(0.0f32, f32::max)
                        * norm;
                    if m <= 1e-8 { -150.0 } else { 20.0 * m.log10() }
                })
                .collect()
        })
        .collect()
}

/// A colour for `db` between `floor` and 0 dB (dark blue … yellow … white).
fn heat(db: f32, floor: f32) -> [u8; 3] {
    let t = ((db - floor) / -floor).clamp(0.0, 1.0);
    let stops: [(f32, [f32; 3]); 5] = [
        (0.0, [10.0, 10.0, 30.0]),
        (0.3, [60.0, 20.0, 120.0]),
        (0.55, [200.0, 40.0, 90.0]),
        (0.8, [250.0, 170.0, 40.0]),
        (1.0, [255.0, 255.0, 220.0]),
    ];
    let i = stops
        .iter()
        .rposition(|(s, _)| *s <= t)
        .unwrap_or(0)
        .min(stops.len() - 2);
    let (s0, c0) = stops[i];
    let (s1, c1) = stops[i + 1];
    let u = ((t - s0) / (s1 - s0)).clamp(0.0, 1.0);
    let mix = |k: usize| (c0[k] + (c1[k] - c0[k]) * u) as u8;
    [mix(0), mix(1), mix(2)]
}

/// A spectrogram of `signal` (`seconds_from` its first frame's time), its
/// pixels embedded under labelled axes.
///
/// # Errors
/// Its pixels would not encode.
pub fn spectrogram(
    signal: &[f32],
    rate: u32,
    seconds_from: f64,
    title: &str,
) -> Result<String, String> {
    const FLOOR: f32 = -96.0;
    let h = 560.0;
    let a = Area {
        x0: ML,
        y0: MT,
        w: W - ML - MR,
        h: h - MT - MB,
        xs: (
            seconds_from,
            seconds_from + signal.len() as f64 / 2.0 / f64::from(rate),
        ),
        ys: (20f64.log10(), 20_000f64.log10()),
        log_x: false,
    };
    let (columns, rows) = (a.w as usize, a.h as usize);
    let db = spectrogram_db(signal, rate, (columns, rows));
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(columns as u32, rows as u32).ok_or("an empty spectrogram")?;
    let data = pixmap.data_mut();
    for (c, col) in db.iter().enumerate() {
        for (r, v) in col.iter().enumerate() {
            let [cr, cg, cb] = heat(*v, FLOOR);
            let i = ((rows - 1 - r) * columns + c) * 4;
            data[i..i + 4].copy_from_slice(&[cr, cg, cb, 255]);
        }
    }
    let png = pixmap.encode_png().map_err(|e| e.to_string())?;
    let mut svg = open(h, title);
    let _ = write!(
        svg,
        r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" href="data:image/png;base64,{}"/>"#,
        a.x0,
        a.y0,
        a.w,
        a.h,
        base64::engine::general_purpose::STANDARD.encode(png)
    );
    let y_ticks: Vec<(f64, String)> = hz_ticks()
        .into_iter()
        .map(|(f, l)| (f.log10(), l))
        .collect();
    axes(
        &mut svg,
        &a,
        (&ticks(a.xs, 10, |v| format!("{v:.1}")), &y_ticks),
        ("seconds", "Hz"),
    );
    let _ = write!(
        svg,
        r#"<text x="{}" y="22" fill="{TEXT}" text-anchor="end">colour: {FLOOR} dB (dark) … 0 dBFS (white)</text>"#,
        W - MR
    );
    svg.push_str("</svg>");
    Ok(svg)
}

/// Third-octave spectra (`bands` dB at `centres` Hz), one line each, named
/// by `names`; with two, their difference (the second minus the first)
/// below.
#[must_use]
pub fn spectra(centres: &[f64], series: &[(String, Vec<f64>)], title: &str) -> String {
    let diff = series.len() == 2;
    let h = if diff { 720.0 } else { 460.0 };
    let mut svg = open(h, title);
    let top_h = if diff { 400.0 } else { h - MT - MB };
    let all = series
        .iter()
        .flat_map(|(_, b)| b.iter().copied())
        .filter(|v| *v > -149.0);
    let hi = all.clone().fold(f64::MIN, f64::max).max(-60.0);
    let lo = all.fold(f64::MAX, f64::min).max(hi - 90.0);
    let hi = (hi / 6.0).ceil() * 6.0;
    let lo = (lo / 6.0).floor() * 6.0;
    let a = Area {
        x0: ML,
        y0: MT,
        w: W - ML - MR,
        h: top_h,
        xs: (20.0, 20_000.0),
        ys: (lo, hi),
        log_x: true,
    };
    axes(
        &mut svg,
        &a,
        (&hz_ticks(), &ticks(a.ys, 8, |v| format!("{v:.0}"))),
        ("Hz", "dB"),
    );
    for (i, (_, bands)) in series.iter().enumerate() {
        line(
            &mut svg,
            &a,
            centres.iter().copied().zip(bands.iter().map(|b| b.max(lo))),
            COLOURS[i % COLOURS.len()],
            2.0,
        );
    }
    legend(
        &mut svg,
        &series.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        22.0,
    );
    if let [(_, a_bands), (_, b_bands)] = series {
        let d: Vec<f64> = a_bands.iter().zip(b_bands).map(|(x, y)| y - x).collect();
        let most = d
            .iter()
            .fold(1.0f64, |m, v| m.max(v.abs()))
            .min(24.0)
            .ceil();
        let b = Area {
            x0: ML,
            y0: MT + top_h + 56.0,
            w: W - ML - MR,
            h: h - (MT + top_h + 56.0) - MB,
            xs: (20.0, 20_000.0),
            ys: (-most, most),
            log_x: true,
        };
        axes(
            &mut svg,
            &b,
            (&hz_ticks(), &ticks(b.ys, 6, |v| format!("{v:+.0}"))),
            ("Hz", "B − A, dB"),
        );
        line(
            &mut svg,
            &b,
            [(20.0, 0.0), (20_000.0, 0.0)].into_iter(),
            TEXT,
            1.0,
        );
        line(
            &mut svg,
            &b,
            centres.iter().copied().zip(d),
            COLOURS[2],
            2.0,
        );
    }
    svg.push_str("</svg>");
    svg
}

/// An averaged hit of each mic (mono, the onset `pre_ms` into each),
/// overlaid: the whole window's waveform, its envelope in dB, and its
/// first 20 ms close up — where a mic upside down or late against another
/// shows.
#[must_use]
pub fn hit(mics: &[(String, Vec<f32>, Vec<f32>)], rate: u32, pre_ms: f64, title: &str) -> String {
    let h = 900.0;
    let mut svg = open(h, title);
    let ms = |i: usize| i as f64 * 1e3 / f64::from(rate) - pre_ms;
    let len = mics.first().map_or(0, |m| m.1.len());
    let most = mics
        .iter()
        .flat_map(|m| m.1.iter())
        .fold(1e-6f32, |m, v| m.max(v.abs()));
    let most = f64::from(most) * 1.05;
    let panel = |y0: f64, ph: f64, xs: (f64, f64), ys: (f64, f64)| Area {
        x0: ML,
        y0,
        w: W - ML - MR,
        h: ph,
        xs,
        ys,
        log_x: false,
    };
    let whole = (ms(0), ms(len));
    let a = panel(MT, 250.0, whole, (-most, most));
    axes(
        &mut svg,
        &a,
        (
            &ticks(whole, 12, |v| format!("{v:.0}")),
            &[(0.0, "0".into())],
        ),
        ("ms from the hit", "waveform"),
    );
    let b = panel(MT + 300.0, 230.0, whole, (-60.0, 0.0));
    axes(
        &mut svg,
        &b,
        (
            &ticks(whole, 12, |v| format!("{v:.0}")),
            &ticks(b.ys, 6, |v| format!("{v:.0}")),
        ),
        ("ms from the hit", "envelope dBFS"),
    );
    let close = (-2.0f64.min(pre_ms), 20.0);
    let c = panel(MT + 580.0, 240.0, close, (-most, most));
    axes(
        &mut svg,
        &c,
        (
            &ticks(close, 11, |v| format!("{v:.0}")),
            &[(0.0, "0".into())],
        ),
        ("ms from the hit (close up)", "waveform"),
    );
    let step = (len / 2_400).max(1);
    for (i, (_, wave, env)) in mics.iter().enumerate() {
        let colour = COLOURS[i % COLOURS.len()];
        line(
            &mut svg,
            &a,
            (0..len).step_by(step).map(|k| (ms(k), f64::from(wave[k]))),
            colour,
            1.2,
        );
        line(
            &mut svg,
            &b,
            (0..len)
                .step_by(step)
                .map(|k| (ms(k), super::measure::amp_db(f64::from(env[k])).max(-60.0))),
            colour,
            1.5,
        );
        let from = ((close.0 + pre_ms) * f64::from(rate) / 1e3).max(0.0) as usize;
        let to = (((close.1 + pre_ms) * f64::from(rate) / 1e3) as usize).min(len);
        line(
            &mut svg,
            &c,
            (from..to).map(|k| (ms(k), f64::from(wave[k]))),
            colour,
            1.5,
        );
    }
    legend(
        &mut svg,
        &mics.iter().map(|m| m.0.clone()).collect::<Vec<_>>(),
        22.0,
    );
    svg.push_str("</svg>");
    svg
}

/// The fonts the labels are drawn with: the system's, read once.
fn fonts() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static FONTS: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            std::sync::Arc::new(db)
        })
        .clone()
}

/// Draw `svg` to the PNG `path` (its folder made first).
///
/// # Errors
/// The SVG would not parse, or the file would not be written.
pub fn write_png(svg: &str, path: &Path) -> Result<(), String> {
    use resvg::{tiny_skia, usvg};
    let opts = usvg::Options {
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &opts).map_err(|e| format!("the picture: {e}"))?;
    let size = tree.size().to_int_size();
    let mut pixmap =
        tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("an empty picture")?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    pixmap
        .save_png(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_is_an_svg_resvg_reads() {
        let rate = 48_000;
        let s: Vec<f32> = (0..rate as usize)
            .flat_map(|i| {
                let v = (i as f32 * 0.03).sin() * 0.5;
                [v, -v]
            })
            .collect();
        let opts = resvg::usvg::Options::default();
        let parse = |svg: &str| {
            resvg::usvg::Tree::from_str(svg, &opts)
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        parse(&waveform(&s, rate, 10.0, "a & b")).unwrap();
        parse(&spectrogram(&s, rate, 10.0, "x").unwrap()).unwrap();
        let centres = crate::mix_measure::analysis::Bands::Third.centres();
        let flat = vec![-20.0; centres.len()];
        parse(&spectra(
            &centres,
            &[("A".into(), flat.clone()), ("B".into(), flat)],
            "ab",
        ))
        .unwrap();
        let wave: Vec<f32> = s.iter().step_by(2).take(4_800).copied().collect();
        parse(&hit(
            &[("In".into(), wave.clone(), wave)],
            rate,
            10.0,
            "kick",
        ))
        .unwrap();
    }
}

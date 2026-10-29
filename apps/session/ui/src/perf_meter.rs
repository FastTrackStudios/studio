//! The Performance meter: REAPER's (View → Performance Meter), over the
//! window. The engine reads itself twice a second (`session_engine::
//! perf_meter`) and every window follows it (`view_client::perf`); this
//! draws what it last said:
//!
//! - a graph of the last minute — the engine's CPU, the audio callback's
//!   (RT) and the render ahead's — with the CPU's cur / avg / range;
//! - the totals: CPU, RT CPU, the longest block against a block's time,
//!   the dropouts so far, disk, RAM;
//! - every track: its FX CPU, how many FX, their delay compensation, and
//!   its media CPU (its items read, resampled, stretched). A track's CPU is
//!   of one core: past 100 it alone cannot keep up. A header sorts by it.
//!
//! Shown or hidden by `SESSION_PERF_METER` (the View menu), from any
//! thread: the panel takes the ask up on its next read. It reads only
//! while it is open.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;
use session_engine::view::{PerfFrame, TrackPerf};

/// Whether the meter is shown, as asked from anywhere.
static OPEN: AtomicBool = AtomicBool::new(false);

/// Show the meter, or hide it if it is shown.
pub fn toggle() {
    OPEN.fetch_xor(true, Ordering::AcqRel);
}

/// Whether the meter is shown.
#[must_use]
pub fn open() -> bool {
    OPEN.load(Ordering::Acquire)
}

/// How often the panel looks for a new frame: half the engine's period, so
/// a frame is drawn within a quarter second of being read.
const POLL_MS: u64 = 250;

/// How many frames the graph keeps: a minute at two a second.
pub const HISTORY: usize = 120;

/// One frame's points on the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    /// The engine's CPU, of all cores.
    pub cpu: f64,
    /// The callback's and the render ahead's, of one core.
    pub rt: f64,
    pub ahead: f64,
}

/// A series' current value, mean, least and most.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub cur: f64,
    pub avg: f64,
    pub min: f64,
    pub max: f64,
}

/// `values`' [`Summary`], oldest first; all zero when there are none.
#[must_use]
pub fn summary(values: impl IntoIterator<Item = f64>) -> Summary {
    let mut out: Option<Summary> = None;
    let mut sum = 0.0;
    let mut n = 0usize;
    for v in values {
        sum += v;
        n += 1;
        out = Some(match out {
            None => Summary {
                cur: v,
                avg: v,
                min: v,
                max: v,
            },
            Some(s) => Summary {
                cur: v,
                avg: 0.0,
                min: s.min.min(v),
                max: s.max.max(v),
            },
        });
    }
    out.map_or_else(Summary::default, |s| Summary {
        avg: sum / n as f64,
        ..s
    })
}

/// What the track table is sorted by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Track,
    FxCpu,
    FxCount,
    Pdc,
    MediaCpu,
}

/// A column and which way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub column: Column,
    pub descending: bool,
}

impl Default for Sort {
    /// The costliest FX first: what is asked of the meter most.
    fn default() -> Self {
        Self {
            column: Column::FxCpu,
            descending: true,
        }
    }
}

impl Sort {
    /// The sort after a click on `column`'s header: the other way on the
    /// same column; another column from its natural end — the song's order
    /// for the tracks, the most first for the numbers.
    #[must_use]
    pub fn clicked(self, column: Column) -> Self {
        if column == self.column {
            Self {
                descending: !self.descending,
                ..self
            }
        } else {
            Self {
                column,
                descending: column != Column::Track,
            }
        }
    }
}

/// `tracks` in `sort`'s order; ties stay in the song's order.
#[must_use]
pub fn sorted(tracks: &[TrackPerf], sort: Sort) -> Vec<&TrackPerf> {
    let mut out: Vec<&TrackPerf> = tracks.iter().collect();
    out.sort_by(|a, b| {
        let order = match sort.column {
            Column::Track => a.index.cmp(&b.index),
            Column::FxCpu => a.fx_cpu_pct.total_cmp(&b.fx_cpu_pct),
            Column::FxCount => a.fx_count.cmp(&b.fx_count),
            Column::Pdc => a.pdc_samples.cmp(&b.pdc_samples),
            Column::MediaCpu => a.media_cpu_pct.total_cmp(&b.media_cpu_pct),
        };
        let order = if sort.descending {
            order.reverse()
        } else {
            order
        };
        order.then(a.index.cmp(&b.index))
    });
    out
}

/// A share as REAPER prints it: two places.
fn pct(v: f64) -> String {
    format!("{v:.2}%")
}

/// Bytes, in the largest unit under a thousand of it.
fn bytes(n: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{v:.0}{}", UNITS[unit])
    } else {
        format!("{v:.1}{}", UNITS[unit])
    }
}

const GRAPH_W: f64 = 536.0;
const GRAPH_H: f64 = 84.0;

/// `values` as an SVG polyline's points over the graph, the newest at the
/// right edge, `top` at the top.
fn line(values: impl Iterator<Item = f64>, top: f64) -> String {
    let values: Vec<f64> = values.collect();
    let step = GRAPH_W / (HISTORY.max(2) - 1) as f64;
    let start = HISTORY.saturating_sub(values.len()) as f64 * step;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = start + i as f64 * step;
            let y = GRAPH_H - (v / top).clamp(0.0, 1.0) * GRAPH_H;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const CPU_INK: &str = "#4c9aff";
const RT_INK: &str = "#45c46b";
const AHEAD_INK: &str = "#e8a23a";

const CARD: &str = "position:absolute; top:56px; right:16px; width:560px; \
                    max-height:calc(100% - 112px); box-sizing:border-box; padding:12px; \
                    border-radius:10px; background:#17191d; border:1px solid #2a2d33; \
                    color:#e6e6e6; font-size:12px; line-height:16px; display:flex; \
                    flex-direction:column; gap:10px; pointer-events:auto; \
                    box-shadow:0 8px 24px rgba(0,0,0,0.45); overflow:hidden;";
const HEAD: &str = "font-size:10px; font-weight:700; color:#8a8f98; letter-spacing:0.04em;";
const CELL: &str = "min-width:0; overflow:hidden; white-space:nowrap;";
const MUTED: &str = "#8a8f98";

/// One header of the track table: its label, and the sort a click makes.
#[component]
fn Header(
    label: &'static str,
    column: Column,
    mut sort: Signal<Sort>,
    width: &'static str,
    right: bool,
) -> Element {
    let now = sort();
    let mark = if now.column != column {
        ""
    } else if now.descending {
        " ▼"
    } else {
        " ▲"
    };
    let align = if right { "flex-end" } else { "flex-start" };
    let ink = if now.column == column {
        "#e6e6e6"
    } else {
        MUTED
    };
    rsx! {
        div {
            style: "{CELL} {width} display:flex; justify-content:{align}; cursor:pointer; color:{ink};",
            onclick: move |_| {
                let next = sort.peek().clicked(column);
                sort.set(next);
            },
            "{label}{mark}"
        }
    }
}

/// A line of the totals: a name and what it reads.
#[component]
fn Total(name: &'static str, value: String) -> Element {
    rsx! {
        div { style: "display:flex; gap:6px; min-width:0;",
            span { style: "color:{MUTED}; white-space:nowrap;", "{name}" }
            span { style: "{CELL}", "{value}" }
        }
    }
}

/// The meter, over the window while it is open. No props: it reads the
/// engine's last frame.
#[component]
pub fn PerfMeter() -> Element {
    let mut shown = use_signal(open);
    let mut frame = use_signal(|| None::<Arc<PerfFrame>>);
    let mut history = use_signal(VecDeque::<Sample>::new);
    let sort = use_signal(Sort::default);
    use_future(move || async move {
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(POLL_MS)).await;
            let now = open();
            if now != *shown.peek() {
                shown.set(now);
            }
            if !now {
                continue;
            }
            let Some(latest) = crate::view_client::perf() else {
                continue;
            };
            let new = frame
                .peek()
                .as_ref()
                .is_none_or(|f| f.version != latest.version);
            if !new {
                continue;
            }
            history.with_mut(|h| {
                if h.len() == HISTORY {
                    h.pop_front();
                }
                h.push_back(Sample {
                    cpu: latest.cpu_pct,
                    rt: latest.rt_cpu_pct,
                    ahead: latest.ahead_cpu_pct,
                });
            });
            frame.set(Some(latest));
        }
    });
    if !shown() {
        return rsx! {};
    }
    let close = move |_| {
        toggle();
        shown.set(false);
    };
    let f = frame();
    let h = history();
    let cpu = summary(h.iter().map(|s| s.cpu));
    // One scale for the three lines: a core, or more when a line is past it.
    let top = h
        .iter()
        .flat_map(|s| [s.cpu, s.rt, s.ahead])
        .fold(100.0_f64, f64::max);
    let cpu_line = line(h.iter().map(|s| s.cpu), top);
    let rt_line = line(h.iter().map(|s| s.rt), top);
    let ahead_line = line(h.iter().map(|s| s.ahead), top);
    let (graph_w, graph_h) = (GRAPH_W, GRAPH_H);
    let rows: Vec<TrackPerf> = f
        .as_ref()
        .map(|f| sorted(&f.tracks, sort()).into_iter().cloned().collect())
        .unwrap_or_default();
    rsx! {
        div {
            style: "position:absolute; inset:0; pointer-events:none; z-index:40;",
            div { style: "{CARD}",
                // Title and close.
                div { style: "display:flex; align-items:center; gap:8px;",
                    span { style: "font-weight:700; font-size:13px; flex:1; min-width:0;", "Performance meter" }
                    if let Some(f) = &f {
                        span { style: "color:{MUTED};", "{f.cores} cores" }
                    }
                    div {
                        style: "padding:0 6px; cursor:pointer; color:{MUTED}; font-size:14px;",
                        onclick: close,
                        "×"
                    }
                }
                // The last minute.
                div { style: "display:flex; flex-direction:column; gap:4px;",
                    div { style: "display:flex; gap:12px; color:{MUTED}; font-size:11px;",
                        span { "cur/avg: {pct(cpu.cur)}/{pct(cpu.avg)}  range: {pct(cpu.min)}–{pct(cpu.max)}" }
                        span { style: "flex:1;" }
                        span { style: "color:{CPU_INK};", "CPU" }
                        span { style: "color:{RT_INK};", "RT" }
                        span { style: "color:{AHEAD_INK};", "Ahead" }
                    }
                    div {
                        style: "position:relative; width:{graph_w}px; height:{graph_h}px; \
                                background:#101113; border:1px solid #2a2d33; border-radius:4px; overflow:hidden;",
                        svg {
                            width: "{graph_w}",
                            height: "{graph_h}",
                            view_box: "0 0 {graph_w} {graph_h}",
                            style: "position:absolute; left:0; top:0; width:{graph_w}px; height:{graph_h}px;",
                            polyline { points: "{ahead_line}", fill: "none", stroke: AHEAD_INK, stroke_width: "1.5" }
                            polyline { points: "{rt_line}", fill: "none", stroke: RT_INK, stroke_width: "1.5" }
                            polyline { points: "{cpu_line}", fill: "none", stroke: CPU_INK, stroke_width: "1.5" }
                        }
                        span {
                            style: "position:absolute; top:2px; left:4px; font-size:10px; color:#5d626b;",
                            "{top:.0}%"
                        }
                    }
                }
                match &f {
                    None => rsx! {
                        div { style: "color:{MUTED};", "Waiting for the engine…" }
                    },
                    Some(f) => rsx! {
                        // The totals.
                        div {
                            style: "display:grid; grid-template-columns:repeat(2, minmax(0, 1fr)); gap:2px 16px;",
                            Total { name: "CPU:", value: pct(f.cpu_pct) }
                            Total { name: "RT CPU:", value: format!("{} · ahead {}", pct(f.rt_cpu_pct), pct(f.ahead_cpu_pct)) }
                            Total { name: "RT longest block:", value: format!("{:.2} / {:.2} ms", f.rt_longest_ms, f.block_ms) }
                            Total { name: "Xruns:", value: format!("{} over · {} device", f.overruns, f.xruns) }
                            Total { name: "Gaps:", value: format!("{:.0} ms · {} short ahead", f.gap_ms, f.ahead_short) }
                            Total { name: "Disk:", value: format!("{}/s read · {}/s write", bytes(f.disk_read_bps), bytes(f.disk_write_bps)) }
                            Total { name: "RAM use:", value: bytes(f.ram_bytes as f64) }
                            Total { name: "System free:", value: format!("{} / {}", bytes(f.system_free_bytes as f64), bytes(f.system_total_bytes as f64)) }
                        }
                        // Every track.
                        div {
                            style: "display:flex; flex-direction:column; min-height:0; flex:1; \
                                    border:1px solid #2a2d33; border-radius:4px; overflow:hidden;",
                            div {
                                style: "display:flex; gap:8px; padding:4px 8px; background:#1f2227; {HEAD}",
                                Header { label: "Track", column: Column::Track, sort, width: "flex:1;", right: false }
                                Header { label: "FX CPU", column: Column::FxCpu, sort, width: "width:70px;", right: true }
                                Header { label: "# FX", column: Column::FxCount, sort, width: "width:40px;", right: true }
                                Header { label: "PDC", column: Column::Pdc, sort, width: "width:50px;", right: true }
                                Header { label: "Media CPU", column: Column::MediaCpu, sort, width: "width:80px;", right: true }
                            }
                            div { style: "overflow-y:scroll; min-height:0; max-height:420px;",
                                for t in rows {
                                    div {
                                        key: "{t.guid}",
                                        style: "display:flex; gap:8px; padding:2px 8px; border-top:1px solid #202328;",
                                        div { style: "{CELL} flex:1;", "{t.index + 1}: {t.name}" }
                                        div { style: "{CELL} width:70px; text-align:right;", "{pct(t.fx_cpu_pct)}" }
                                        div { style: "{CELL} width:40px; text-align:right;", "{t.fx_count}" }
                                        div { style: "{CELL} width:50px; text-align:right;", "{t.pdc_samples}" }
                                        div { style: "{CELL} width:80px; text-align:right;", "{pct(t.media_cpu_pct)}" }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use session_engine::view::TrackPerf;

    use super::{Column, Sort, Summary, sorted, summary};

    fn track(index: u32, fx: f64, media: f64, count: u32, pdc: u32) -> TrackPerf {
        TrackPerf {
            index,
            guid: format!("t{index}"),
            name: format!("Track {index}"),
            fx_cpu_pct: fx,
            media_cpu_pct: media,
            fx_count: count,
            pdc_samples: pdc,
        }
    }

    fn order(tracks: &[TrackPerf], sort: Sort) -> Vec<u32> {
        sorted(tracks, sort).into_iter().map(|t| t.index).collect()
    }

    #[test]
    fn the_costliest_fx_come_first_and_a_header_sorts_its_column_either_way() {
        let tracks = vec![
            track(0, 0.5, 3.0, 2, 0),
            track(1, 12.0, 0.1, 10, 64),
            track(2, 0.5, 9.0, 40, 240),
        ];
        // Default: FX CPU, most first; the tie in the song's order.
        assert_eq!(order(&tracks, Sort::default()), vec![1, 0, 2]);
        // Another number's header: its most first.
        let media = Sort::default().clicked(Column::MediaCpu);
        assert_eq!(order(&tracks, media), vec![2, 0, 1]);
        // The same header again: the other way.
        assert_eq!(
            order(&tracks, media.clicked(Column::MediaCpu)),
            vec![1, 0, 2]
        );
        // The tracks' header: the song's order.
        let by_track = media.clicked(Column::Track);
        assert!(!by_track.descending);
        assert_eq!(order(&tracks, by_track), vec![0, 1, 2]);
        assert_eq!(
            order(&tracks, Sort::default().clicked(Column::Pdc)),
            vec![2, 1, 0]
        );
    }

    #[test]
    fn a_series_reads_as_its_last_value_its_mean_and_its_range() {
        assert_eq!(summary([]), Summary::default());
        let s = summary([2.0, 6.0, 1.0, 3.0]);
        assert_eq!((s.cur, s.min, s.max), (3.0, 1.0, 6.0));
        assert!((s.avg - 3.0).abs() < 1e-12);
    }
}

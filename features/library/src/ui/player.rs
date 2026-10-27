//! The player: the library as a music app. Anything with a mix plays: a
//! session's newest version, a song through its session, an album or a
//! setlist as a queue. It keeps playing while the library is browsed (the
//! mini player under every page), opens to Now Playing (the artwork, the
//! waveform with the comments' marks, the transport), and from there to
//! the queue, the mix's comments (Review) and the full session.
//!
//! One clock for the whole library ([`use_clock`]): Review plays through
//! it too, so what is heard on a session's page and in the mini player is
//! the same thing.

use dioxus::prelude::*;
use lucide_dioxus::{
    Airplay, AudioLines, ChevronDown, FolderOpen, GripHorizontal, Layers, ListMusic, MessageSquare,
    Pause, Play, Repeat, Repeat1, RotateCcw, Share, Shuffle, SkipBack, SkipForward,
};

use super::kit::*;
use super::{Open, Route, Viewport, open, use_library, use_nav};
use crate::model::{Library, Listen, clock};

/// The playhead, while playing.
pub const PLAYHEAD: &str = "#a78bfa";

/// What happens at the end of a mix.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Again {
    /// On to the next; the queue stops at its end.
    Off,
    /// On to the next, and round from the start.
    All,
    /// The same mix again.
    One,
}

/// How much of the screen the player takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    /// The mini player under the page.
    Mini,
    /// Now Playing, over everything.
    Playing,
    /// Now Playing, turned to the queue (a phone's one column).
    Queue,
}

/// The player: what is queued, where in it, and how it plays.
#[derive(Clone, Copy)]
pub struct Player {
    pub queue: Signal<Vec<Listen>>,
    pub index: Signal<usize>,
    /// Where the playhead is in the current mix, in seconds.
    pub at: Signal<f32>,
    pub playing: Signal<bool>,
    pub again: Signal<Again>,
    pub shuffle: Signal<bool>,
    /// The queue's order before it was shuffled, to put back.
    unshuffled: Signal<Option<Vec<Listen>>>,
    pub sheet: Signal<Sheet>,
    /// What the queue was started from: "Paper Boats", "Sunday nights".
    pub from: Signal<String>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            queue: Signal::new(Vec::new()),
            index: Signal::new(0),
            at: Signal::new(0.0),
            playing: Signal::new(false),
            again: Signal::new(Again::Off),
            shuffle: Signal::new(false),
            unshuffled: Signal::new(None),
            sheet: Signal::new(Sheet::Mini),
            from: Signal::new(String::new()),
        }
    }

    /// What is playing (or paused), if anything is queued.
    pub fn current(self) -> Option<Listen> {
        self.queue.read().get(*self.index.read()).copied()
    }

    fn current_peek(self) -> Option<Listen> {
        self.queue.peek().get(*self.index.peek()).copied()
    }

    /// Play `queue` from `start`, said to come from `from`.
    pub fn play(mut self, queue: Vec<Listen>, start: usize, from: &str) {
        if queue.is_empty() {
            return;
        }
        self.index.set(start.min(queue.len() - 1));
        self.queue.set(queue);
        self.unshuffled.set(None);
        self.shuffle.set(false);
        self.at.set(0.0);
        self.from.set(from.to_owned());
        self.playing.set(true);
    }

    /// Hear `listen` from `at`: the current mix sought, the same session's
    /// other version swapped in at the same moment (A/B), anything else put
    /// in front of what was up next.
    pub fn cue(mut self, listen: Listen, at: f32, play: bool) {
        match self.current_peek() {
            Some(now) if now == listen => {}
            Some(now) if now.session == listen.session => {
                let i = *self.index.peek();
                self.queue.write()[i] = listen;
            }
            Some(_) => {
                let i = *self.index.peek() + 1;
                self.queue.write().insert(i, listen);
                self.index.set(i);
            }
            None => {
                self.queue.set(vec![listen]);
                self.index.set(0);
            }
        }
        self.at.set(at.max(0.0));
        if play {
            self.playing.set(true);
        }
    }

    pub fn toggle(mut self) {
        if self.current_peek().is_some() {
            let on = *self.playing.peek();
            self.playing.set(!on);
        }
    }

    pub fn seek(mut self, at: f32) {
        self.at.set(at.max(0.0));
    }

    /// Back `seconds`, not before the start.
    pub fn back(mut self, seconds: f32) {
        let at = *self.at.peek();
        self.at.set((at - seconds).max(0.0));
    }

    /// The next in the queue; round to the start when repeating, else the
    /// end stops it.
    pub fn next(mut self) {
        let len = self.queue.peek().len();
        let i = *self.index.peek();
        if i + 1 < len {
            self.index.set(i + 1);
        } else if *self.again.peek() == Again::All && len > 0 {
            self.index.set(0);
        } else {
            self.playing.set(false);
        }
        self.at.set(0.0);
    }

    /// The start of this mix, or (at its start already) the one before.
    pub fn previous(mut self) {
        let i = *self.index.peek();
        if *self.at.peek() < 3.0 && i > 0 {
            self.index.set(i - 1);
        }
        self.at.set(0.0);
    }

    /// The mix ran out.
    fn ended(mut self) {
        if *self.again.peek() == Again::One {
            self.at.set(0.0);
        } else {
            self.next();
        }
    }

    /// Play the queue's `i`th.
    pub fn jump(mut self, i: usize) {
        if i < self.queue.peek().len() {
            self.index.set(i);
            self.at.set(0.0);
            self.playing.set(true);
        }
    }

    /// Everything up next goes; what is playing stays.
    pub fn clear(mut self) {
        let i = *self.index.peek();
        self.queue.write().truncate(i + 1);
        self.unshuffled.set(None);
    }

    /// Move the queue's `from`th to `to` (both after what is playing).
    pub fn reorder(mut self, from: usize, to: usize) {
        let i = *self.index.peek();
        let len = self.queue.peek().len();
        if from <= i || to <= i || from >= len || to >= len || from == to {
            return;
        }
        let mut q = self.queue.write();
        let item = q.remove(from);
        q.insert(to, item);
    }

    /// Shuffle what is up next, or put it back in order.
    pub fn toggle_shuffle(mut self) {
        let on = !*self.shuffle.peek();
        self.shuffle.set(on);
        let i = *self.index.peek();
        if on {
            let before = self.queue.peek().clone();
            let mut q = before.clone();
            shuffle_after(&mut q, i);
            self.unshuffled.set(Some(before));
            self.queue.set(q);
        } else if let Some(order) = {
            let o = self.unshuffled.write().take();
            o
        } {
            // The original order, from what is playing on.
            let now = self.current_peek();
            let at = now.and_then(|n| order.iter().position(|l| *l == n));
            if let Some(at) = at {
                self.index.set(at);
                self.queue.set(order);
            }
        }
    }

    /// Repeat the queue, or not.
    pub fn toggle_repeat(mut self) {
        let a = *self.again.peek();
        self.again.set(if a == Again::All {
            Again::Off
        } else {
            Again::All
        });
    }

    /// Repeat this mix, or not.
    pub fn toggle_repeat_one(mut self) {
        let a = *self.again.peek();
        self.again.set(if a == Again::One {
            Again::Off
        } else {
            Again::One
        });
    }

    pub fn expand(mut self) {
        self.sheet.set(Sheet::Playing);
    }

    pub fn collapse(mut self) {
        self.sheet.set(Sheet::Mini);
    }
}

/// Shuffle `q` after index `i`, the same way every time for the same queue
/// (no clock, no thread rng: a screenshot is the same twice).
fn shuffle_after(q: &mut [Listen], i: usize) {
    let from = (i + 1).min(q.len());
    let tail = &mut q[from..];
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15 ^ tail.len() as u64;
    for k in (1..tail.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let j = (seed % (k as u64 + 1)) as usize;
        tail.swap(k, j);
    }
}

const fn play_label(playing: bool) -> &'static str {
    if playing { "Pause" } else { "Play" }
}

pub(crate) fn use_player() -> Player {
    use_context::<Player>()
}

/// The clock: a quarter-second tick while playing; the mix's end moves
/// the queue on.
pub(crate) fn use_clock(player: Player) {
    let lib = use_library();
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(250)).await;
            if !*player.playing.peek() {
                continue;
            }
            let Some(now) = player.current_peek() else {
                continue;
            };
            let length = lib.peek().mix(now).map_or(0.0, |(_, m)| m.seconds);
            let next = *player.at.peek() + 0.25;
            if next >= length {
                player.ended();
            } else {
                let mut at = player.at;
                at.set(next);
            }
        }
    });
}

/// What the player shows for a listen.
#[derive(Clone, PartialEq)]
struct Shown {
    listen: Listen,
    title: String,
    artist: String,
    colour: &'static str,
    note: String,
    seconds: f32,
    peaks: Vec<f32>,
    /// Comment times on this version (open ones), and whether a client's.
    marks: Vec<(f32, bool)>,
    comments: usize,
    latest: bool,
    versions: Vec<(u32, String, String)>,
}

fn shown(l: &Library, listen: Listen) -> Option<Shown> {
    let (session, mix) = l.mix(listen)?;
    let (title, artist, colour) = l.listen_label(listen)?;
    Some(Shown {
        listen,
        title,
        artist,
        colour,
        note: mix.note.clone(),
        seconds: mix.seconds,
        peaks: mix.peaks.clone(),
        marks: session
            .notes
            .iter()
            .filter(|n| n.version == mix.n && !n.resolved)
            .map(|n| (n.at, n.client))
            .collect(),
        comments: session.notes.iter().filter(|n| !n.resolved).count(),
        latest: session.mixes.last().is_some_and(|m| m.n == mix.n),
        versions: session
            .mixes
            .iter()
            .rev()
            .map(|m| (m.n, m.note.clone(), format!("{}, {}", m.by, m.when)))
            .collect(),
    })
}

/// A mix's artwork, until there is some: the song's colour, deepening, with
/// its title set large.
#[component]
pub(crate) fn Artwork(
    colour: &'static str,
    title: String,
    size: u32,
    #[props(default = 12)] radius: u32,
) -> Element {
    let big = size >= 120;
    let font = (size / 9).max(11);
    rsx! {
        div {
            style: "width: {size}px; height: {size}px; flex-shrink: 0; border-radius: {radius}px; overflow: hidden; position: relative; \
                    background: linear-gradient(145deg, {colour} 0%, {colour}99 45%, #18181b 100%); \
                    box-shadow: 0 1px 0 rgba(255,255,255,0.06) inset;",
            if big {
                span { style: "position: absolute; left: {size / 14}px; right: {size / 14}px; bottom: {size / 16}px; \
                               font-size: {font}px; font-weight: 800; letter-spacing: -0.02em; line-height: 1.05; color: rgba(255,255,255,0.92);",
                    "{title}"
                }
                span { style: "position: absolute; top: {size / 14}px; left: {size / 14}px; display: flex;",
                    AudioLines { size: (font + 4) as usize, color: "rgba(255,255,255,0.75)" }
                }
            } else {
                span { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; display: flex; align-items: center; justify-content: center;",
                    AudioLines { size: (size / 2) as usize, color: "rgba(255,255,255,0.8)" }
                }
            }
        }
    }
}

/// The waveform as a scrubber: the heard part bright, the playhead purple,
/// the comments' marks under it. A press on a bar seeks there.
#[component]
fn Scrubber(
    peaks: Vec<f32>,
    seconds: f32,
    at: f32,
    marks: Vec<(f32, bool)>,
    height: u32,
) -> Element {
    let player = use_player();
    let peaks = fewer(&peaks, 72);
    let bins = peaks.len().max(1);
    let played = ((at / seconds.max(0.1)) * bins as f32) as usize;
    let x = (at / seconds.max(0.1) * 100.0).clamp(0.0, 100.0);
    rsx! {
        div { style: "position: relative; width: 100%; padding-bottom: 12px;",
            div { style: "position: relative; height: {height}px; display: flex; align-items: center; gap: 2px;",
                for (i, p) in peaks.iter().enumerate() {
                    {
                        let h = (p * 100.0).max(6.0);
                        let c = if i < played { "rgba(255,255,255,0.92)" } else { "rgba(255,255,255,0.22)" };
                        let seek = i as f32 / bins as f32 * seconds;
                        rsx! {
                            span { key: "{i}", style: "flex: 1 1 0; min-width: 0; height: 100%; display: flex; align-items: center; cursor: pointer;",
                                onclick: move |_| player.seek(seek),
                                span { style: "width: 100%; height: {h}%; border-radius: 2px; background: {c};" }
                            }
                        }
                    }
                }
                span { style: "position: absolute; top: -6px; bottom: -6px; left: {x}%; width: 3px; margin-left: -1px; border-radius: 2px; background: {PLAYHEAD};" }
                span { style: "position: absolute; top: -11px; left: {x}%; width: 12px; height: 12px; margin-left: -6px; border-radius: 6px; background: {PLAYHEAD};" }
            }
            for (i, (t, client)) in marks.iter().copied().enumerate() {
                {
                    let left = (t / seconds.max(0.1) * 100.0).clamp(0.0, 100.0);
                    let c = if client { WARN } else { PICK };
                    rsx! {
                        span { key: "m{i}", style: "position: absolute; bottom: 0; left: {left}%; width: 6px; height: 6px; margin-left: -3px; border-radius: 3px; background: {c};" }
                    }
                }
            }
        }
    }
}

/// `peaks` as at most `n` bars, each the loudest of the bins it covers:
/// a phone's width cannot show 160.
fn fewer(peaks: &[f32], n: usize) -> Vec<f32> {
    if peaks.len() <= n || n == 0 {
        return peaks.to_vec();
    }
    (0..n)
        .map(|k| {
            let from = k * peaks.len() / n;
            let to = ((k + 1) * peaks.len() / n).max(from + 1);
            peaks[from..to].iter().copied().fold(0.0, f32::max)
        })
        .collect()
}

/// A round icon button.
#[component]
fn Round(
    label: &'static str,
    size: u32,
    #[props(default)] solid: bool,
    #[props(default)] on: bool,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    let bg = if solid {
        "#fafafa"
    } else if on {
        "rgba(167,139,250,0.22)"
    } else {
        "transparent"
    };
    rsx! {
        button { class: "lp", "aria-label": label, title: label,
            style: "width: {size}px; height: {size}px; border-radius: {size}px; border: none; background: {bg}; flex-shrink: 0; \
                    display: flex; align-items: center; justify-content: center; position: relative;",
            onclick: move |_| on_press.call(()),
            {children}
        }
    }
}

// ── The mini player ─────────────────────────────────────────────────────────

/// Under the page while something is queued: what it is, how far in, play
/// and next. A press on it opens Now Playing.
#[component]
pub(crate) fn MiniPlayer() -> Element {
    let player = use_player();
    let lib = use_library();
    let Some(now) = player.current() else {
        return rsx! {};
    };
    let Some(s) = shown(&lib.read(), now) else {
        return rsx! {};
    };
    let at = (player.at)();
    let x = (at / s.seconds.max(0.1) * 100.0).clamp(0.0, 100.0);
    let playing = (player.playing)();
    let artist = if s.artist.is_empty() {
        String::new()
    } else {
        format!(" · {}", s.artist)
    };
    rsx! {
        div { style: "flex-shrink: 0; position: relative; height: 68px; background: {SHEET}; border-top: 1px solid {RULE_STRONG}; \
                      display: flex; align-items: center; gap: 12px; padding: 0 12px;",
            span { style: "position: absolute; top: -1px; left: 0; width: {x}%; height: 2px; background: {PLAYHEAD};" }
            button { class: "lp", "aria-label": "Now playing",
                style: "flex: 1; min-width: 0; height: 60px; display: flex; align-items: center; justify-content: flex-start; gap: 12px; padding: 0 4px; border: none; \
                        border-radius: 10px; background: transparent; color: {INK}; font-family: {FONT}; text-align: left;",
                onclick: move |_| player.expand(),
                Artwork { colour: s.colour, title: s.title.clone(), size: 44, radius: 8 }
                div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                    span { style: "{BODY} {ONE_LINE}", "{s.title}" }
                    span { style: "{META} color: {INK_3}; {ONE_LINE}", "Mix v{now.version}{artist}" }
                }
            }
            span { style: "{META} color: {INK_3}; white-space: nowrap;", "{clock(at)}" }
            Round { label: play_label(playing), size: 48, on_press: move |()| player.toggle(),
                if playing { Pause { size: 24, color: INK, fill: INK } } else { Play { size: 24, color: INK, fill: INK } }
            }
            Round { label: "Next", size: 48, on_press: move |()| player.next(),
                SkipForward { size: 22, color: INK, fill: INK }
            }
        }
    }
}

// ── Now Playing ─────────────────────────────────────────────────────────────

/// Now Playing, over everything: sideways the player and the queue side by
/// side; upright and on a phone one column, turned to the queue by its
/// button.
#[component]
pub(crate) fn NowPlaying() -> Element {
    let player = use_player();
    let lib = use_library();
    let nav = use_nav();
    let Viewport(size) = use_context::<Viewport>();
    let sheet = (player.sheet)();
    if sheet == Sheet::Mini {
        return rsx! {};
    }
    let Some(now) = player.current() else {
        return rsx! {};
    };
    let Some(s) = shown(&lib.read(), now) else {
        return rsx! {};
    };
    let (w, h) = size();
    let wide = w >= 1000.0;
    let from = (player.from)();
    let session = now.session;
    let colour = s.colour;
    rsx! {
        div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 40; display: flex; flex-direction: column; \
                      background: linear-gradient(180deg, {colour}66 0%, #111114 55%, #0c0c0f 100%); background-color: {DESK}; color: {INK}; font-family: {FONT};",
            // Where it plays from, and the two ways on: down to the page, out
            // to the full session.
            div { style: "flex-shrink: 0; height: 64px; display: flex; align-items: center; gap: 12px; padding: 8px 16px 0;",
                Round { label: "Close", size: 44, on_press: move |()| player.collapse(),
                    ChevronDown { size: 26, color: INK }
                }
                div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: center; gap: 2px;",
                    if !from.is_empty() {
                        span { style: "{COLUMN} color: rgba(255,255,255,0.6);", "Playing from" }
                        span { style: "{META} font-weight: 700; {ONE_LINE}", "{from}" }
                    }
                }
                button { class: "lp", "aria-label": "Open in Session", title: "Open in Session",
                    style: "height: 40px; padding: 0 14px; border-radius: 20px; border: 1px solid rgba(255,255,255,0.18); background: rgba(255,255,255,0.06); \
                            color: {INK}; font-family: {FONT}; font-size: 14px; font-weight: 650; display: flex; align-items: center; gap: 8px;",
                    onclick: move |_| open(Open::Session(session)),
                    FolderOpen { size: 16, color: INK }
                    if w >= 600.0 { "Open session" }
                }
            }
            if wide {
                div { style: "flex: 1; min-height: 0; display: flex; gap: 48px; padding: 16px 48px 32px;",
                    div { style: "flex: 1 1 0; min-width: 0; min-height: 0; overflow-y: scroll; display: flex; justify-content: center;",
                        Deck { s: s.clone(), art: ((h - 500.0).clamp(220.0, 440.0)) as u32, nav_to_review: move |()| { player.collapse(); nav.go(Route::Session(session)); } }
                    }
                    div { style: "flex: 0 0 400px; min-height: 0; display: flex; flex-direction: column; border-left: 1px solid rgba(255,255,255,0.08); padding-left: 32px;",
                        QueueList { header: false }
                    }
                }
            } else if sheet == Sheet::Queue {
                div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; width: 100%; max-width: 560px; margin: 0 auto; padding: 8px 20px 20px;",
                    QueueList { header: true }
                }
            } else {
                div { style: "flex: 1; min-height: 0; overflow-y: scroll; padding: 8px 24px 24px; display: flex; justify-content: center;",
                    Deck { s: s.clone(), art: ((w - 48.0).min(h - 520.0)).clamp(200.0, 640.0) as u32, nav_to_review: move |()| { player.collapse(); nav.go(Route::Session(session)); } }
                }
            }
        }
    }
}

/// The player itself: artwork, what it is, the scrubber, the transport,
/// and the row of ways out (AirPlay, the comments, share).
#[component]
fn Deck(s: Shown, art: u32, nav_to_review: EventHandler<()>) -> Element {
    let player = use_player();
    let mut versions = use_signal(|| false);
    let at = (player.at)();
    let playing = (player.playing)();
    let sheet = (player.sheet)();
    let left = (s.seconds - at).max(0.0);
    let listen = s.listen;
    let comments = s.comments;
    let comment_word = if comments == 1 { "comment" } else { "comments" };
    let artist = if s.artist.is_empty() {
        "No artist".to_owned()
    } else {
        s.artist.clone()
    };
    rsx! {
        div { style: "width: 100%; max-width: {art.max(360)}px; display: flex; flex-direction: column; align-items: stretch; gap: 20px;",
            div { style: "display: flex; justify-content: center; padding-top: 8px;",
                Artwork { colour: s.colour, title: s.title.clone(), size: art, radius: 16 }
            }
            // What it is, and its versions behind the layers button.
            div { style: "position: relative; display: flex; align-items: center; gap: 12px;",
                div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                    span { style: "font-size: 22px; font-weight: 750; letter-spacing: -0.01em; {ONE_LINE}", "{s.title}" }
                    span { style: "font-size: 16px; color: rgba(255,255,255,0.65); {ONE_LINE}", "{artist}" }
                    span { style: "{META} color: rgba(255,255,255,0.5); {ONE_LINE}",
                        "Mix v{listen.version} · {s.note}"
                        if s.latest { " · Latest" }
                    }
                }
                Round { label: "Versions", size: 44, on: versions(), on_press: move |()| versions.toggle(),
                    Layers { size: 22, color: INK }
                }
                if versions() {
                    div { style: "position: absolute; top: 56px; right: 0; width: 320px; max-height: 320px; overflow-y: scroll; z-index: 5; \
                                  border-radius: 14px; background: #1c1c21; border: 1px solid {RULE_STRONG}; padding: 6px; \
                                  box-shadow: 0 16px 40px rgba(0,0,0,0.5);",
                        span { style: "display: block; {COLUMN} color: {INK_3}; padding: 8px 10px 6px;", "Versions" }
                        for (n, note, by) in s.versions.clone() {
                            {
                                let on = n == listen.version;
                                let bg = if on { "rgba(167,139,250,0.18)" } else { "transparent" };
                                rsx! {
                                    button { key: "{n}", class: "lp",
                                        style: "width: 100%; min-height: 52px; display: flex; align-items: center; justify-content: flex-start; gap: 12px; padding: 6px 10px; border: none; \
                                                border-radius: 10px; background: {bg}; color: {INK}; font-family: {FONT}; text-align: left;",
                                        onclick: move |_| {
                                            // The same moment of the other version: A/B.
                                            let t = *player.at.peek();
                                            player.cue(crate::model::Listen { session: listen.session, version: n }, t, *player.playing.peek());
                                            versions.set(false);
                                        },
                                        span { style: "width: 32px; font-size: 15px; font-weight: 750;", "v{n}" }
                                        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                                            span { style: "font-size: 14px; font-weight: 600; {ONE_LINE}", "{note}" }
                                            span { style: "font-size: 12px; color: {INK_3}; {ONE_LINE}", "{by}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; gap: 6px;",
                Scrubber { peaks: s.peaks.clone(), seconds: s.seconds, at, marks: s.marks.clone(), height: 56 }
                div { style: "display: flex; align-items: center; justify-content: space-between; font-size: 12px; font-weight: 600; color: rgba(255,255,255,0.6);",
                    span { style: "width: 64px;", "{clock(at)}" }
                    span { style: "display: flex; align-items: center; gap: 5px; height: 20px; padding: 0 8px; border-radius: 5px; \
                                   background: rgba(255,255,255,0.1); font-size: 11px; font-weight: 700; color: rgba(255,255,255,0.75);",
                        AudioLines { size: 12, color: "rgba(255,255,255,0.75)" }
                        "Lossless"
                    }
                    span { style: "width: 64px; text-align: right;", "-{clock(left)}" }
                }
            }
            // The transport.
            div { style: "display: flex; align-items: center; justify-content: space-between; padding: 0 4px;",
                Round { label: "Back 15 seconds", size: 52, on_press: move |()| player.back(15.0),
                    RotateCcw { size: 28, color: INK }
                    span { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; \
                                   padding-top: 2px; font-size: 9px; font-weight: 800; color: {INK};", "15" }
                }
                Round { label: "Previous", size: 56, on_press: move |()| player.previous(),
                    SkipBack { size: 30, color: INK, fill: INK }
                }
                Round { label: play_label(playing), size: 76, solid: true, on_press: move |()| player.toggle(),
                    if playing { Pause { size: 34, color: "#0c0c0f", fill: "#0c0c0f" } } else { Play { size: 34, color: "#0c0c0f", fill: "#0c0c0f" } }
                }
                Round { label: "Next", size: 56, on_press: move |()| player.next(),
                    SkipForward { size: 30, color: INK, fill: INK }
                }
                Round { label: "Queue", size: 52, on: sheet == Sheet::Queue,
                    on_press: move |()| { let mut s = player.sheet; s.set(if *s.peek() == Sheet::Queue { Sheet::Playing } else { Sheet::Queue }); },
                    ListMusic { size: 26, color: INK }
                }
            }
            // The ways out: where it plays, what was said, who hears it.
            div { style: "display: flex; align-items: center; justify-content: space-between; padding: 4px 4px 0;",
                Round { label: "AirPlay", size: 48, on_press: move |()| {},
                    Airplay { size: 22, color: "rgba(255,255,255,0.75)" }
                }
                button { class: "lp", "aria-label": "Comments",
                    style: "height: 40px; padding: 0 16px; border-radius: 20px; border: none; background: rgba(255,255,255,0.1); \
                            color: {INK}; font-family: {FONT}; font-size: 14px; font-weight: 650; display: flex; align-items: center; gap: 8px;",
                    onclick: move |_| nav_to_review.call(()),
                    MessageSquare { size: 16, color: INK }
                    if comments == 0 { "Comment" } else { "{comments} {comment_word}" }
                }
                Round { label: "Share", size: 48, on_press: move |()| {},
                    Share { size: 22, color: "rgba(255,255,255,0.75)" }
                }
            }
        }
    }
}

// ── The queue ───────────────────────────────────────────────────────────────

/// The queue: (on a phone) what is playing, then how it plays (shuffle,
/// repeat, repeat one), then what is up next, each with a handle to drag
/// it into place.
#[component]
fn QueueList(header: bool) -> Element {
    let player = use_player();
    let lib = use_library();
    let mut dragging = use_signal(|| None::<usize>);
    let l = lib.read();
    let queue = (player.queue)();
    let index = (player.index)();
    let again = (player.again)();
    let shuffle = (player.shuffle)();
    let playing = (player.playing)();
    let current = queue.get(index).and_then(|n| shown(&l, *n));
    let next: Vec<(usize, Shown)> = queue
        .iter()
        .enumerate()
        .skip(index + 1)
        .filter_map(|(i, n)| Some((i, shown(&l, *n)?)))
        .collect();
    let from = (player.from)();
    let seg = |on: bool| {
        let (bg, ink) = if on {
            ("rgba(167,139,250,0.28)", INK)
        } else {
            ("transparent", "rgba(255,255,255,0.7)")
        };
        (
            format!(
                "flex: 1 1 0; height: 40px; border: none; border-radius: 9px; background: {bg}; display: flex; align-items: center; justify-content: center;"
            ),
            ink,
        )
    };
    let (shuffle_style, shuffle_ink) = seg(shuffle);
    let (repeat_style, repeat_ink) = seg(again == Again::All);
    let (one_style, one_ink) = seg(again == Again::One);
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 16px;",
            onmouseup: move |_| dragging.set(None),
            onmouseleave: move |_| dragging.set(None),
            if header {
                if let Some(c) = current.clone() {
                    div { style: "display: flex; align-items: center; gap: 14px;",
                        Artwork { colour: c.colour, title: c.title.clone(), size: 64, radius: 10 }
                        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                            span { style: "font-size: 17px; font-weight: 700; {ONE_LINE}", "{c.title}" }
                            span { style: "{META} color: rgba(255,255,255,0.6); {ONE_LINE}", "{c.artist} · Mix v{c.listen.version}" }
                        }
                        Round { label: play_label(playing), size: 48, on_press: move |()| player.toggle(),
                            if playing { Pause { size: 24, color: INK, fill: INK } } else { Play { size: 24, color: INK, fill: INK } }
                        }
                    }
                }
            } else {
                span { style: "{HEADING}", "Queue" }
            }
            div { style: "display: flex; gap: 4px; padding: 4px; border-radius: 12px; background: rgba(255,255,255,0.08);",
                button { class: "lp", "aria-label": "Shuffle", title: "Shuffle", style: "{shuffle_style}", onclick: move |_| player.toggle_shuffle(),
                    Shuffle { size: 20, color: shuffle_ink } }
                button { class: "lp", "aria-label": "Repeat", title: "Repeat", style: "{repeat_style}", onclick: move |_| player.toggle_repeat(),
                    Repeat { size: 20, color: repeat_ink } }
                button { class: "lp", "aria-label": "Repeat one", title: "Repeat one", style: "{one_style}", onclick: move |_| player.toggle_repeat_one(),
                    Repeat1 { size: 20, color: one_ink } }
            }
            div { style: "display: flex; align-items: baseline; gap: 8px;",
                div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                    span { style: "font-size: 17px; font-weight: 750;", "Up next" }
                    if !from.is_empty() { span { style: "{META} color: rgba(255,255,255,0.55); {ONE_LINE}", "From {from}" } }
                }
                if !next.is_empty() {
                    button { class: "lp",
                        style: "height: 32px; padding: 0 12px; border-radius: 16px; border: none; background: rgba(255,255,255,0.1); \
                                color: {INK}; font-family: {FONT}; font-size: 13px; font-weight: 650;",
                        onclick: move |_| player.clear(), "Clear" }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: scroll; margin: 0 -8px;",
                if next.is_empty() {
                    p { style: "{META} color: rgba(255,255,255,0.55); padding: 8px; line-height: 1.5;",
                        "Nothing up next. Play an album, a setlist or a song and it queues here."
                    }
                }
                for (i, n) in next {
                    {
                        let lifted = dragging() == Some(i);
                        let bg = if lifted { "rgba(255,255,255,0.1)" } else { "transparent" };
                        rsx! {
                            div { key: "{i}-{n.listen.session.0}",
                                style: "height: 64px; display: flex; align-items: center; gap: 12px; padding: 0 8px; border-radius: 10px; background: {bg};",
                                onmousemove: move |_| {
                                    let held = *dragging.peek();
                                    if let Some(d) = held && d != i {
                                        player.reorder(d, i);
                                        dragging.set(Some(i));
                                    }
                                },
                                button { class: "lp", "aria-label": "Play {n.title}",
                                    style: "flex: 1; min-width: 0; height: 60px; display: flex; align-items: center; justify-content: flex-start; gap: 12px; padding: 0; border: none; \
                                            background: transparent; color: {INK}; font-family: {FONT}; text-align: left; border-radius: 8px;",
                                    onclick: move |_| player.jump(i),
                                    Artwork { colour: n.colour, title: n.title.clone(), size: 44, radius: 6 }
                                    div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                        span { style: "font-size: 15px; font-weight: 600; {ONE_LINE}", "{n.title}" }
                                        span { style: "{META} color: rgba(255,255,255,0.55); {ONE_LINE}", "{n.artist} · Mix v{n.listen.version}" }
                                    }
                                }
                                span { "aria-label": "Drag to reorder", title: "Drag to reorder",
                                    style: "width: 44px; height: 44px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; cursor: grab;",
                                    onmousedown: move |e| { e.stop_propagation(); dragging.set(Some(i)); },
                                    GripHorizontal { size: 22, color: "rgba(255,255,255,0.5)" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionId;

    fn l(n: u128) -> Listen {
        Listen {
            session: SessionId(uuid::Uuid::from_u128(n)),
            version: 1,
        }
    }

    #[test]
    fn a_shuffle_keeps_what_is_playing_and_every_song() {
        let mut q: Vec<Listen> = (1..=8).map(l).collect();
        shuffle_after(&mut q, 2);
        assert_eq!(&q[..3], &[l(1), l(2), l(3)], "played and playing stay put");
        let mut rest: Vec<u128> = q[3..].iter().map(|x| x.session.0.as_u128()).collect();
        assert_ne!(rest, vec![4, 5, 6, 7, 8], "up next moved");
        rest.sort_unstable();
        assert_eq!(rest, vec![4, 5, 6, 7, 8], "nothing lost");
    }
}

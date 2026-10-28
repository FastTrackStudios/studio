//! A page of bars, as the window steps through the song (`SESSION_STEP`,
//! `[` / `]`): which bars it holds, where they start and end, which tracks
//! it frames — worked out on the engine (`SESSION_PAGE`), which has the
//! song's tempo map, its markers and its tracks; the window frames what it
//! says.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Markers, ProjectContext, TempoMap as _, Tracks};
use facet::Facet;

/// A little of the bars either side of a page is shown too: a hit on its
/// first downbeat, or the next page's, is seen whole.
const EDGE_OF_A_BAR: f64 = 0.25;

static PAGE: ActionMeta = crate::actions::meta(
    "SESSION_PAGE",
    "session_page",
    "A page of bars",
    "The page of `bars` bars that `SESSION_STEP` frames: the one holding `bar`, or the moment `at` (seconds), or `by` pages on from `first` (or from the page the edit cursor is in), kept inside the song; and the tracks of `group` (the visibility manager's, empty for every track) it frames. While stopped the edit cursor goes to its start. What a window's step asks the engine; it frames what this says.",
    "View",
    "",
);

/// Which page.
#[derive(Facet)]
struct PageArg {
    bars: i32,
    group: String,
    by: Option<i32>,
    bar: Option<i32>,
    at: Option<f64>,
    /// The page the window is on, and whether this step resized it.
    first: Option<i32>,
    resized: Option<bool>,
}

/// The page.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct PageOut {
    pub first_bar: i32,
    pub last_bar: i32,
    pub bars: i32,
    pub group: String,
    /// Seconds.
    pub from: f64,
    pub to: f64,
    /// What is shown of it, with a little either side.
    pub shown_from: f64,
    pub shown_to: f64,
    /// The tracks framed down the view, by GUID (none: every track).
    pub tracks: Vec<String>,
}

/// `bar` moved on by `n` bars as the song counts them: no bar 0.
const fn add_bars(bar: i32, n: i32) -> i32 {
    let moved = bar.saturating_add(n);
    if bar < 0 && moved >= 0 {
        moved.saturating_add(1)
    } else if bar > 0 && moved <= 0 {
        moved.saturating_sub(1)
    } else {
        moved
    }
}

/// The first bar of the page of `bars` holding `bar`, pages counted from
/// bar 1 (bars before it, the count-in, page backwards from −1).
const fn page_of(bar: i32, bars: i32) -> i32 {
    let bars = if bars < 1 { 1 } else { bars };
    // How many whole pages before it, counted from bar 1 (or, before the
    // song, back from −1).
    let into = if bar > 0 {
        bar.saturating_sub(1)
    } else {
        bar.saturating_neg().saturating_sub(1)
    };
    let pages = match into.checked_div(bars) {
        Some(pages) => pages,
        None => 0,
    };
    let start = pages.saturating_mul(bars).saturating_add(1);
    if bar > 0 {
        start
    } else {
        // Mirrored: the count-in's pages run back from −1.
        start
            .saturating_neg()
            .saturating_sub(bars.saturating_sub(1))
    }
}

/// The song's last bar: the one its `SONGEND` falls in (just before it,
/// when it is on a bar line), `None` without one.
fn last_bar(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Option<i32> {
    let end = Markers::all(daw, project.clone())
        .into_iter()
        .find(|m| m.name == "SONGEND")
        .and_then(|m| m.position.seconds())?;
    Some(crate::song_bars::musical(daw, project, (end - 1e-3).max(0.0)).0)
}

/// The tracks a page frames down the view: the group's tracks, kept to the
/// top-level folder holding most of them (the kit, not the references'
/// rehearsal drums) and everything under it. Empty for every track.
fn framed(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    group: &str,
) -> Result<Vec<String>, String> {
    if group.is_empty() {
        return Ok(Vec::new());
    }
    let tracks = Tracks::all(daw, project.clone());
    let names: Vec<String> = tracks.iter().map(|t| t.name.clone()).collect();
    let groups = dynamic_template::visibility::groups(names).map_err(|e| e.to_string())?;
    let key = dynamic_template::visibility::normalize_key(group);
    let members = groups
        .get(&key)
        .filter(|m| !m.is_empty())
        .ok_or_else(|| format!("no tracks in the {group} group"))?;
    let top_of = |guid: &str| -> String {
        let mut at = guid.to_owned();
        while let Some(parent) = tracks
            .iter()
            .find(|t| t.guid == at)
            .and_then(|t| t.parent_guid.clone())
        {
            at = parent;
        }
        at
    };
    let mut counts: Vec<(String, usize)> = Vec::new();
    for track in tracks.iter().filter(|t| members.contains(&t.name)) {
        let top = top_of(&track.guid);
        match counts.iter_mut().find(|(t, _)| *t == top) {
            Some((_, n)) => *n = n.saturating_add(1),
            None => counts.push((top, 1)),
        }
    }
    let (top, _) = counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .ok_or_else(|| format!("no tracks in the {group} group"))?;
    Ok(tracks
        .iter()
        .filter(|t| top_of(&t.guid) == top)
        .map(|t| t.guid.clone())
        .collect())
}

static SPAN: ActionMeta = crate::actions::meta(
    "SESSION_SPAN",
    "session_span",
    "What a view frames",
    "The span and tracks `SESSION_VIEW` frames, as the song has them: `from`/`to` seconds, `bars` [first, last] (as the song counts them), or `section` (its `nth`, from 1) — and `tracks` (paths, names or GUIDs) resolved to GUIDs. Read-only.",
    "View",
    "",
);

/// What a view frames, worked out.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct SpanOut {
    /// Seconds.
    pub span: Option<(f64, f64)>,
    pub tracks: Vec<String>,
}

/// What to look at.
#[derive(Facet)]
pub struct ViewArg {
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub bars: Option<Vec<i32>>,
    pub section: Option<String>,
    /// Which of the sections of that name, from 1 (1).
    pub nth: Option<u32>,
    pub tracks: Option<Vec<String>>,
}

/// The span `a` asks for, if it asks for one.
pub fn view_span(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
    a: &ViewArg,
) -> Result<Option<(f64, f64)>, String> {
    if let Some(name) = &a.section {
        let all = crate::actions::sections(daw, project);
        let named: Vec<&crate::actions::SectionOut> = all
            .iter()
            .filter(|s| s.name.trim().eq_ignore_ascii_case(name.trim()))
            .collect();
        let nth = usize::try_from(a.nth.unwrap_or(1).max(1)).unwrap_or(1);
        return named
            .get(nth.saturating_sub(1))
            .map(|s| Some((s.start, s.end)))
            .ok_or_else(|| {
                let mut known: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
                known.dedup();
                format!("no section {name} (#{nth}): the song has {known:?}")
            });
    }
    if let Some(bars) = &a.bars {
        let (Some(&first), Some(&last)) = (bars.first(), bars.last()) else {
            return Err("`bars` is [first, last]".to_owned());
        };
        // Bars as the song counts them; the bar after the last found in
        // the engine's, where there is no bar 0 to step over.
        let from = crate::song_bars::time(daw, &project, first.min(last), 1, 0.0);
        let after = crate::song_bars::measure(daw, &project, first.max(last)).saturating_add(1);
        let to = daw.musical_to_time(project.clone(), after, 1, 0.0);
        return Ok(Some((from, to)));
    }
    match (a.from, a.to) {
        (Some(from), Some(to)) if to > from => Ok(Some((from.max(0.0), to))),
        (Some(_), Some(_)) => Err("`to` must be after `from`".to_owned()),
        (None, None) => Ok(None),
        _ => Err("a span is `from` and `to` together".to_owned()),
    }
}

/// `SESSION_PAGE` onto `list`.
///
/// # Errors
///
/// The list refused it (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SPAN, Effect::ReadOnly, |a: ViewArg| {
        let (daw, project) = crate::actions::song()?;
        let span = view_span(daw, &project, &a)?;
        let tracks = a
            .tracks
            .iter()
            .flatten()
            .map(|t| crate::actions::resolve(daw, &project, t))
            .collect::<Result<Vec<String>, String>>()?;
        Ok(SpanOut { span, tracks })
    })?;
    list.register(&PAGE, Effect::Mutating, |a: PageArg| {
        let (daw, project) = crate::actions::song()?;
        let bars = a.bars.max(1);
        let tracks = framed(daw, &project, &a.group)?;
        let by = a.by.unwrap_or(0);
        let first = if let Some(bar) = a.bar {
            page_of(bar, bars)
        } else if let Some(at) = a.at {
            page_of(crate::song_bars::musical(daw, &project, at).0, bars)
        } else if let (Some(first), true, false) = (a.first, by != 0, a.resized.unwrap_or(false)) {
            add_bars(page_of(first, bars), by.saturating_mul(bars))
        } else {
            let at = crate::edit_state::current()
                .unwrap_or_else(|| crate::organize_edit::engine_edit(daw, &project))
                .at;
            let here = page_of(crate::song_bars::musical(daw, &project, at).0, bars);
            add_bars(here, by.saturating_mul(bars))
        };
        // Not past the song's either end.
        let lowest = page_of(crate::song_bars::musical(daw, &project, 0.0).0, bars);
        let mut first = first.max(lowest);
        if let Some(end_bar) = last_bar(daw, &project) {
            first = first.min(page_of(end_bar, bars));
        }
        let after = add_bars(first, bars);
        let closing = add_bars(after, -1);
        let from = crate::song_bars::time(daw, &project, first, 1, 0.0);
        let to = crate::song_bars::time(daw, &project, after, 1, 0.0);
        let playing =
            crate::stream_set::transport_of(&crate::open::current_song().unwrap_or_default())
                .is_some_and(|(_, playing)| playing);
        if !playing {
            daw::service::transport::service::Transport::set_position(daw, project, from)
                .map_err(|e| e.to_string())?;
            crate::edit_state::ask(crate::edit_state::Edit {
                at: from,
                selection: None,
            });
        }
        let margin = (to - from) / f64::from(bars) * EDGE_OF_A_BAR;
        Ok(PageOut {
            first_bar: first,
            last_bar: closing,
            bars,
            group: a.group,
            from,
            to,
            shown_from: (from - margin).max(0.0),
            shown_to: to + margin,
            tracks,
        })
    })?;
    Ok(())
}

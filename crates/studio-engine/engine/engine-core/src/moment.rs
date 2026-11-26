//! Where a moment of the song is, as an action names it — a bar, a time or
//! a section ([`moment`]) — the song's sections ([`sections`]) and the
//! song's guid ([`project_guid`]). From engine-mix's `fx_actions`, shared
//! by the mix, edit and listen.

use daw::service::{ProjectContext, TempoMap as _};
use facet::Facet;

/// One section.
#[derive(Facet)]
pub struct SectionOut {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub first_bar: i32,
    pub last_bar: i32,
}

/// The song's sections, in order.
pub fn sections(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<SectionOut> {
    let mut all: Vec<SectionOut> =
        <daw::standalone::Standalone as daw::service::region::Regions>::all(daw, project.clone())
            .into_iter()
            // The sections lane's (a region on no lane is on it): not the
            // song's own region, nor an edit zone on the Drums lane.
            .filter(|r| {
                r.lane.is_none_or(|l| {
                    l == engine_domain::ruler_lanes::CoreLane::Sections.lane_index()
                })
            })
            .map(|r| {
                let (start, end) = (r.time_range.start_seconds(), r.time_range.end_seconds());
                SectionOut {
                    name: r.name,
                    start,
                    end,
                    first_bar: crate::song_bars::musical(daw, &project, start + 1e-6).0,
                    last_bar: crate::song_bars::musical(daw, &project, (end - 1e-6).max(start)).0,
                }
            })
            .collect();
    all.sort_by(|a, b| a.start.total_cmp(&b.start));
    all
}

/// Where `text` is, seconds: a bar (`33`, `33:2`, `33:2.5` — beat 2 and a
/// half), a time (`12.5s`), or a section by name (`Chorus 1`; its end with
/// ` end`).
pub fn moment(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    text: &str,
) -> Result<f64, String> {
    let text = text.trim();
    if let Some(secs) = text.strip_suffix('s')
        && let Ok(secs) = secs.trim().parse::<f64>()
    {
        return Ok(secs.max(0.0));
    }
    let (bar, beat) = text.split_once(':').unwrap_or((text, "1"));
    if let (Ok(bar), Ok(beat)) = (bar.trim().parse::<i32>(), beat.trim().parse::<f64>()) {
        use num_traits::ToPrimitive as _;
        let whole = beat.floor().max(1.0);
        let into = beat - whole;
        let beat = whole.to_i32().ok_or_else(|| format!("no beat {whole}"))?;
        // The beat's start, then the fraction of it on.
        // The bar as the song counts it.
        let bar = crate::song_bars::measure(daw, &project, bar);
        let start = daw.musical_to_time(project.clone(), bar, beat, 0.0);
        let next = daw.musical_to_time(project.clone(), bar, beat.saturating_add(1), 0.0);
        return Ok(into.mul_add(next - start, start));
    }
    let (name, end) = text
        .strip_suffix(" end")
        .map_or((text, false), |name| (name, true));
    let sections = sections(daw, project);
    sections
        .iter()
        .find(|s| s.name.trim().eq_ignore_ascii_case(name.trim()))
        .map(|s| if end { s.end } else { s.start })
        .ok_or_else(|| {
            format!(
                "{text:?} is not a bar (`33:2`), a time (`12.5s`) or a section — the sections are {}",
                sections
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The song's guid in `project`.
pub fn project_guid(project: &ProjectContext) -> Result<String, String> {
    match project {
        ProjectContext::Project(guid) => Ok(guid.clone()),
        ProjectContext::Current => Err("no song open".to_owned()),
    }
}

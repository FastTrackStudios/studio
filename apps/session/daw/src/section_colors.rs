//! A song's sections in keyflow's colours: each region on the Sections
//! lane coloured by the kind of section its name says it is (`VS 1` a
//! verse, `PRE-CH 2` a pre-chorus) — the colours a chart engraves its
//! sections in, so the ruler and the chart read alike.

use daw::service::{ProjectContext, Regions};

/// Colour `project`'s sections by kind; a region whose name names no
/// kind, or that has its colour already, is left as it is. Returns how
/// many were coloured.
///
/// # Errors
/// The engine refused a colour.
pub fn apply<D: Regions>(daw: &D, project: &ProjectContext) -> daw_proto::DawResult<usize> {
    colour(daw, project, true)
}

/// How many of `project`'s sections [`apply`] would colour.
#[must_use]
pub fn pending<D: Regions>(daw: &D, project: &ProjectContext) -> usize {
    colour(daw, project, false).unwrap_or(0)
}

fn colour<D: Regions>(daw: &D, project: &ProjectContext, set: bool) -> daw_proto::DawResult<usize> {
    let sections = session::ruler_lanes::CoreLane::Sections.lane_index();
    let mut coloured = 0_usize;
    for region in Regions::all(daw, project.clone()) {
        if region.lane.is_some_and(|lane| lane != sections) {
            continue;
        }
        let (Some(id), Some(kind)) = (
            region.id,
            session::keyflow::actions::parse_region_section_type(&region.name),
        ) else {
            continue;
        };
        let colour = session::keyflow::actions::section_type_color(&kind);
        if region.color != Some(rgb_of_native(colour)) {
            if set {
                Regions::set_color(daw, project.clone(), id, colour)?;
            }
            coloured = coloured.saturating_add(1);
        }
    }
    Ok(coloured)
}

/// REAPER's native `0x01BBGGRR` as the `0xRRGGBB` an engine reports a
/// region's colour in.
const fn rgb_of_native(native: u32) -> u32 {
    let (red, green, blue) = (native & 0xFF, (native >> 8) & 0xFF, (native >> 16) & 0xFF);
    (red << 16) | (green << 8) | blue
}

//! The engraver as a chart is drawn here: the pipeline `kf` draws with,
//! a chart laid out as it is printed, and the engraver's own fonts for
//! drawing its pages without it. Shared by the PNG/PDF engraving
//! ([`crate::chart`], native) and the chart that follows its recording
//! ([`crate::follow`], native and wasm).

use std::sync::{Arc, OnceLock};

use keyflow::engraver::api::pipeline::ChartPipeline;
use keyflow::engraver::layout::chart::{ChartLayoutConfig, ChartLayoutResult, LayoutMode};
use keyflow::engraver::style::MStyle;

use crate::{Error, Result};

/// The pipeline `kf` draws with: the shared fonts, `MStyle::new()`.
fn pipeline() -> Result<&'static ChartPipeline> {
    static PIPELINE: OnceLock<std::result::Result<ChartPipeline, String>> = OnceLock::new();
    PIPELINE
        .get_or_init(|| {
            let style: &'static MStyle = Box::leak(Box::new(MStyle::new()));
            ChartPipeline::with_style(style).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| Error::Engrave(e.clone()))
}

/// Chart `chart` laid out as it is printed: A4 pages, the Master Rhythm
/// preset. The pipeline it was laid out with comes along.
///
/// # Errors
///
/// The engraver's fonts did not load.
pub fn layout(chart: &keyflow::Chart) -> Result<(&'static ChartPipeline, ChartLayoutResult)> {
    let pipeline = pipeline()?;
    let config = ChartLayoutConfig::master_rhythm().with_page_offsets(true);
    Ok((
        pipeline,
        pipeline.layout_with_config(chart, &LayoutMode::paginated_a4(), &config),
    ))
}

/// The engraver's own fonts, for drawing its pages without it: as pixels
/// (the PNG pages) or as outlines (the share page's chart, [`crate::follow`]).
///
/// # Errors
///
/// The engraver's fonts did not load.
pub fn fontdb() -> Result<Arc<usvg::fontdb::Database>> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let pipeline = pipeline()?;
    Ok(Arc::clone(FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        for (label, bytes) in pipeline.fonts().embeddable_fonts_for_raster() {
            db.load_font_data(bytes.to_vec());
            // The generic families the engraving names (a rehearsal mark's
            // `sans-serif`) are the face the engraver files under them:
            // fontdb maps a generic family by name only.
            let family = db
                .faces()
                .last()
                .and_then(|f| f.families.first())
                .map(|(name, _)| name.clone());
            match (label, family) {
                ("sans-serif", Some(name)) => db.set_sans_serif_family(name),
                ("serif", Some(name)) => db.set_serif_family(name),
                _ => {}
            }
        }
        Arc::new(db)
    })))
}

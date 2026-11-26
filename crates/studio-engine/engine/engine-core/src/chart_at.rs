//! Where a song's chart is at a time: the bar as played, the written bar
//! it plays and on which pass, and its section — what a chart following
//! the playhead shows (`SESSION_CHART_AT`).
//!
//! The chart's clock is the played one (`keyflow::text::bar_times`): a
//! repeat's bars come round again on their second pass, and the bars
//! after it come later by as much, the way the song was built from the
//! chart (`chart_import::chart_to_layout`, from zero, its count-in first).
//! A one-pass clock (each written bar once) falls behind the music from
//! the first repeat on and has no bar past the written end (studio#450).

/// The chart's bar at a time.
#[derive(Clone, Debug, PartialEq, facet::Facet)]
pub struct ChartBar {
    /// The bar as played, 1 the first after the count-in: a repeat's
    /// second pass is the next bars along.
    pub bar: u32,
    /// The written bar it plays, 1 the first, as the chart draws them.
    pub written: u32,
    /// Which time through a repeat (1 outside one).
    pub pass: u32,
    /// Its section, as the chart names it (`Verse`; the song's region
    /// for it numbers it, `Verse 1`).
    pub section: String,
    /// Where the bar starts and ends, seconds.
    pub start: f64,
    pub end: f64,
}

/// The bar of `chart` (Keyflow source) playing at `at` seconds, on the
/// song the chart built: `None` in the count-in or after the last bar.
///
/// # Errors
///
/// The chart does not parse.
pub fn chart_bar(chart: &str, at: f64) -> Result<Option<ChartBar>, String> {
    let layout =
        engine_domain::setlist::chart_import::chart_to_layout(chart).map_err(|e| e.to_string())?;
    let parsed = keyflow::text::chart::parse_chart(chart).map_err(|e| format!("chart: {e}"))?;
    let played = keyflow::text::bar_times::bar_times(
        &parsed,
        keyflow::text::bar_times::BarClock::at(layout.count_in_seconds),
    );
    Ok(played
        .iter()
        .enumerate()
        .find(|(_, b)| b.start <= at && at < b.end)
        .map(|(index, b)| ChartBar {
            bar: one_based(index),
            written: one_based(b.bar),
            pass: b.pass,
            section: parsed
                .sections
                .get(b.section)
                .map(|s| s.section.display_name())
                .unwrap_or_default(),
            start: b.start,
            end: b.end,
        }))
}

/// A 0-based index as a 1-based number.
fn one_based(index: usize) -> u32 {
    u32::try_from(index).map_or(u32::MAX, |i| i.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 120 bpm, 2 s bars: a verse whose two bars repeat, then a chorus.
    const REPEAT: &str = "Song - Artist\n120bpm 4/4 #C\n\nVS 4\n|: C | G :| F C\n\nCH 2\nF G\n";

    fn at(chart: &str, t: f64) -> Option<(u32, u32, u32, String)> {
        chart_bar(chart, t)
            .unwrap()
            .map(|b| (b.bar, b.written, b.pass, b.section))
    }

    #[test]
    fn the_first_pass_is_the_bars_as_written() {
        assert_eq!(at(REPEAT, 0.5), Some((1, 1, 1, "Verse".into())));
        assert_eq!(at(REPEAT, 2.5), Some((2, 2, 1, "Verse".into())));
    }

    #[test]
    fn the_second_pass_plays_the_written_bars_again() {
        assert_eq!(at(REPEAT, 4.5), Some((3, 1, 2, "Verse".into())));
        assert_eq!(at(REPEAT, 6.5), Some((4, 2, 2, "Verse".into())));
    }

    #[test]
    fn after_the_repeat_the_bars_come_later_by_its_pass() {
        assert_eq!(at(REPEAT, 8.5), Some((5, 3, 1, "Verse".into())));
        // Past the written end (6 bars written, 12 s): still the chart.
        assert_eq!(at(REPEAT, 13.0), Some((7, 5, 1, "Chorus".into())));
        assert_eq!(at(REPEAT, 15.9), Some((8, 6, 1, "Chorus".into())));
    }

    #[test]
    fn past_the_last_bar_there_is_none() {
        assert_eq!(at(REPEAT, 16.0), None);
    }

    #[test]
    fn a_count_in_comes_before_bar_one() {
        let chart = "Song - Artist\n120bpm 4/4 #C\n\ncount 1\n\nVS 2\nC G\n";
        assert_eq!(at(chart, 1.0), None);
        assert_eq!(at(chart, 2.5), Some((1, 1, 1, "Verse".into())));
    }
}

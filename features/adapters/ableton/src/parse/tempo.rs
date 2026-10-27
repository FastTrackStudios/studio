//! Tempo and time signature parsing.
//!
//! Tempo lives on the master/main track's mixer:
//! - v10+:  `MasterTrack.DeviceChain.Mixer.Tempo.Manual[@Value]`
//! - v12+:  `MainTrack.DeviceChain.Mixer.Tempo.Manual[@Value]`
//! - v8-9:  Falls back to `Tempo.ArrangerAutomation.Events.FloatEvent[@Value]`
//!
//! Time signature uses Ableton's encoded integer format (0-494):
//!   numerator = (encoded % 99) + 1
//!   denominator = 2^(encoded / 99)

use super::xml_helpers::*;
use crate::types::{AutomationPoint, TimeSignature};
use roxmltree::Node;

/// Parse the master tempo from a MasterTrack or MainTrack node.
pub fn parse_tempo(master_node: Node<'_, '_>) -> f64 {
    // Try: DeviceChain.Mixer.Tempo.Manual[@Value]
    if let Some(tempo_node) = descend(master_node, "DeviceChain.Mixer.Tempo") {
        if let Some(bpm) = child_f64(tempo_node, "Manual")
            && (10.0..=999.0).contains(&bpm)
        {
            return bpm;
        }

        // Fallback: ArrangerAutomation.Events first FloatEvent
        if let Some(events) = descend(tempo_node, "ArrangerAutomation.Events") {
            for event in events.children() {
                if event.has_tag_name("FloatEvent")
                    && let Some(bpm) = event.attribute("Value").and_then(|v| v.parse::<f64>().ok())
                    && (10.0..=999.0).contains(&bpm)
                {
                    return bpm;
                }
            }
        }
    }

    120.0 // Ableton default
}

/// Parse tempo automation events from the master track.
///
/// In v10+, automation lives in `AutomationEnvelopes.Envelopes` and is matched
/// to the tempo parameter via the `PointeeId` / `AutomationTarget.Id` link.
pub fn parse_tempo_automation(master_node: Node<'_, '_>) -> Vec<AutomationPoint> {
    let mut points = Vec::new();

    // Get the tempo automation target ID
    let target_id = descend(master_node, "DeviceChain.Mixer.Tempo.AutomationTarget")
        .and_then(|n| n.attribute("Id"))
        .and_then(|v| v.parse::<i32>().ok());

    // First check inline ArrangerAutomation on the Tempo element itself (v8-9)
    if let Some(events) = descend(
        master_node,
        "DeviceChain.Mixer.Tempo.ArrangerAutomation.Events",
    ) {
        collect_float_events(events, &mut points);
    }

    // Then check envelope-based automation (v10+)
    if let Some(target_id) = target_id
        && let Some(envelopes) = descend(master_node, "AutomationEnvelopes.Envelopes")
    {
        for envelope in envelopes.children() {
            if !envelope.has_tag_name("AutomationEnvelope") {
                continue;
            }
            let pointee_id =
                descend(envelope, "EnvelopeTarget").and_then(|n| child_i32(n, "PointeeId"));

            if pointee_id == Some(target_id) {
                if let Some(events) = descend(envelope, "Automation.Events") {
                    collect_float_events(events, &mut points);
                }
                break;
            }
        }
    }

    // Live parks the envelope's first breakpoint far before the arrangement
    // (-63072000 beats): it is the tempo the set starts at — a set whose
    // tempo lives in its automation says nothing else about it (`Manual` is
    // then whatever the knob was left at). Kept: `TempoMap` reads the tempo
    // at beat 0 off it, and a set written back needs it.
    points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    points
}

fn collect_float_events(events_node: Node<'_, '_>, points: &mut Vec<AutomationPoint>) {
    for event in events_node.children() {
        if event.has_tag_name("FloatEvent")
            && let (Some(time), Some(value)) = (
                event.attribute("Time").and_then(|v| v.parse::<f64>().ok()),
                event.attribute("Value").and_then(|v| v.parse::<f64>().ok()),
            )
        {
            let curve_control_1 = parse_curve_control(&event, "CurveControl1X", "CurveControl1Y");
            let curve_control_2 = parse_curve_control(&event, "CurveControl2X", "CurveControl2Y");
            points.push(AutomationPoint {
                time,
                value,
                curve_control_1,
                curve_control_2,
            });
        }
    }
}

/// Parse a pair of bezier control handle attributes from a FloatEvent.
fn parse_curve_control(event: &Node<'_, '_>, x_attr: &str, y_attr: &str) -> Option<(f64, f64)> {
    let x = event
        .attribute(x_attr)
        .and_then(|v| v.parse::<f64>().ok())?;
    let y = event
        .attribute(y_attr)
        .and_then(|v| v.parse::<f64>().ok())?;
    Some((x, y))
}

/// Parse the time signature from the master track.
///
/// Location: `MasterTrack.DeviceChain.Mixer.TimeSignature.TimeSignatures
///            .RemoteableTimeSignature.Numerator/Denominator`
///
/// Or from encoded `EnumEvent` values (0-494).
pub fn parse_time_signature(master_node: Node<'_, '_>) -> TimeSignature {
    // Try direct Numerator/Denominator (common in v10+)
    if let Some(ts_node) = descend(
        master_node,
        "DeviceChain.Mixer.TimeSignature.TimeSignatures",
    ) {
        for remote_ts in ts_node.children() {
            if remote_ts.has_tag_name("RemoteableTimeSignature")
                && let (Some(num), Some(den)) = (
                    child_i32(remote_ts, "Numerator"),
                    child_i32(remote_ts, "Denominator"),
                )
                && num > 0
                && den > 0
            {
                return TimeSignature {
                    numerator: num as u8,
                    denominator: den as u8,
                };
            }
        }
    }

    // Try encoded EnumEvent format
    if let Some(events) = descend(
        master_node,
        "DeviceChain.Mixer.TimeSignature.AutomationTarget",
    ) {
        // Look for EnumEvent in automation
        if let Some(encoded) = events
            .parent()
            .and_then(|p| descend(p, "ArrangerAutomation.Events"))
        {
            for event in encoded.children() {
                if event.has_tag_name("EnumEvent")
                    && let Some(v) = event.attribute("Value").and_then(|v| v.parse::<u32>().ok())
                {
                    return decode_time_signature(v);
                }
            }
        }
    }

    TimeSignature::default()
}

/// Decode Ableton's encoded time signature integer (0-494).
///
/// Formula:
///   numerator = (encoded % 99) + 1
///   denominator = 2^(encoded / 99)
fn decode_time_signature(encoded: u32) -> TimeSignature {
    let numerator = (encoded % 99) + 1;
    let denominator = 1u32 << (encoded / 99); // 2^(encoded/99)

    TimeSignature {
        numerator: numerator.min(255) as u8,
        denominator: denominator.min(255) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_tempo_automation;
    use crate::tempo::TempoMap;

    /// JT's recital: the knob says 120, the automation starts at 160 and
    /// drops to 116 at beat 768.
    const MASTER: &str = r#"<MainTrack>
        <AutomationEnvelopes><Envelopes>
          <AutomationEnvelope Id="1">
            <EnvelopeTarget><PointeeId Value="8" /></EnvelopeTarget>
            <Automation><Events>
              <FloatEvent Id="1" Time="-63072000" Value="160" />
              <FloatEvent Id="2" Time="768" Value="160" />
              <FloatEvent Id="3" Time="768" Value="116" />
            </Events></Automation>
          </AutomationEnvelope>
        </Envelopes></AutomationEnvelopes>
        <DeviceChain><Mixer><Tempo>
          <Manual Value="120" />
          <AutomationTarget Id="8" />
        </Tempo></Mixer></DeviceChain>
      </MainTrack>"#;

    #[test]
    fn a_tempo_kept_in_automation_starts_where_the_automation_does() {
        let doc = roxmltree::Document::parse(MASTER).expect("xml");
        let points = parse_tempo_automation(doc.root_element());
        let map = TempoMap::new(120.0, &points);
        assert!((map.bpm_at(0.0) - 160.0).abs() < 1e-9, "not the knob's 120");
        // 768 beats at 160 bpm, then 116.
        assert!((map.seconds_at(768.0) - 288.0).abs() < 1e-9);
        assert!((map.bpm_at(800.0) - 116.0).abs() < 1e-9);
    }
}

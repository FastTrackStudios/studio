//! A Live set's tempo map, converted: every tempo step and every change
//! of metre reaches the `.RPP` (`studio::file::convert`), each tempo a
//! square point and each metre change named on the point where it falls.

#![cfg(feature = "convert-ableton")]

use std::io::Write;

/// A Live 12 set with nothing but its main track's tempo and time-signature
/// envelopes: `tempo` and `meters` are `(beat, value)` events, Live's way
/// (a step two events at one beat, the first parked far before the set;
/// a signature as Live's one number, `(numerator - 1) + 99 * log2(den)`).
fn live_set(tempo: &[(f64, f64)], meters: &[(f64, u32)]) -> String {
    let events = |tag: &str, events: &[(f64, f64)]| -> String {
        events
            .iter()
            .enumerate()
            .map(|(i, (t, v))| format!(r#"<{tag} Id="{i}" Time="{t}" Value="{v}" />"#))
            .collect()
    };
    let meters: Vec<(f64, f64)> = meters.iter().map(|&(t, v)| (t, f64::from(v))).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12049" Creator="Ableton Live 12.0">
  <LiveSet>
    <Tracks />
    <MainTrack>
      <AutomationEnvelopes><Envelopes>
        <AutomationEnvelope Id="0">
          <EnvelopeTarget><PointeeId Value="8" /></EnvelopeTarget>
          <Automation><Events>{tempo}</Events></Automation>
        </AutomationEnvelope>
        <AutomationEnvelope Id="1">
          <EnvelopeTarget><PointeeId Value="9" /></EnvelopeTarget>
          <Automation><Events>{meters}</Events></Automation>
        </AutomationEnvelope>
      </Envelopes></AutomationEnvelopes>
      <DeviceChain><Mixer>
        <Tempo><Manual Value="120" /><AutomationTarget Id="8" /></Tempo>
        <TimeSignature><Manual Value="200" /><AutomationTarget Id="9" /></TimeSignature>
      </Mixer></DeviceChain>
    </MainTrack>
    <Locators><Locators /></Locators>
  </LiveSet>
</Ableton>"#,
        tempo = events("FloatEvent", tempo),
        meters = events("EnumEvent", &meters),
    )
}

/// `xml` written gzipped as `<dir>/Set.als`, converted to `Set.RPP`, read
/// back: its tempo points as `(seconds, bpm, shape, signature)`, and the
/// header's tempo and signature.
fn converted(xml: &str) -> (Vec<(f64, f64, i32, Option<(i32, i32)>)>, (f64, i32, i32)) {
    let dir = tempfile::tempdir().expect("a folder");
    let als = dir.path().join("Set.als");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(xml.as_bytes()).expect("gzip");
    std::fs::write(&als, gz.finish().expect("gzip")).expect("the set");
    let rpp = dir.path().join("Set.RPP");
    studio::file::convert(&als, &rpp).expect("converts");
    let project = dawfile_reaper::io::read_project(&rpp).expect("reads back");
    let (bpm, num, den, _) = project.properties.tempo.expect("a TEMPO line");
    let points = project
        .tempo_envelope
        .map(|env| {
            env.points
                .iter()
                .map(|p| {
                    let sig = p
                        .time_signature_encoded
                        .map(|e| (e & 0xFFFF, (e >> 16) & 0xFFFF));
                    (p.position, p.tempo, p.shape, sig)
                })
                .collect()
        })
        .unwrap_or_default();
    (points, (f64::from(bpm), num, den))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn every_metre_change_and_tempo_step_reaches_the_rpp() {
    // 120 bpm (parked before the set) to beat 16, a step to 60; 4/4, a 5/4
    // bar at beat 8, 4/4 again at 13, a 3/4 bar at 21 (past the step),
    // 4/4 at 24. 4/4 is 201, 5/4 is 202, 3/4 is 200 in Live's numbering.
    let xml = live_set(
        &[(-63_072_000.0, 120.0), (16.0, 120.0), (16.0, 60.0)],
        &[
            (-63_072_000.0, 201),
            (8.0, 202),
            (13.0, 201),
            (21.0, 200),
            (24.0, 201),
        ],
    );
    let (points, header) = converted(&xml);
    assert!(
        close(header.0, 120.0) && (header.1, header.2) == (4, 4),
        "the tempo and metre at 0, not the knobs' (Manual 120, 3/4): {header:?}"
    );
    // Beats 0..16 at 120 (0.5 s each), then 60 (1 s each).
    let expected: [(f64, f64, Option<(i32, i32)>); 6] = [
        (0.0, 120.0, Some((4, 4))),
        (4.0, 120.0, Some((5, 4))),
        (6.5, 120.0, Some((4, 4))),
        (8.0, 60.0, None),
        (13.0, 60.0, Some((3, 4))),
        (16.0, 60.0, Some((4, 4))),
    ];
    assert_eq!(points.len(), expected.len(), "{points:?}");
    for (got, want) in points.iter().zip(expected) {
        assert!(
            close(got.0, want.0) && close(got.1, want.1) && got.3 == want.2,
            "{got:?} is not {want:?}"
        );
        assert_eq!(got.2, 1, "square: a step, never a ramp");
    }
}

#[test]
fn a_set_at_one_tempo_in_one_metre_has_no_envelope() {
    // 6/8 throughout (Live's 5 + 99 * 3 = 302), 97 bpm.
    let (points, header) = converted(&live_set(&[(-63_072_000.0, 97.0)], &[(-63_072_000.0, 302)]));
    assert!(points.is_empty(), "{points:?}");
    assert!(
        close(header.0, 97.0) && (header.1, header.2) == (6, 8),
        "{header:?}"
    );
}

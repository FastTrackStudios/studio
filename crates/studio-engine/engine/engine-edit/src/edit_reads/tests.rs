//! The compact reads: a hit list's rows line up with its header, the
//! filters an agent narrows by, the bars a hit falls in, a slice's speed,
//! a quantize's detail, and the exceptions listed apart.

use super::*;

fn hit(id: &str, take: &str, time: f64, now: Option<f64>, bar: i32) -> HitRow {
    HitRow {
        id: id.to_owned(),
        piece: "Kick".to_owned(),
        take: take.to_owned(),
        time,
        now,
        bar,
        beat: 1.25,
        peak_db: -6.04,
        confidence: 0.9,
        heard: "both".to_owned(),
        flags: vec!["ghost".to_owned(), "flam".to_owned()],
        origin: "auto".to_owned(),
        note: String::new(),
    }
}

fn arg(json: &str) -> ListArg {
    facet_json::from_str(json).expect("an arg")
}

#[test]
fn a_compact_list_is_a_header_and_a_row_per_hit() {
    let all = vec![
        hit("a~1", "260907_1321-01", 10.0, Some(10.01), 9),
        hit("b~2", "260907_1340-02", 10.0, None, 9),
        hit("c~1", "260907_1321-01", 20.0, Some(20.0), 14),
    ];
    let out: Value = serde_json::from_str(
        &list_page(&arg(r#"{"compact": true, "playing": true}"#), all).expect("a page"),
    )
    .expect("json");
    assert_eq!(out["total"], 2);
    assert_eq!(out["cols"].as_array().map(Vec::len), Some(HIT_COLS.len()));
    let rows = out["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2);
    let first = rows[0].as_array().expect("a row");
    assert_eq!(
        first.len(),
        HIT_COLS.len(),
        "a row lines up with its header"
    );
    assert_eq!(first[0], "a~1");
    assert_eq!(first[2], "260907_1321-01", "the take, by its pass");
    assert_eq!(first[4], 10.01);
    assert_eq!(first[7], -6.0);
    assert_eq!(first[9], "ghost,flam");
    assert!(out["next"].is_null());
}

#[test]
fn bars_and_takes_narrow_the_list_and_pages_say_the_next() {
    let all: Vec<HitRow> = (0..10)
        .map(|i| {
            let t = f64::from(i);
            hit(
                &format!("h{i}"),
                if i % 2 == 0 { "A" } else { "B" },
                t,
                Some(t),
                i,
            )
        })
        .collect();
    let narrowed = all
        .iter()
        .filter(|h| wanted(&arg(r#"{"bars": [3, 6], "take": "a"}"#), h))
        .map(|h| h.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(narrowed, ["h4", "h6"]);
    let out: Value = serde_json::from_str(
        &list_page(&arg(r#"{"compact": true, "limit": 4}"#), all.clone()).expect("a page"),
    )
    .expect("json");
    assert_eq!(out["next"], 4);
    // Objects keep their old shape, the take added.
    let objects: Value =
        serde_json::from_str(&list_page(&arg(r#"{"limit": 1}"#), all).expect("a page"))
            .expect("json");
    assert_eq!(objects["hits"][0]["take"], "A");
    assert_eq!(objects["total"], 10);
}

#[test]
fn a_time_is_in_the_bar_that_holds_it() {
    let bars = [
        BarSpan {
            bar: 9,
            start: 10.0,
            end: 12.0,
        },
        BarSpan {
            bar: 10,
            start: 12.0,
            end: 14.0,
        },
    ];
    assert_eq!(bar_of(&bars, 10.0), Some(0));
    assert_eq!(bar_of(&bars, 12.0), Some(1));
    assert_eq!(bar_of(&bars, 13.999), Some(1));
    assert_eq!(bar_of(&bars, 14.0), None);
    assert_eq!(bar_of(&bars, 9.0), None);
}

#[test]
fn a_slice_drawn_out_is_flagged_slow() {
    let marker = |position: f64, source_position: f64| daw_proto::StretchMarker {
        position,
        source_position,
        slope: 0.0,
    };
    // Bar 128: 0.11 s of recording drawn over 1 s.
    let drawn = speeds(
        1.0,
        &[marker(0.0, 0.0), marker(1.0, 0.11), marker(1.2, 0.31)],
    );
    assert!((drawn.1 - 0.11).abs() < 1e-9, "{drawn:?}");
    assert_eq!(speed_flag(drawn), "slow");
    assert_eq!(speed_flag(speeds(1.0, &[])), "");
    assert_eq!(speed_flag(speeds(1.3, &[])), "fast");
}

#[test]
fn a_quantizes_detail_lists_every_move_and_why_each_hit_was_held() {
    let zone = crate::hits::Zone {
        guid: "z1".to_owned(),
        start: 30.0,
        end: 40.0,
        kind: crate::hits::ZoneKind::Exception,
        origin: crate::hits::ZoneOrigin::Manual,
        note: "put back as played".to_owned(),
        grid: None,
    };
    let moves: Vec<(f64, f64)> = (0..45)
        .map(|i| (f64::from(i), f64::from(i) + 0.01))
        .collect();
    let held = [
        (31.0, Held::Exception),
        (32.0, Held::Exception),
        (50.0, Held::NoLine),
    ];
    let mut d = detail((&moves, &held), 0.5, std::slice::from_ref(&zone), |t| {
        (
            num_traits::ToPrimitive::to_i32(&t.floor()).unwrap_or(0),
            1.0,
        )
    });
    assert_eq!(d.moves.rows.len(), 45, "every move, not the biggest 30");
    assert_eq!(d.moves.cols.len(), d.moves.rows[0].len());
    assert_eq!(d.moves.rows[0][4], 10.0, "off 10 ms");
    assert_eq!(d.moves.rows[0][5], 5.0, "moved half of it");
    let exception = d
        .held
        .iter()
        .find(|g| g.why == Held::Exception.why())
        .expect("held by the zone");
    assert_eq!(exception.count, 2);
    assert_eq!(d.zones_held.len(), 1);
    assert_eq!(d.zones_held[0].hits, 2);
    d.brief();
    assert!(d.moves.rows.is_empty());
    assert_eq!(
        d.held.iter().map(|g| g.count).sum::<u32>(),
        3,
        "counts stay"
    );
}

#[test]
fn exceptions_are_listed_apart_from_the_fills() {
    let zone = |guid: &str, kind, origin| crate::hits::Zone {
        guid: guid.to_owned(),
        start: 0.0,
        end: 1.0,
        kind,
        origin,
        note: String::new(),
        grid: None,
    };
    use crate::hits::{ZoneKind, ZoneOrigin};
    let out = zones_out(vec![
        zone("fill", ZoneKind::Fill, ZoneOrigin::Approved),
        zone("hold", ZoneKind::Exception, ZoneOrigin::Manual),
        zone("no", ZoneKind::Exception, ZoneOrigin::Rejected),
    ]);
    assert_eq!(
        out.exceptions
            .iter()
            .map(|z| z.guid.as_str())
            .collect::<Vec<_>>(),
        ["hold"]
    );
    assert_eq!(out.zones.len(), 2);
}

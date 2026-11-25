//! SCRATCH (not committed): a real chart (`KF_CHART`) imported through the
//! service as Studio does, then composed and parsed as the pane does.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keyflow_lang::text::chart::parse_chart;
use keyflow_session::typed::{Held, TypedChart, plan};
use keyflow_session::{ChartSource, EXTENSION, Provider};
use session::doc::SessionDoc;

#[test]
fn a_real_chart_round_trips() {
    let Ok(path) = std::env::var("KF_CHART") else {
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap();
    let chart = parse_chart(&text).expect("the .kf parses");
    let doc = SessionDoc::new();
    let id = uuid::Uuid::new_v4();
    let provider = Provider::default();
    provider.open_doc(id, doc.clone());
    let source = ChartSource::Session {
        doc: id.to_string(),
    };
    let ask = |call: &keyflow_session::ServiceCall| {
        provider
            .answer(call.method, &call.args, Some(EXTENSION))
            .unwrap()
    };
    let held = || {
        let [t, s, d] = source.timeline_calls().unwrap().unwrap();
        Held::of_replies(&ask(&t), &ask(&s), &ask(&d)).unwrap()
    };
    let calls = source.typed_calls(plan(&chart, &held())).unwrap();
    for call in &calls {
        ask(call);
    }
    let tempo = doc.tempo();
    println!(
        "tempo at 0: {} bpm; regions: {}",
        tempo.bpm_at(0.0),
        doc.regions().len()
    );
    let now = held();
    println!(
        "held tempo {} bpm, regions {}",
        now.tempo.bpm_at(0.0),
        now.regions.len()
    );
    let road = now.road_map();
    println!("road: {:?}", road.tempo);
    for s in &road.sections {
        println!("  {} bar {} x{}", s.name, s.first_bar, s.bars);
    }
    let typed = TypedChart::of(&chart);
    println!(
        "chart sections: {:?}",
        typed
            .sections()
            .iter()
            .map(|s| (&s.0, s.1, s.2))
            .collect::<Vec<_>>()
    );
    let shown = now.shown_source("Sleigh Ride");
    std::fs::write("/tmp/kf/shown.kf", &shown).unwrap();
    let back = TypedChart::of(&parse_chart(&shown).expect("what the pane shows parses"));
    let key = |t: &TypedChart| {
        t.chords
            .iter()
            .map(|c| format!("{}:{}:{}:{}", c.bar + 1, c.beat, c.symbol, c.length))
            .collect::<Vec<_>>()
    };
    let (a, b) = (key(&typed), key(&back));
    println!(
        "BARS {} -> {}; CHORDS {} -> {}",
        typed.bars.len(),
        back.bars.len(),
        a.len(),
        b.len()
    );
    let mut shown_diffs = 0;
    for (x, y) in a.iter().zip(&b) {
        if x != y && shown_diffs < 5 {
            println!("DIFF {x}  vs  {y}");
            shown_diffs += 1;
        }
    }
    assert_eq!(typed.bars.len(), back.bars.len(), "bars");
    assert_eq!(a, b, "chords");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_session_shows() {
    let Ok(path) = std::env::var("KF_SESSION") else {
        return;
    };
    let file = session::file::SessionFile::open(std::path::Path::new(&path))
        .await
        .unwrap();
    let held = Held::of_doc(file.session());
    let road = held.road_map();
    println!(
        "tempo marks {}; first {:?}",
        road.tempo.len(),
        road.tempo.first()
    );
    for s in &road.sections {
        println!("  {:?} bar {} x{}", s.name, s.first_bar, s.bars);
    }
    let shown = held.shown_source("Real");
    std::fs::write("/tmp/kf/real-shown.kf", &shown).unwrap();
    parse_chart(&shown).expect("what the pane shows parses");
}

#[test]
fn meters_of_a_turn() {
    let chart = parse_chart("Turn\n#C 60bpm 4/4\n\nCount 1\n\nIN 3\nC\n!T3/4 F\n!T4/4 G\n").unwrap();
    for s in &chart.sections {
        for m in s.measures() {
            println!("{:?} {:?}", s.section.section_type, m.time_signature);
        }
    }
    println!("initial {:?} final {:?}", chart.initial_time_signature, chart.time_signature);
}

#[test]
fn sections_of_a_real_chart() {
    let Ok(path) = std::env::var("KF_CHART") else { return };
    let chart = parse_chart(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let typed = TypedChart::of(&chart);
    for (name, from, to) in typed.sections() {
        println!("SECTION {name:?} {from} {to}");
    }
    for s in &chart.sections {
        println!("PARSED {:?} count {:?} measures {}", s.section.section_type, s.section.measure_count, s.measures().len());
    }
}

//! The Signal inserts' model: a chain read into rows, followed by its
//! revision, and every control sent as the action it stands for.

use std::sync::Arc;

use serde_json::json;
use web_time::{Duration, Instant};

use super::chain::{BlockId, Chain, ChainError, PhaseId, Status, Via};
use super::source::{Call, Edit, Fixture, MeterValue, ParamValue, SignalSource, SourceError};
use super::view::{self, Fold, Hit, Inserts, Row};
use crate::tone::Panel;

const KICK: &str = "Drums/Kick";

fn kit() -> Vec<(String, Chain)> {
    super::fixtures::kick_kit().expect("the Kick fixture parses")
}

fn kick() -> Chain {
    kit()
        .into_iter()
        .find(|(p, _)| p == KICK)
        .expect("Drums/Kick")
        .1
}

/// Inserts over a fixture of the whole Kick folder, each chain read once.
fn followed() -> (Arc<Fixture>, Inserts) {
    let fixture = Arc::new(Fixture::new(kit()));
    let source: Arc<dyn SignalSource> = fixture.clone();
    let mut inserts = Inserts::new(Some(source));
    let first = inserts.follow();
    assert_eq!(first.read.len(), 7, "every Kick track's chain is read once");
    (fixture, inserts)
}

fn panel() -> Panel {
    Panel {
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 400.0,
    }
}

fn runs(fixture: &Fixture) -> Vec<(String, serde_json::Value)> {
    fixture
        .calls()
        .into_iter()
        .filter_map(|c| match c {
            Call::Run { action, args, .. } => Some((action, args)),
            _ => None,
        })
        .collect()
}

/// The rows' shape, as words: what each row is.
fn shape(chain: &Chain, fold: Fold) -> Vec<String> {
    view::rows(chain, fold)
        .iter()
        .map(|row| match row {
            Row::Fx { .. } => "Signal".to_owned(),
            Row::Phase(p) => format!("[{}]", p.id.as_str()),
            Row::Block { block, .. } => block.id.as_str().to_owned(),
        })
        .collect()
}

#[test]
fn the_kick_fixture_is_the_kits_chains() {
    let kit = kit();
    assert_eq!(kit.len(), 7);
    let kick = kick();
    assert_eq!(kick.port.as_deref(), Some(KICK));
    assert_eq!(kick.session.as_deref(), Some("unbreakable-drums"));
    let (_, gate) = kick.block("rescue.gate").expect("the kick's gate");
    assert_eq!(gate.kind, "gate");
    let key = gate.key.as_ref().expect("keyed");
    assert_eq!(key.via, Via::Hits);
    assert_eq!(key.from.as_deref(), Some("Drums/Kick/Sum/Trig"));
    let threshold = gate
        .params
        .iter()
        .find(|p| p.id == "threshold")
        .expect("threshold");
    assert_eq!(threshold.text, "-40.0 dB");
    assert!(threshold.clap_id.is_some());
    assert_eq!(
        gate.meters.first().map(|m| m.id.as_str()),
        Some("gain_reduction")
    );
    assert!((kick.phase("tone").expect("tone").makeup_db - 3.81).abs() < 1e-9);
}

#[test]
fn a_chain_opens_into_its_phases_and_blocks() {
    assert_eq!(
        shape(&kick(), Fold::Open),
        [
            "Signal",
            "[rescue]",
            "rescue.gate",
            "[tone]",
            "tone.comp",
            "tone.eq",
            "tone.sat"
        ]
    );
}

#[test]
fn a_shut_chain_is_one_insert() {
    assert_eq!(shape(&kick(), Fold::Shut), ["Signal"]);
    let chain = kick();
    let laid = view::layout(&view::rows(&chain, Fold::Shut), panel());
    assert_eq!(laid.len(), 1);
    assert!((view::tall(&chain, Fold::Shut) - view::FX_H).abs() < 1e-9);
}

#[test]
fn rows_are_laid_end_to_end_and_hit_where_drawn() {
    let chain = kick();
    let rows = view::rows(&chain, Fold::Open);
    let laid = view::layout(&rows, panel());
    for pair in laid.windows(2) {
        assert!((pair[0].1.y + pair[0].1.height - pair[1].1.y).abs() < 1e-9);
    }
    let (_, gate_row) = laid
        .iter()
        .find(|(r, _)| matches!(r, Row::Block { block, .. } if block.id.as_str() == "rescue.gate"))
        .expect("the gate's row");
    let mid_y = gate_row.y + gate_row.height / 2.0;
    assert_eq!(
        view::hit(&laid, gate_row.x + 20.0, mid_y),
        Some(Hit::Open(BlockId("rescue.gate".into())))
    );
    assert_eq!(
        view::hit(&laid, gate_row.x + gate_row.width - 4.0, mid_y),
        Some(Hit::BlockBypass(BlockId("rescue.gate".into())))
    );
    assert_eq!(view::hit(&laid, 10.0, 2.0), Some(Hit::Fold));
    let (_, tone_bar) = laid
        .iter()
        .find(|(r, _)| matches!(r, Row::Phase(p) if p.id.as_str() == "tone"))
        .expect("the tone bar");
    let makeup = view::makeup_box(*tone_bar);
    assert_eq!(
        view::hit(&laid, makeup.x + 2.0, tone_bar.y + 4.0),
        Some(Hit::Makeup(PhaseId("tone".into())))
    );
}

#[test]
fn a_bypass_is_one_signal_set_on_that_block() {
    let (fixture, mut inserts) = followed();
    let chain = inserts.strip(KICK).expect("the kick").chain.clone();
    let edit = view::toggle_edit(&chain, &Hit::BlockBypass(BlockId("tone.comp".into())))
        .expect("a switch is an edit");
    inserts.run(KICK, &edit).expect("the fixture takes it");
    assert_eq!(
        runs(&fixture),
        [(
            "SIGNAL_SET".to_owned(),
            json!({ "block": "tone.comp", "bypass": true })
        )]
    );
    let strip = inserts.strip(KICK).expect("the kick");
    assert!(strip.chain.block("tone.comp").expect("comp").1.bypass);
    assert_eq!(
        strip.chain.revision,
        chain.revision + 1,
        "an edit bumps the revision"
    );
}

#[test]
fn a_phase_switch_and_make_up_are_signal_sets_on_the_phase() {
    let (fixture, mut inserts) = followed();
    let chain = inserts.strip(KICK).expect("the kick").chain.clone();
    let edit =
        view::toggle_edit(&chain, &Hit::PhaseBypass(PhaseId("rescue".into()))).expect("an edit");
    inserts.run(KICK, &edit).expect("taken");
    let db = view::dragged_makeup(3.81, -12.0);
    assert!(
        (db - 5.0).abs() < 1e-9,
        "dragged up 12 px is +1.2 dB, in tenths: {db}"
    );
    inserts
        .run(
            KICK,
            &Edit::Makeup {
                phase: PhaseId("tone".into()),
                db,
            },
        )
        .expect("taken");
    assert_eq!(
        runs(&fixture),
        [
            (
                "SIGNAL_SET".to_owned(),
                json!({ "phase": "rescue", "bypass": true })
            ),
            (
                "SIGNAL_SET".to_owned(),
                json!({ "phase": "tone", "makeup_db": 5.0 })
            ),
        ]
    );
}

#[test]
fn a_moved_revision_is_read_again_and_a_still_one_is_not() {
    let (fixture, mut inserts) = followed();
    let reads = |f: &Fixture| {
        f.calls()
            .iter()
            .filter(|c| matches!(c, Call::Chain(_)))
            .count()
    };
    assert_eq!(reads(&fixture), 7);
    let still = inserts.follow();
    assert!(!still.changed());
    assert_eq!(reads(&fixture), 7, "nothing moved, nothing read");
    let seen = inserts.generation();

    // Signal changed the kick's chain itself (a preset): a new revision.
    let mut changed = kick();
    changed.revision += 10;
    changed.phases[0].blocks[0].status = Status::Unavailable;
    fixture.replace(KICK, changed);
    let moved = inserts.follow();
    assert_eq!(moved.read, [KICK.to_owned()]);
    assert_eq!(reads(&fixture), 8, "only the kick is read again");
    assert_ne!(inserts.generation(), seen);
    let strip = inserts.strip(KICK).expect("the kick");
    assert_eq!(
        strip.chain.block("rescue.gate").expect("gate").1.status,
        Status::Unavailable
    );
}

#[test]
fn a_fold_survives_a_new_revision() {
    let (fixture, mut inserts) = followed();
    assert!(inserts.set_fold(KICK, Some(Fold::Shut)));
    let mut changed = kick();
    changed.revision += 1;
    fixture.replace(KICK, changed);
    inserts.follow();
    assert_eq!(inserts.strip(KICK).expect("kick").fold, Fold::Shut);
}

#[test]
fn a_chain_that_cannot_be_read_is_not_asked_again_until_it_moves() {
    struct Broken;
    impl SignalSource for Broken {
        fn signal_chain(&self, _: &str) -> Result<Chain, SourceError> {
            Err(SourceError::Refused("the plugin is busy".into()))
        }
        fn signal_run(
            &self,
            _: &str,
            _: &str,
            _: &serde_json::Value,
        ) -> Result<Chain, SourceError> {
            Err(SourceError::Refused("busy".into()))
        }
        fn signal_session(&self, _: &str) -> Result<super::source::Session, SourceError> {
            Err(SourceError::Refused("busy".into()))
        }
        fn signal_algorithms(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Vec<super::source::Algorithm>, SourceError> {
            Err(SourceError::Refused("busy".into()))
        }
        fn signal_revisions(&self) -> Vec<(String, u64)> {
            vec![("t".into(), 3)]
        }
        fn signal_meters(&self, _: &str) -> Vec<MeterValue> {
            Vec::new()
        }
        fn fx_set(&self, _: &str, _: &str, _: &ParamValue) -> Result<(), SourceError> {
            Ok(())
        }
    }
    let mut inserts = Inserts::new(Some(Arc::new(Broken)));
    assert_eq!(inserts.follow().failed.len(), 1);
    assert!(
        inserts.follow().failed.is_empty(),
        "not asked again at revision 3"
    );
    assert!(!inserts.due(Instant::now(), inserts.generation()));
}

#[test]
fn a_track_whose_signal_is_gone_drops_its_rows() {
    let plan = view::stale([("a", 1), ("b", 2)], &[("b".into(), 2), ("c".into(), 1)]);
    assert_eq!(plan.reread, ["c".to_owned()]);
    assert_eq!(plan.gone, ["a".to_owned()]);
}

#[test]
fn a_value_goes_to_fx_set_on_its_clap_slot() {
    let (fixture, mut inserts) = followed();
    let clap = kick()
        .block("tone.comp")
        .and_then(|(_, b)| b.params.iter().find(|p| p.id == "ratio").cloned())
        .and_then(|p| p.clap_id)
        .expect("ratio is exposed");
    inserts
        .set_param(
            KICK,
            &BlockId("tone.comp".into()),
            "ratio",
            &ParamValue::Normalized(0.5),
        )
        .expect("taken");
    assert!(fixture.calls().contains(&Call::FxSet {
        track: KICK.into(),
        clap_id: clap.clone(),
        value: ParamValue::Normalized(0.5),
    }));
    let ratio = inserts
        .strip(KICK)
        .and_then(|s| s.chain.block("tone.comp"))
        .and_then(|(_, b)| b.params.iter().find(|p| p.id == "ratio").cloned())
        .expect("ratio");
    assert!((ratio.value - 10.5).abs() < 1e-9);
    assert_eq!(ratio.text, "10.5:1");
    assert_eq!(
        super::source::fx_set_args(KICK, "Signal", &clap, &ParamValue::Normalized(0.5)),
        json!({ "track": KICK, "fx": "Signal", "set": { format!("@{clap}"): "=0.500000" } })
    );
}

#[test]
fn a_parameter_not_exposed_is_refused_without_a_call() {
    let mut chain = kick();
    chain.phases[1].blocks[0].params[0].clap_id = None;
    let fixture = Arc::new(Fixture::new([(KICK.to_owned(), chain)]));
    let source: Arc<dyn SignalSource> = fixture.clone();
    let mut inserts = Inserts::new(Some(source));
    inserts.follow();
    let param = kick().phases[1].blocks[0].params[0].id.clone();
    let block = kick().phases[1].blocks[0].id.clone();
    assert!(
        inserts
            .set_param(KICK, &block, &param, &ParamValue::Normalized(0.1))
            .is_err()
    );
    assert!(
        !fixture
            .calls()
            .iter()
            .any(|c| matches!(c, Call::FxSet { .. }))
    );
}

#[test]
fn blocks_are_added_removed_and_moved_through_signal_run() {
    let (fixture, mut inserts) = followed();
    inserts
        .run(
            KICK,
            &Edit::Add {
                phase: PhaseId("tone".into()),
                kind: "eq".into(),
                after: Some(BlockId("tone.sat".into())),
                algorithm: None,
            },
        )
        .expect("added");
    inserts
        .run(
            KICK,
            &Edit::Move {
                block: BlockId("tone.sat".into()),
                phase: PhaseId("tone".into()),
                after: None,
            },
        )
        .expect("moved");
    inserts
        .run(
            KICK,
            &Edit::Remove {
                block: BlockId("tone.eq".into()),
            },
        )
        .expect("removed");
    let chain = &inserts.strip(KICK).expect("kick").chain;
    assert_eq!(
        shape(chain, Fold::Open),
        [
            "Signal",
            "[rescue]",
            "rescue.gate",
            "[tone]",
            "tone.sat",
            "tone.comp",
            "tone.eq_2"
        ]
    );
    let actions: Vec<String> = runs(&fixture).into_iter().map(|(a, _)| a).collect();
    assert_eq!(
        actions,
        [
            "SIGNAL_BLOCK_ADD",
            "SIGNAL_BLOCK_MOVE",
            "SIGNAL_BLOCK_REMOVE"
        ]
    );
    assert_eq!(
        runs(&fixture)[0].1,
        json!({ "phase": "tone", "kind": "eq", "after": "tone.sat" })
    );
}

#[test]
fn every_edit_reads_back_from_its_action() {
    let edits = [
        Edit::BypassBlock {
            block: BlockId("rescue.gate".into()),
            bypass: true,
        },
        Edit::BypassPhase {
            phase: PhaseId("tone".into()),
            bypass: false,
        },
        Edit::Makeup {
            phase: PhaseId("tone".into()),
            db: 0.5,
        },
        Edit::Add {
            phase: PhaseId("tone".into()),
            kind: "eq".into(),
            after: Some(BlockId("tone.comp".into())),
            algorithm: Some("parametric".into()),
        },
        Edit::Algorithm {
            block: BlockId("depth.verb".into()),
            algorithm: "plate".into(),
        },
        Edit::Remove {
            block: BlockId("tone.eq".into()),
        },
        Edit::Move {
            block: BlockId("tone.eq".into()),
            phase: PhaseId("polish".into()),
            after: None,
        },
    ];
    for edit in edits {
        let (action, args) = edit.action();
        assert_eq!(Edit::from_action(action, &args), Ok(edit));
    }
    assert!(Edit::from_action("SIGNAL_EXPLODE", &json!({})).is_err());
}

#[test]
fn the_meters_are_read_ten_times_a_second() {
    let (fixture, mut inserts) = followed();
    fixture.set_meters(
        KICK,
        vec![MeterValue {
            index: 0,
            value: -6.5,
        }],
    );
    let t0 = Instant::now();
    assert!(inserts.read_meters(t0));
    assert_eq!(inserts.strip(KICK).and_then(|s| s.meter(0)), Some(-6.5));
    fixture.set_meters(
        KICK,
        vec![MeterValue {
            index: 0,
            value: -2.0,
        }],
    );
    assert!(!inserts.read_meters(t0 + Duration::from_millis(40)));
    assert_eq!(inserts.strip(KICK).and_then(|s| s.meter(0)), Some(-6.5));
    assert!(inserts.read_meters(t0 + Duration::from_millis(101)));
    assert_eq!(inserts.strip(KICK).and_then(|s| s.meter(0)), Some(-2.0));
}

#[test]
fn a_strip_draws_its_rows_once_per_change() {
    let (fixture, mut inserts) = followed();
    let palette = crate::palette::Palette::from_theme(&daw_ui::theming::Theme::dark());
    let font = crate::text::Font::embedded().expect("the embedded font");
    let first = {
        let strip = inserts.strip_mut(KICK).expect("kick");
        super::paint::rows_cached(strip, &palette, &font, panel())
            .commands
            .len()
    };
    assert!(first > 0);
    let strip = inserts.strip_mut(KICK).expect("kick");
    let key = strip.picture.as_ref().map(|(k, _)| *k);
    super::paint::rows_cached(strip, &palette, &font, panel());
    assert_eq!(strip.picture.as_ref().map(|(k, _)| *k), key, "same picture");
    // Meters moving never redraw the rows.
    fixture.set_meters(
        KICK,
        vec![MeterValue {
            index: 0,
            value: -9.0,
        }],
    );
    inserts.read_meters(Instant::now() + Duration::from_secs(1));
    assert!(inserts.strip(KICK).expect("kick").picture.is_some());
    // A bypass does.
    inserts
        .run(
            KICK,
            &Edit::BypassBlock {
                block: BlockId("rescue.gate".into()),
                bypass: true,
            },
        )
        .expect("taken");
    assert!(inserts.strip(KICK).expect("kick").picture.is_none());
}

#[test]
fn signal_inserts_opens_and_shuts_a_track() {
    let (_, mut inserts) = followed();
    let mut seen = 0;
    super::inserts(super::InsertsArg {
        track: KICK.into(),
        open: Some(false),
    })
    .expect("asked");
    super::take_asks(&mut inserts, &mut seen);
    assert_eq!(inserts.strip(KICK).expect("kick").fold, Fold::Shut);
    super::inserts(super::InsertsArg {
        track: KICK.into(),
        open: None,
    })
    .expect("asked");
    super::take_asks(&mut inserts, &mut seen);
    assert_eq!(inserts.strip(KICK).expect("kick").fold, Fold::Open);
    assert!(
        super::inserts(super::InsertsArg {
            track: " ".into(),
            open: None
        })
        .is_err()
    );
}

#[test]
fn a_document_is_read_leniently_and_refused_only_when_it_is_not_a_chain() {
    let doc = json!({
        "schema": "signal.chain/1",
        "revision": 9,
        "future_field": {"anything": true},
        "phases": [{
            "id": "tone",
            "blocks": [
                {"id": "tone.x", "kind": "clap", "status": "error", "key": {"via": "self"}, "backend": "clap", "new": 1},
                {"id": "tone.y", "kind": "nam", "status": "unavailable"}
            ]
        }]
    });
    let chain = Chain::from_value(&doc).expect("a chain");
    assert_eq!(chain.revision, 9);
    let (phase, x) = chain.block("tone.x").expect("x");
    assert_eq!(
        phase.name, "tone",
        "a phase with no name is called by its id"
    );
    assert_eq!(x.status, Status::Error);
    assert_eq!(x.key.as_ref().map(|k| k.via.clone()), Some(Via::OwnInput));
    assert_eq!(
        chain.block("tone.y").expect("y").1.status,
        Status::Unavailable
    );
    assert_eq!(
        super::paint::second_line(x).0,
        "error",
        "trouble is said before the key"
    );
    assert_eq!(
        Chain::from_value(&json!({"schema": "signal.chain/2", "phases": []})),
        Err(ChainError::Schema(Some("signal.chain/2".into())))
    );
    assert!(matches!(Chain::parse("{"), Err(ChainError::Json(_))));
    assert_eq!(
        Chain::from_value(&json!({"schema": "signal.chain/1"})),
        Err(ChainError::Shape("phases"))
    );
}

#[test]
fn the_session_is_every_port_sharing_it() {
    let (fixture, _) = followed();
    let session = fixture.signal_session(KICK).expect("a session");
    assert_eq!(session.session.as_deref(), Some("unbreakable-drums"));
    assert_eq!(session.ports.len(), 7);
}

#[test]
fn a_move_to_another_phase_takes_its_prefix() {
    let (_, mut inserts) = followed();
    inserts
        .run(
            KICK,
            &Edit::Move {
                block: BlockId("tone.sat".into()),
                phase: PhaseId("rescue".into()),
                after: Some(BlockId("rescue.gate".into())),
            },
        )
        .expect("moved");
    let chain = &inserts.strip(KICK).expect("kick").chain;
    assert!(chain.block("rescue.sat").is_some());
    assert!(chain.block("tone.sat").is_none());
}

#[test]
fn an_algorithm_is_listed_and_set() {
    let (fixture, mut inserts) = followed();
    let room = "Drums/Kick/Verb/Room";
    let listed = fixture.signal_algorithms(room, "reverb").expect("listed");
    assert!(listed.iter().any(|a| a.id == "plate"));
    inserts
        .run(
            room,
            &Edit::Algorithm {
                block: BlockId("depth.verb".into()),
                algorithm: "plate".into(),
            },
        )
        .expect("set");
    assert_eq!(
        runs(&fixture).last().map(|(_, a)| a.clone()),
        Some(json!({ "block": "depth.verb", "algorithm": "plate" }))
    );
    let chain = &inserts.strip(room).expect("room").chain;
    assert_eq!(
        chain
            .block("depth.verb")
            .expect("verb")
            .1
            .algorithm
            .as_deref(),
        Some("plate")
    );
}

#[test]
fn phase_meters_and_kit_lanes_are_read() {
    let doc = json!({
        "schema": "signal.chain/1",
        "mode": "kit",
        "phases": [],
        "lanes": [{
            "lane": "Drums/Kick",
            "phases": [{
                "id": "tone", "name": "Tone",
                "blocks": [{"id": "tone.comp", "kind": "comp", "meters": [{"id": "gain_reduction", "name": "GR", "unit": "dB", "index": 0}]}],
                "meters": [{"id": "level", "name": "Tone level", "unit": "dB", "index": 1}]
            }]
        }]
    });
    let chain = Chain::from_value(&doc).expect("a kit chain");
    assert!(chain.is_kit());
    assert_eq!(chain.lanes.len(), 1);
    assert_eq!(chain.lanes[0].phases[0].meters[0].index, 1);
    let tone = json!({
        "schema": "signal.chain/1",
        "phases": [{"id": "tone", "blocks": [], "meters": [{"id": "level", "name": "L", "unit": "dB", "index": 4}]}]
    });
    let chain = Chain::from_value(&tone).expect("a chain");
    assert_eq!(chain.meter_indices().collect::<Vec<_>>(), [4]);
}

#[test]
fn the_source_phase_has_no_switch_or_make_up() {
    let kit = kit();
    let (_, sample) = kit
        .iter()
        .find(|(p, _)| p == "Drums/Kick/Sum/Sample")
        .expect("sample");
    let laid = view::layout(&view::rows(sample, Fold::Open), panel());
    let (_, bar) = laid
        .iter()
        .find(|(r, _)| matches!(r, Row::Phase(p) if p.is_source()))
        .expect("the source bar");
    let switch = view::switch_box(*bar);
    assert_eq!(view::hit(&laid, switch.x + 4.0, bar.y + 4.0), None);
    let makeup = view::makeup_box(*bar);
    assert_eq!(view::hit(&laid, makeup.x + 4.0, bar.y + 4.0), None);
}

#[test]
fn a_moved_value_revision_reads_the_chain_again_at_the_same_revision() {
    use std::sync::atomic::{AtomicU64, Ordering};

    /// The fixture, plus a `values_revision` the test moves.
    struct Valued {
        inner: Arc<Fixture>,
        values: AtomicU64,
    }
    impl SignalSource for Valued {
        fn signal_chain(&self, t: &str) -> Result<Chain, SourceError> {
            self.inner.signal_chain(t)
        }
        fn signal_run(
            &self,
            t: &str,
            a: &str,
            args: &serde_json::Value,
        ) -> Result<Chain, SourceError> {
            self.inner.signal_run(t, a, args)
        }
        fn signal_session(&self, t: &str) -> Result<super::source::Session, SourceError> {
            self.inner.signal_session(t)
        }
        fn signal_algorithms(
            &self,
            t: &str,
            k: &str,
        ) -> Result<Vec<super::source::Algorithm>, SourceError> {
            self.inner.signal_algorithms(t, k)
        }
        fn signal_revisions(&self) -> Vec<(String, u64)> {
            self.inner.signal_revisions()
        }
        fn signal_values_revisions(&self) -> Vec<(String, u64)> {
            vec![(KICK.into(), self.values.load(Ordering::Relaxed))]
        }
        fn signal_meters(&self, t: &str) -> Vec<MeterValue> {
            self.inner.signal_meters(t)
        }
        fn fx_set(&self, t: &str, c: &str, v: &ParamValue) -> Result<(), SourceError> {
            self.inner.fx_set(t, c, v)
        }
    }

    let fixture = Arc::new(Fixture::new(kit()));
    let source = Arc::new(Valued {
        inner: fixture.clone(),
        values: AtomicU64::new(1),
    });
    let mut inserts = Inserts::new(Some(source.clone()));
    let reads = |f: &Fixture| {
        f.calls()
            .iter()
            .filter(|c| matches!(c, Call::Chain(t) if t == KICK))
            .count()
    };
    inserts.follow();
    assert_eq!(reads(&fixture), 1);
    inserts.follow();
    assert_eq!(reads(&fixture), 1, "values still: not read again");

    // Automation moved a value: the chain's revision stays, its values move.
    source.values.store(2, Ordering::Relaxed);
    let moved = inserts.follow();
    assert_eq!(moved.read, [KICK.to_owned()]);
    assert_eq!(reads(&fixture), 2, "read again for the plugin's value text");
    inserts.follow();
    assert_eq!(reads(&fixture), 2, "and only once");
}

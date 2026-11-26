//! A long action on the window's link holds up nothing else on it.
//!
//! The engine's view service served over an in-process vox link, as the
//! engine serves it to a window (`crate::open`'s bootstrap): vox drives
//! every call's handler inside one connection loop, so a handler that
//! blocks is a link that answers nothing else. `SLOW` stands in for a
//! render (`MIX_MEASURE`, a first `LISTEN`): a sync action that takes
//! seconds.

use std::sync::Arc;
use std::time::Duration;

use web_time::Instant;

use architect::action::{ActionMeta, Effect};

use crate::actions::meta;
use crate::view::{ActionCall, EngineViewClient};

static SLOW: ActionMeta = meta(
    "TEST_OFF_LOOP_SLOW",
    "test_off_loop_slow",
    "A render, for the test",
    "Takes as long as it is asked (`ms`), blocking.",
    "Test",
    "",
);
static QUICK: ActionMeta = meta(
    "TEST_OFF_LOOP_QUICK",
    "test_off_loop_quick",
    "A read, for the test",
    "Answers at once.",
    "Test",
    "",
);

/// How long the render runs.
const RENDER: Duration = Duration::from_millis(1_500);
/// How long any other call may take while it does.
const BOUND: Duration = Duration::from_millis(200);

#[derive(facet::Facet)]
struct SlowArg {
    ms: u64,
}

fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let list = crate::actions::list();
        list.register(&SLOW, Effect::ReadOnly, |a: SlowArg| {
            std::thread::sleep(Duration::from_millis(a.ms));
            Ok::<_, String>(a.ms)
        })
        .expect("SLOW registers");
        list.register(&QUICK, Effect::ReadOnly, |(): ()| Ok::<_, String>(7u32))
            .expect("QUICK registers");
    });
}

#[test]
fn a_call_on_the_link_answers_while_a_render_runs_on_it() {
    register();
    // The engine's runtime: two workers, as `daw_view::open` builds it.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime");
    rt.block_on(async {
        let router = crate::studio_engine::view_host::router(&daw::standalone::Standalone::new());
        let caller = architect::LocalServer::serve(router, architect::Scope::new())
            .caller()
            .await
            .expect("the view service, served");
        let view = Arc::new(EngineViewClient::new(caller));
        let render = {
            let view = Arc::clone(&view);
            tokio::spawn(async move {
                let began = Instant::now();
                let out = view
                    .run(ActionCall {
                        id: SLOW.id.to_owned(),
                        args: format!("{{\"ms\":{}}}", RENDER.as_millis()),
                    })
                    .await;
                (out, began.elapsed())
            })
        };
        // The render under way, the window asks something else.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let asked = Instant::now();
        let quick = view
            .run(ActionCall {
                id: QUICK.id.to_owned(),
                args: String::new(),
            })
            .await;
        let waited = asked.elapsed();
        let (rendered, took) = render.await.expect("the render's task");
        assert_eq!(quick.ok().as_deref(), Some("7"));
        assert!(
            waited < BOUND,
            "a quick call waited {waited:?} behind a {RENDER:?} render on the same link"
        );
        assert!(rendered.is_ok(), "the render answered: {rendered:?}");
        assert!(took >= RENDER, "the render ran its length: {took:?}");
    });
}

#[test]
fn an_async_action_and_an_unknown_one_answer_as_before() {
    register();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime");
    rt.block_on(async {
        let out = super::run(QUICK.id, b"").await.expect("QUICK runs");
        assert_eq!(out, b"7");
        let unknown = super::run("TEST_OFF_LOOP_NO_SUCH", b"").await;
        assert!(
            matches!(unknown, Err(architect::action::ActionError::Unknown(_))),
            "{unknown:?}"
        );
    });
}

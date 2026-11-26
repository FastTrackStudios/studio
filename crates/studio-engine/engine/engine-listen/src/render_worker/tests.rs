//! The render worker: where renders go, what a crash report says, and a
//! real worker (this test binary, in worker mode) rendering a test song
//! the same as the engine does, to the bit, surviving a plugin crash that
//! the engine never sees.

use std::time::Instant;

use architect::action::{ActionCaller, Invocation, InvocationSource, Principal};
use serde_json::{Value, json};

use super::client::{Failure, Spawner};
use super::crash::{Report, bundle_name, is_plugin_object};
use super::policy::{Mode, Origin, Where, decide, origin_of, strip};

#[test]
fn an_agents_render_goes_to_the_worker_and_the_apps_stays() {
    for id in super::policy::RENDERS {
        assert_eq!(
            decide(Mode::Auto, Origin::Agent, id, None, b"{}"),
            Where::Worker,
            "{id}"
        );
        assert_eq!(
            decide(Mode::Auto, Origin::App, id, None, b"{}"),
            Where::Process,
            "{id}"
        );
        assert_eq!(
            decide(Mode::Worker, Origin::App, id, None, b"{}"),
            Where::Worker,
            "{id}"
        );
        assert_eq!(
            decide(Mode::Process, Origin::Agent, id, None, b"{}"),
            Where::Process,
            "{id}"
        );
    }
}

#[test]
fn only_renders_leave_and_a_kept_ab_stays_in_the_live_song() {
    for id in [
        "LISTEN_CACHE",
        "LISTEN_HITS",
        "LISTEN_ALIGN",
        "LISTEN_LIVE",
        "TRACK_SET",
    ] {
        assert_eq!(
            decide(Mode::Worker, Origin::Agent, id, Some(Where::Worker), b"{}"),
            Where::Process,
            "{id}"
        );
    }
    let kept = br#"{"track": "Kick", "keep": true}"#;
    assert_eq!(
        decide(Mode::Worker, Origin::Agent, "LISTEN_AB", None, kept),
        Where::Process
    );
    let put_back = br#"{"track": "Kick", "keep": false}"#;
    assert_eq!(
        decide(Mode::Auto, Origin::Agent, "LISTEN_AB", None, put_back),
        Where::Worker
    );
}

#[test]
fn a_call_can_say_where_it_runs_and_who_it_is_from() {
    assert_eq!(
        decide(
            Mode::Auto,
            Origin::App,
            "LISTEN",
            Some(Where::Worker),
            b"{}"
        ),
        Where::Worker
    );
    assert_eq!(
        decide(
            Mode::Auto,
            Origin::Agent,
            "LISTEN",
            Some(Where::Process),
            b"{}"
        ),
        Where::Process
    );
    let routed = strip(br#"{"track": "Kick", "_origin": "agent", "_render_in": "process"}"#);
    assert_eq!(routed.origin, Some(Origin::Agent));
    assert_eq!(routed.asked, Some(Where::Process));
    let args: Value = serde_json::from_slice(&routed.args).expect("json");
    assert_eq!(args, json!({ "track": "Kick" }));
}

#[test]
fn arguments_without_routing_words_pass_byte_for_byte() {
    for raw in [
        &br#"{"track":"Kick" ,"from": "1"}"#[..],
        b"",
        b"{",
        b"[1,2]",
        b"\"_x\"",
    ] {
        let routed = strip(raw);
        assert_eq!(routed.args, raw, "{}", String::from_utf8_lossy(raw));
        assert_eq!((routed.origin, routed.asked), (None, None));
    }
}

#[test]
fn a_principal_or_an_agent_invocation_is_an_agent() {
    assert_eq!(origin_of(&ActionCaller::app()), Origin::App);
    assert_eq!(
        origin_of(&ActionCaller::app().with_invocation(Invocation::new(InvocationSource::Key))),
        Origin::App
    );
    assert_eq!(
        origin_of(&ActionCaller::app().with_invocation(Invocation::new(InvocationSource::Agent))),
        Origin::Agent
    );
    assert_eq!(
        origin_of(&ActionCaller::new(Principal::Anonymous)),
        Origin::Agent
    );
}

#[test]
fn modes_read_back_as_they_are_written() {
    for m in [Mode::Auto, Mode::Worker, Mode::Process] {
        assert_eq!(Mode::parse(m.name()), Some(m));
    }
    assert_eq!(Mode::parse("off"), Some(Mode::Process));
    assert_eq!(Mode::parse("nonsense"), None);
}

#[test]
fn a_crash_is_laid_at_the_innermost_plugin_bundle_on_the_stack() {
    let report = Report::parse(
        "signal=11\n\
         object=/usr/lib/libc.so.6\n\
         object=/home/u/.clap/Signal.clap\n\
         object=/home/u/.clap/Signal.clap\n\
         object=/run/x/daw-extension\n\
         object=/home/u/.vst3/Other.vst3/Contents/x86_64-linux/Other.so\n",
    );
    assert_eq!(report.signal_name(), Some("SIGSEGV"));
    assert_eq!(report.objects.len(), 4, "duplicates are dropped");
    let plugin = report.plugin().expect("a plugin");
    assert_eq!(plugin.name, "Signal");
    assert_eq!(plugin.file.as_deref(), Some("/home/u/.clap/Signal.clap"));
}

#[test]
fn what_the_render_said_names_the_plugin_when_the_stack_cannot() {
    let report = Report::parse("signal=6\nplugin=Kick/Sum: Signal\nobject=/usr/lib/libc.so.6\n");
    let plugin = report.plugin().expect("a plugin");
    assert_eq!(plugin.name, "Kick/Sum: Signal");
    assert_eq!(plugin.file, None);
    assert_eq!(
        Report::parse("signal=11\nobject=/usr/lib/libc.so.6\n").plugin(),
        None
    );
    // Both: the stack's bundle, under the render's name for it.
    let both = Report::parse("plugin=Kick: Signal\nobject=/p/Signal.clap\n");
    assert_eq!(
        both.plugin().map(|p| (p.name, p.file)),
        Some(("Kick: Signal".into(), Some("/p/Signal.clap".into())))
    );
}

#[test]
fn plugin_binaries_are_told_from_the_engines_and_the_systems() {
    assert!(is_plugin_object("/home/u/.clap/Signal.clap"));
    assert!(is_plugin_object("/x/Foo.vst3/Contents/x86_64-linux/Foo.so"));
    assert!(!is_plugin_object("/usr/lib/libc.so.6"));
    assert!(!is_plugin_object(
        "/run/media/x/target/desktop-dev/daw-extension"
    ));
    assert_eq!(
        bundle_name("/x/Foo.vst3/Contents/x86_64-linux/Foo.so"),
        "Foo"
    );
    assert_eq!(bundle_name("/home/u/.clap/Signal.clap"), "Signal");
}

#[test]
fn a_crash_answers_plugin_crashed_naming_the_plugin() {
    let f = Failure::PluginCrashed {
        plugin: Some(super::crash::Plugin {
            name: "Signal".into(),
            file: Some("/p/Signal.clap".into()),
        }),
        signal: Some("SIGSEGV".into()),
        op: "LISTEN".into(),
    };
    assert_eq!(f.code(), "plugin_crashed");
    let m = f.message();
    assert!(m.starts_with("plugin_crashed: "), "{m}");
    assert!(
        m.contains("\"Signal\"") && m.contains("SIGSEGV") && m.contains("LISTEN"),
        "{m}"
    );
}

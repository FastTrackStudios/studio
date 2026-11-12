//! The Patchbay core as an Architect app hosts it: linked in, its actions
//! on the app's list, over one engine per machine.
//!
//! The engine runs on a free loopback port with a scratch config and the
//! device layer off (`PATCHBAY_DEVICES=off`), so nothing is discovered on
//! the LAN and no saved preset can wire anything. On Linux it reads the
//! machine's `PipeWire` graph (read only: an empty config makes no
//! links).
//!
//! One test function: the engine is one per process, and the environment
//! it reads is the process's.
#![cfg(feature = "extension")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use architect::action::ActionList;
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::install::Installs;
use architect_settings::Registry;
use futures::executor::block_on;
use patchbay::server::{Engine, EngineMode, EngineOptions};
use serde_json::{Value, json};

fn run(list: &ActionList, id: &str, args: &Value) -> Result<Value, String> {
    block_on(list.run_json_async(id, args.to_string().as_bytes()))
        .map(|out| serde_json::from_slice(&out).unwrap_or(Value::Null))
        .map_err(|e| e.to_string())
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn the_core_runs_as_a_linked_extension_over_one_engine() {
    let tmp = tempfile::tempdir().unwrap();
    let addr = format!("127.0.0.1:{}", free_port());
    // SAFETY: set before anything in this process reads the environment
    // or starts a thread (nextest runs each test in a process of its own).
    unsafe {
        std::env::set_var("PATCHBAY_CONFIG", tmp.path().join("patchbay.styx"));
        std::env::set_var("PATCHBAY_ADDR", &addr);
        std::env::set_var("PATCHBAY_DEVICES", "off");
    }

    let list = ActionList::new();
    let host = Host::new(list.clone(), SharedSections::new(Registry::new()));
    let ext = Extensions::new(host, Installs::new(tmp.path().join("installs")))
        .with_linked(&patchbay::ext::Patchbay)
        .unwrap();
    ext.install_contribution_actions().unwrap();
    let _ = ext.sync();

    // Loading it starts nothing: the port is still free.
    drop(std::net::TcpListener::bind(&addr).expect("loading the extension took the port"));

    // The first action starts this machine's engine, here.
    let status = run(&list, "PATCHBAY_STATUS", &json!({})).unwrap();
    let engine = patchbay::ext::engine().unwrap();
    assert_eq!(
        engine.mode(),
        &EngineMode::Owned {
            addr: addr.clone(),
            serving: true
        }
    );

    // A second app on this machine attaches to it instead of running a
    // second engine.
    let mut opts = EngineOptions::new(addr.clone());
    opts.discover_peers = false;
    let other = Engine::start_or_attach(opts).unwrap();
    assert_eq!(
        other.mode(),
        &EngineMode::Attached {
            url: format!("ws://{addr}/vox")
        }
    );

    // One engine: what one writes, the other reads.
    block_on(other.run(|c| async move { c.set_alias("probe.node".into(), "Probe".into()).await }))
        .unwrap()
        .unwrap();
    let aliases = block_on(engine.run(|c| async move { c.aliases().await }))
        .unwrap()
        .unwrap();
    assert!(
        aliases
            .iter()
            .any(|a| a.target == "probe.node" && a.alias == "Probe"),
        "the alias set through the attached client is not in the engine: {aliases:?}"
    );

    // The actions answer the JSON `patchbay … --json` prints, which a
    // client over ws (the CLI's link) builds the same way. Graph counts
    // can move between two reads of a live graph, so compare its shape
    // and the parts that do not.
    let cli_status = block_on(other.run(|c| async move { patchbay::report::status(&c).await }))
        .unwrap()
        .unwrap();
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&status), keys(&cli_status));
    assert_eq!(status["dante_active"], cli_status["dante_active"]);

    let devices = run(&list, "PATCHBAY_DEVICE_LIST", &json!({})).unwrap();
    let cli_devices = block_on(other.run(|c| async move { c.list_devices().await }))
        .unwrap()
        .unwrap();
    assert_eq!(devices, serde_json::to_value(cli_devices).unwrap());

    let health = run(&list, "PATCHBAY_HEALTH", &json!({})).unwrap();
    for key in ["ok", "graph", "clock", "dante", "services", "issues"] {
        assert!(health.get(key).is_some(), "health has no `{key}`: {health}");
    }
    let now = run(&list, "PATCHBAY_NOW", &json!({})).unwrap();
    assert!(now.get("supported").is_some(), "now: {now}");

    let err = run(&list, "PATCHBAY_DEVICE_SHOW", &json!({})).unwrap_err();
    assert!(err.contains("give `id`"), "{err}");
    assert!(run(&list, "PATCHBAY_DEVICE_SHOW", &json!({"id": "nope"})).is_err());
}

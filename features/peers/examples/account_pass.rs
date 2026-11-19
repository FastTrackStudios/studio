//! One account pass against a real issuer, as this machine's Studio engine
//! would make it, printing who the account's other engines are.
//!
//! `cargo run -p studio-peers --features mesh --example account_pass --
//! <session.json> [issuer]` (default issuer `https://auth.fasttrackstudio.app`).
//! The token is read from the session file and never printed.
//! `--forget <install-id> <name> <platform>` instead takes that engine's
//! row off the account (a test engine's, say).

#![allow(clippy::print_stdout, clippy::expect_used, clippy::unwrap_used)]

use studio_peers::About;
use studio_peers::mesh::{Mesh, bind, issuer_vox, this_engine};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let session = args.next().expect("a session.json");
    let issuer = args
        .next()
        .unwrap_or_else(|| "https://auth.fasttrackstudio.app".into());
    let raw = std::fs::read_to_string(&session).expect("the session file");
    let field = |name: &str| {
        let at = raw.find(&format!("\"{name}\"")).expect("the field")
            + name.len()
            + 2;
        let rest = raw[at..].trim_start_matches([':', ' ']);
        let rest = rest.strip_prefix('"').unwrap();
        rest[..rest.find('"').unwrap()].to_owned()
    };
    let token = field("token");
    let account = field("user_id");
    let rest: Vec<String> = std::env::args().skip(3).collect();
    if let [flag, install, name, platform] = rest.as_slice() {
        assert_eq!(flag, "--forget");
        let pool = architect::connect::Pool::new();
        let devices: auth_proto::DeviceServiceClient = pool
            .client(&architect::connect::Endpoint::url(issuer_vox(&issuer)))
            .await
            .expect("the issuer");
        let me = this_engine(install.parse().unwrap(), &account, name, platform);
        let outcome = devices.forget_device(token, me.device_id).await;
        println!("forget {}: {}", me.device_id, if outcome.is_ok() { "ok" } else { "refused" });
        return;
    }
    let endpoint = bind(iroh::SecretKey::generate(), "account-pass probe")
        .await
        .unwrap();
    let mesh = Mesh::start(endpoint, About::default(), None).await.unwrap();
    let me = this_engine(
        uuid::Uuid::new_v4(),
        &account,
        "account-pass probe",
        "linux",
    );
    let pool = architect::connect::Pool::new();
    let devices: auth_proto::DeviceServiceClient = pool
        .client(&architect::connect::Endpoint::url(issuer_vox(&issuer)))
        .await
        .expect("the issuer");
    match mesh.account_pass(&devices, &token, &me).await {
        Ok(report) => {
            println!("pass ok: org {:?}", report.org.map(|o| o.slug));
            for d in report.others {
                println!(
                    "  {} ({}) endpoint {}",
                    d.name,
                    d.platform,
                    d.endpoint_id.as_deref().unwrap_or("-")
                );
            }
            // Leave no probe row behind.
            let _ = devices.forget_device(token, me.device_id).await;
        }
        Err(e) => println!("pass failed: {e}"),
    }
}

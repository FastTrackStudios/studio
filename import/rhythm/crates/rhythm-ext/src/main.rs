use std::io::{self, BufRead, Write};

use clap::{Parser, Subcommand};
use rhythm_ext::{DetectRequest, PieceFileSpec, dispatch};
use rhythm_hits::DetectorRegistry;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

#[derive(Parser)]
#[command(name = "fts-rhythm", about = "FastTrackStudio swappable hit-detection & rhythm-editing extension")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve newline-delimited JSON-RPC over stdin/stdout (process extension mode).
    Serve,
    /// Print available detectors and editing strategies.
    State,
    /// Detect hits on one or more `PieceName=path.wav[:trigger.wav]` specs.
    Detect {
        #[arg(long, default_value = "fts.hits.kit-fusion")]
        detector: String,
        #[arg(required = true)]
        specs: Vec<String>,
    },
}

#[derive(Deserialize)]
struct RpcRequest {
    id: Json,
    action: String,
    #[serde(default)]
    params: Json,
}

#[derive(Serialize)]
struct RpcResponse {
    id: Json,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Json>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let registry = DetectorRegistry::default();

    match cli.cmd {
        Cmd::State => {
            let out = dispatch(&registry, "RHYTHM_STATE", Json::Null).expect("state");
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        Cmd::Detect { detector, specs } => {
            let mut pieces = Vec::new();
            for s in specs {
                let Some((name, paths)) = s.split_once('=') else {
                    eprintln!("invalid spec '{s}', expected Name=mic.wav[:trigger.wav]");
                    std::process::exit(2);
                };
                let (mic_wav, trigger_wav) = match paths.split_once(':') {
                    Some((m, t)) => (m.to_owned(), Some(t.to_owned())),
                    None => (paths.to_owned(), None),
                };
                pieces.push(PieceFileSpec {
                    name: name.to_owned(),
                    mic_wav,
                    trigger_wav,
                });
            }
            let req = DetectRequest {
                detector,
                pieces,
                extra_mic_wavs: Vec::new(),
                threshold_db: None,
                crest_db: 10.0,
                retrig_ms: 30.0,
            };
            match dispatch(&registry, "RHYTHM_DETECT", serde_json::to_value(req).unwrap()) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Cmd::Serve => {
            let stdin = io::stdin();
            let mut stdout = io::stdout().lock();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let resp = match serde_json::from_str::<RpcRequest>(trimmed) {
                    Ok(req) => match dispatch(&registry, &req.action, req.params) {
                        Ok(result) => RpcResponse {
                            id: req.id,
                            result: Some(result),
                            error: None,
                        },
                        Err(e) => RpcResponse {
                            id: req.id,
                            result: None,
                            error: Some(e),
                        },
                    },
                    Err(e) => RpcResponse {
                        id: json!(null),
                        result: None,
                        error: Some(format!("bad json-rpc request: {e}")),
                    },
                };
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                let _ = stdout.flush();
            }
        }
    }
}

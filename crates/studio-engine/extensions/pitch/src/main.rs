use std::io::{self, BufRead, Write};

use clap::{Parser, Subcommand};
use pitch_core::PitchRegistry;
use pitch_ext::{AnalyzeRequest, dispatch};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

#[derive(Parser)]
#[command(
    name = "fts-pitch",
    about = "FastTrackStudio swappable pitch-detection & ARA-style pitch-editing extension"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve newline-delimited JSON-RPC over stdin/stdout (process extension mode).
    Serve,
    /// Print available detectors, segmenters, and synthesizers.
    State,
    /// Analyze a WAV file and print its PitchAnalysisDoc JSON.
    Analyze {
        wav: String,
        #[arg(long, default_value = "fts.pitch.yin")]
        detector: String,
        #[arg(long, default_value = "fts.segment.contour")]
        segmenter: String,
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
    let registry = PitchRegistry::default();

    match cli.cmd {
        Cmd::State => {
            let out = dispatch(&registry, "PITCH_STATE", Json::Null).expect("state");
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        Cmd::Analyze {
            wav,
            detector,
            segmenter,
        } => {
            let req = AnalyzeRequest {
                wav,
                detector,
                segmenter,
            };
            match dispatch(
                &registry,
                "PITCH_ANALYZE",
                serde_json::to_value(req).unwrap(),
            ) {
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

use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

use clap::{Parser, Subcommand};
use stem_seperation::{Backend, Job};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the selected backend executable.
    #[arg(long, global = true)]
    executable: Option<OsString>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Evaluate exported stems against reference WAVs.
    Bench {
        #[command(subcommand)]
        command: BenchAction,
    },
    /// List audio-separator models or common Demucs presets.
    Models {
        #[arg(value_enum)]
        backend: Engine,
    },
    /// Separate a recording using the audio-separator model catalog.
    AudioSeparator {
        input: PathBuf,
        #[arg(long)]
        model: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        model_dir: Option<PathBuf>,
        #[arg(long)]
        single_stem: Option<String>,
    },
    /// Separate a recording using Demucs.
    Demucs {
        input: PathBuf,
        #[arg(long, default_value = "htdemucs")]
        model: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        two_stems: Option<String>,
    },
}

#[derive(Subcommand)]
enum BenchAction {
    /// Score the WAVs listed in a benchmark manifest.
    Score {
        manifest: PathBuf,
        /// New directory for scores.json and scores.csv.
        #[arg(long)]
        output: PathBuf,
        /// Python interpreter with museval installed.
        #[arg(long, default_value = "python3")]
        python: OsString,
        /// Score full-track SDR only, skipping the more expensive BSS Eval.
        #[arg(long)]
        global_only: bool,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum Engine {
    AudioSeparator,
    Demucs,
}

fn main() -> io::Result<()> {
    let Cli {
        executable,
        command,
    } = Cli::parse();
    match command {
        Action::Bench {
            command:
                BenchAction::Score {
                    manifest,
                    output,
                    python,
                    global_only,
                },
        } => {
            let mut scorer = Command::new(python);
            scorer
                .arg("-c")
                .arg(include_str!("../bench/score.py"))
                .arg(manifest)
                .arg(output);
            if global_only {
                scorer.arg("--global-only");
            }
            let status = scorer.status()?;
            if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "benchmark scorer exited with {status}"
                )))
            }
        }
        Action::Models { backend } => {
            let backend = match backend {
                Engine::AudioSeparator => Backend::AudioSeparator {
                    model_dir: None,
                    single_stem: None,
                },
                Engine::Demucs => {
                    return io::stdout().write_all(b"htdemucs\nhtdemucs_ft\nhtdemucs_6s\n");
                }
            };
            let executable = executable.unwrap_or_else(|| backend.executable().into());
            let result = backend.models(&executable)?;
            io::stdout().write_all(&result.stdout)
        }
        Action::AudioSeparator {
            input,
            model,
            output,
            model_dir,
            single_stem,
        } => separate(
            Backend::AudioSeparator {
                model_dir,
                single_stem,
            },
            executable,
            input,
            model,
            output,
        ),
        Action::Demucs {
            input,
            model,
            output,
            device,
            two_stems,
        } => separate(
            Backend::Demucs { device, two_stems },
            executable,
            input,
            model,
            output,
        ),
    }
}

fn separate(
    backend: Backend,
    executable: Option<OsString>,
    input: PathBuf,
    model: String,
    output: PathBuf,
) -> io::Result<()> {
    let executable = executable.unwrap_or_else(|| backend.executable().into());
    let stems = backend.separate(
        &executable,
        &Job {
            input: &input,
            model: &model,
            output_dir: &output,
        },
    )?;
    let mut stdout = io::stdout().lock();
    for stem in stems {
        writeln!(stdout, "{}", stem.display())?;
    }
    Ok(())
}

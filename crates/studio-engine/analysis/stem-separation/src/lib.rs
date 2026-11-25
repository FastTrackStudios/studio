//! Offline, file-based stem separation with swappable inference backends.

use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Clone)]
pub enum Backend {
    AudioSeparator {
        model_dir: Option<PathBuf>,
        single_stem: Option<String>,
    },
    Demucs {
        device: Option<String>,
        two_stems: Option<String>,
    },
}

#[derive(Debug)]
pub struct Job<'a> {
    pub input: &'a Path,
    /// A new directory for one run. Its parent must already exist.
    pub output_dir: &'a Path,
    pub model: &'a str,
}

impl Backend {
    #[must_use]
    pub fn executable(&self) -> &'static str {
        match self {
            Self::AudioSeparator { .. } => "audio-separator",
            Self::Demucs { .. } => "demucs",
        }
    }

    /// Runs inference and returns the audio files under the new output directory.
    /// A custom executable can point to a backend installed in a virtual environment.
    /// Failed runs remove their reserved output directory.
    pub fn separate(&self, executable: &OsStr, job: &Job<'_>) -> io::Result<Vec<PathBuf>> {
        let input = job.input.canonicalize()?;
        if !input.is_file() || !valid_model(job.model) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected an input file and a plain model filename or identifier",
            ));
        }
        fs::create_dir(job.output_dir)?;
        let output = tempfile::TempDir::new_in(job.output_dir);
        let result = output.and_then(|output| {
            run(self.command(executable, job.model, &input, output.path()))?;
            let mut paths = Vec::new();
            collect_audio(output.path(), Path::new(""), &mut paths)?;
            if paths.is_empty() {
                return Err(io::Error::other("backend produced no audio files"));
            }
            paths.sort();
            for path in &paths {
                let dest = job.output_dir.join(path);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(output.path().join(path), dest)?;
            }
            Ok(paths.into_iter().map(|p| job.output_dir.join(p)).collect())
        });
        if result.is_err() {
            let _ = fs::remove_dir_all(job.output_dir);
        }
        result
    }

    /// Returns the catalog exposed by the installed backend.
    pub fn models(&self, executable: &OsStr) -> io::Result<Output> {
        if matches!(self, Self::Demucs { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Demucs v4.0.1 does not provide a model listing command",
            ));
        }
        let mut command = Command::new(executable);
        command.arg("--list_models");
        run(command)
    }

    fn command(&self, executable: &OsStr, model: &str, input: &Path, out: &Path) -> Command {
        let mut command = Command::new(executable);
        match self {
            Self::AudioSeparator {
                model_dir,
                single_stem,
            } => {
                command.args(["--model_filename", model, "--output_format", "WAV"]);
                command.arg("--output_dir").arg(out);
                if let Some(dir) = model_dir {
                    command.arg("--model_file_dir").arg(dir);
                }
                if let Some(stem) = single_stem {
                    command.arg(format!("--single_stem={stem}"));
                }
            }
            Self::Demucs { device, two_stems } => {
                command.args(["-n", model]).arg("--out").arg(out);
                if let Some(device) = device {
                    command.arg(format!("--device={device}"));
                }
                if let Some(stem) = two_stems {
                    command.arg(format!("--two-stems={stem}"));
                }
            }
        }
        command.arg(input);
        command
    }
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && !model.starts_with('-')
        && model != "."
        && model != ".."
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

fn run(mut command: Command) -> io::Result<Output> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command.output().map_err(|error| {
        io::Error::new(error.kind(), format!("could not launch {program}: {error}"))
    })?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "{program} exited with {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        )));
    }
    Ok(output)
}

fn collect_audio(root: &Path, relative: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_audio(root, &path, paths)?;
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
            && entry.metadata()?.len() > 0
        {
            paths.push(path);
        }
    }
    Ok(())
}

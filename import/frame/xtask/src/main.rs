use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(task) = args.next() else {
        print_usage();
        return Ok(());
    };
    let passthrough: Vec<String> = args.collect();

    let workspace_root = workspace_root()?;

    match task.as_str() {
        "parity_test" | "partiy_test" => run_script(
            &workspace_root,
            workspace_root.join("tools/run_parity_render_and_diff.sh"),
            &passthrough,
        ),
        "parity_render" => run_script(
            &workspace_root,
            workspace_root.join("tools/render_parity_actuals.sh"),
            &passthrough,
        ),
        "parity_diff" => run_script(
            &workspace_root,
            workspace_root.join("tools/run_parity_scenarios.sh"),
            &passthrough,
        ),
        "parity_svg" => run_script(
            &workspace_root,
            workspace_root.join("tools/run_svg_parity_scenarios.sh"),
            &passthrough,
        ),
        "parity_svg_analyze" => run_script(
            &workspace_root,
            workspace_root.join("tools/analyze_svg_canonical_precision.sh"),
            &passthrough,
        ),
        "parity_svg_hash" => run_script(
            &workspace_root,
            workspace_root.join("tools/check_svg_hash_baselines.sh"),
            &passthrough,
        ),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => Err(format!("unknown xtask command: {other}")),
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "failed resolving workspace root".to_string())
}

fn run_script(root: &Path, script: PathBuf, args: &[String]) -> Result<(), String> {
    if !script.exists() {
        return Err(format!("script not found: {}", script.display()));
    }

    let status = Command::new(&script)
        .args(args)
        .current_dir(root)
        .env("CARGO", env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .status()
        .map_err(|e| format!("failed to run {}: {e}", script.display()))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} failed with status {}",
            script.display(),
            status
        ))
    }
}

fn print_usage() {
    eprintln!(
        "usage: cargo xtask <command> [args...]

commands:
  parity_test   render actual PNGs + run parity diffs/reports
  partiy_test   typo alias for parity_test
  parity_render render actual PNGs only
  parity_diff   run parity diff/report only
  parity_svg    run SVG strict/canonical parity summary
  parity_svg_analyze analyze canonical diff sensitivity by precision
  parity_svg_hash verify approved SVG SHA-256 baselines

examples:
  cargo xtask parity_test --all --threshold 0.0
  cargo xtask parity_test --scenario paint.gradient-matrix --threshold 0.0
  cargo xtask parity_render --all
  cargo xtask parity_diff --all --threshold 0.0
  cargo xtask parity_svg --all
  cargo xtask parity_svg_analyze --scenario text.mixed-style-runs
  cargo xtask parity_svg_hash"
    );
}

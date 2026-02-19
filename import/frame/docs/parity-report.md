# Figma Parity Report

Generate a consolidated parity artifact from:
- feature coverage (`fts-figma-export` payload),
- optional visual acceptance results,
- optional performance snapshot JSON.

## Command

```bash
tools/run_parity_pipeline.sh \
  tests/frame/parity/scenarios/paint.alpha-steps/export/latest-bridge-export.json \
  --out-dir tests/frame/parity/reports/paint.alpha-steps \
  --actual-png-dir /path/to/actual \
  --expected-png-dir /path/to/expected \
  --threshold 0.0 \
  --require-pass
```

Run the full parity pack (or a single scenario) via:

```bash
tools/run_parity_scenarios.sh --threshold 0.0
tools/run_parity_scenarios.sh --scenario paint.alpha-steps --threshold 0.0
```

Manual split flow is still available via:
- `tools/capture_render_diagnostics.sh`
- `tools/capture_import_diagnostics.sh`
- `tools/run_visual_acceptance.sh`
- `tools/generate_parity_report.sh`
- `tools/run_parity_scenarios.sh`

## Output

- JSON report:
  - `featureCoverage`
  - `visualAcceptance` (if provided)
  - `performance` (if provided)
  - `renderDiagnostics` (if provided)
  - `importDiagnostics` (if provided)
  - `gates` summary
- Markdown report suitable for bead closure evidence.
- `--require-pass` makes pipeline exit non-zero when `gates.parityPass` is `false` (CI-friendly).

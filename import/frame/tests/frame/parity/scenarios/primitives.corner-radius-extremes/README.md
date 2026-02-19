# primitives.corner-radius-extremes

## What it tests
Corner radius correctness including per-corner and clamped extremes.

## Setup
1. Create a Figma frame named `parity/primitives/corner-radius-extremes`.
2. Add these contents:
- Row A: rectangles with radii 0, 4, 16, 64.
- Row B: asymmetric per-corner radii.
- Row C: radius values near or above half-size clamp thresholds.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Rounding math differences create obvious visual mismatch and are easy to regress when optimizing geometry paths.

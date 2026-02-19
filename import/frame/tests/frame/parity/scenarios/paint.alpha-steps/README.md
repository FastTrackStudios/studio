# paint.alpha-steps

## What it tests
Alpha compositing accuracy across very low to full opacity.

## Setup
1. Create a Figma frame named `parity/paint/alpha-steps`.
2. Add these contents:
- Row A: swatches at alpha 1.0, 0.75, 0.5, 0.25, 0.01.
- Row B: same swatches over checkerboard backdrop.
- Row C: nested parent/child opacity combinations.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Opacity math must match exactly for reliable pixel parity in layered UI compositions.

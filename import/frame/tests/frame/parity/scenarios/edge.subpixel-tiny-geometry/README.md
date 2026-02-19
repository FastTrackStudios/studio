# edge.subpixel-tiny-geometry

## What it tests
Very small geometry and subpixel rasterization edge cases.

## Setup
1. Create a Figma frame named `parity/edge/subpixel-tiny-geometry`.
2. Add these contents:
- Row A: shapes below 1px and near 1px sizes.
- Row B: tiny strokes at fractional coordinates.
- Row C: dense tiny pattern block.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
This scenario catches precision and antialias behavior mismatches.

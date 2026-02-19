# primitives.basic-shapes

## What it tests
Integer-aligned rectangle, ellipse, line, star, and polygon rendering plus overlap order.

## Setup
1. Create a Figma frame named `parity/primitives/basic-shapes`.
2. Add these contents:
- Row A: one of each primitive at integer coordinates.
- Row B: same shapes with alternate fill and stroke styles.
- Row C: overlapping stack to validate draw order.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
This is the baseline geometry sanity check; failures here usually indicate core path, fill, or ordering regressions.

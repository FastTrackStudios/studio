# stress.many-siblings

## What it tests
Performance and draw-order correctness with high sibling counts.

## Setup
1. Create a Figma frame named `parity/stress/many-siblings`.
2. Add these contents:
- Row A: 200+ siblings in one parent.
- Row B: mixed node types with reused styles.
- Row C: overlapping siblings stressing order and blending.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Large sibling sets approximate real complex artboards and stress batching paths.

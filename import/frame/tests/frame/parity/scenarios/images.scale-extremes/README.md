# images.scale-extremes

## What it tests
Image quality and placement under strong up/down scaling.

## Setup
1. Create a Figma frame named `parity/images/scale-extremes`.
2. Add these contents:
- Row A: very large source image scaled down heavily.
- Row B: very small source image scaled up heavily.
- Row C: non-uniform scale plus crop offsets.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Scale/crop transforms frequently expose coordinate and sampling differences.

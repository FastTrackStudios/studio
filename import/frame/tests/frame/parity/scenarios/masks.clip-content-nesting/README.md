# masks.clip-content-nesting

## What it tests
Clipping behavior including nested clip chains.

## Setup
1. Create a Figma frame named `parity/masks/clip-content-nesting`.
2. Add these contents:
- Row A: single clip-content container with overflow.
- Row B: two-level nested clip containers.
- Row C: clipped content with transform applied.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Clip propagation errors quickly break many production layouts.

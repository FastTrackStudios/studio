# images.alpha-over-gradient

## What it tests
Transparent image compositing over gradient backgrounds.

## Setup
1. Create a Figma frame named `parity/images/alpha-over-gradient`.
2. Add these contents:
- Row A: transparent PNG over linear gradient.
- Row B: transparent PNG over radial gradient.
- Row C: stacked transparent images over blend layers.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Alpha premultiplication mismatches show up clearly in this setup.

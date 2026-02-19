# effects.shadow-matrix

## What it tests
Drop and inner shadow rendering across blur/radius/offset ranges.

## Setup
1. Create a Figma frame named `parity/effects/shadow-matrix`.
2. Add these contents:
- Row A: drop shadows at small, medium, and large blur.
- Row B: inner shadows at small, medium, and large blur.
- Row C: multiple shadows on one node with different colors/opacities.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Shadow kernels and ordering are often approximated; this scenario verifies exact behavior.

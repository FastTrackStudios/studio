# regression.real-screen-1

## What it tests
A production-like screen fixture representing current real UI complexity.

## Setup
1. Create a Figma frame named `parity/regression/real-screen-1`.
2. Add these contents:
- Build from an actual app screen with text, images, gradients, effects, and auto-layout.
- Freeze image/font assets used by the screen.
- Keep frame stable and deterministic between exports.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Real-screen fixtures guard against regressions that synthetic micro-cases miss.

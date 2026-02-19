# text.alignment-truncation

## What it tests
Text alignment and overflow/truncation in constrained containers.

## Setup
1. Create a Figma frame named `parity/text/alignment-truncation`.
2. Add these contents:
- Row A: left/center/right and top/middle/bottom alignment set.
- Row B: truncation and overflow examples.
- Row C: aligned text in auto-layout containers.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Alignment and clipping issues are common when constraint logic changes.

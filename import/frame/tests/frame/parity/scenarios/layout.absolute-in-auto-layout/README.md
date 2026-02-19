# layout.absolute-in-auto-layout

## What it tests
Absolute-position children inside auto-layout parents.

## Setup
1. Create a Figma frame named `parity/layout/absolute-in-auto-layout`.
2. Add these contents:
- Row A: single absolute child inside auto-layout.
- Row B: multiple absolute children with z-order overlaps.
- Row C: absolute child with transform applied.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Absolute positioning inside managed layouts is a known edge case.

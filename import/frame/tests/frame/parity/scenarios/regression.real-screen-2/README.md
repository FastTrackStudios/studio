# regression.real-screen-2

## What it tests
A second production-like screen with different composition patterns.

## Setup
1. Create a Figma frame named `parity/regression/real-screen-2`.
2. Add these contents:
- Use a different real app screen than real-screen-1.
- Include at least one mask and one blur.
- Include at least one transformed group.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Multiple real-world references reduce overfitting to one screen style.

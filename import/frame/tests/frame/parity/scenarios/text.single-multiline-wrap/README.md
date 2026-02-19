# text.single-multiline-wrap

## What it tests
Basic text shaping, line breaks, and wrapping behavior.

## Setup
1. Create a Figma frame named `parity/text/single-multiline-wrap`.
2. Add these contents:
- Row A: single-line labels at varied sizes.
- Row B: multiline paragraphs in fixed-width containers.
- Row C: mixed punctuation and long strings.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Text layout differences are highly visible and impact nearly every screen.

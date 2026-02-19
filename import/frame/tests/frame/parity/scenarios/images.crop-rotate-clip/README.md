# images.crop-rotate-clip

## What it tests
Combined crop, rotation, and clipping behavior for image nodes.

## Setup
1. Create a Figma frame named `parity/images/crop-rotate-clip`.
2. Add these contents:
- Row A: cropped images with known focal points.
- Row B: rotated image inside a clip frame.
- Row C: crop + rotate + rounded clip.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
This validates transform origin and clip-space correctness for images.

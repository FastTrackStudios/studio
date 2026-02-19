# FTS Figma Export Plugin

Exports selected Figma layers/frames as a high-fidelity JSON payload intended for Frame/AnyRender ingestion.

## What it exports

- Full selected node hierarchy (children preserved)
- Layout + geometry + style metadata (fills, strokes, effects, auto-layout, transforms)
- Text content and text layout metadata
- Optional per-node SVG (base64) for vector-faithful rendering
- Optional per-node PNG (base64) as fallback raster

Schema tags:
- `fts.figma.export/v1` (legacy)
- `fts.figma.export/v2` (current, deduped assets table + node asset refs)

## Setup (pnpm)

```bash
cd /Users/codywright/Documents/Development/Rust/roam-test/packages/fts-figma-import
pnpm install
pnpm build
```

During development:

```bash
pnpm watch
```

## Use in Figma

1. In Figma desktop: `Plugins -> Development -> Import plugin from manifest...`
2. Select `/Users/codywright/Documents/Development/Rust/roam-test/packages/fts-figma-import/manifest.json`
3. Select one or more frame/layer roots.
4. Click `Export Now`.
5. Use `Copy JSON` to paste directly into your app/import UI, or `Download JSON` for file-based import.

## Write directly to `/tmp` (bridge mode)

Figma plugins cannot write local files directly. Use the local bridge process:

```bash
cd /Users/codywright/Documents/Development/Rust/roam-test/packages/fts-figma-import
pnpm bridge
```

Then in the plugin:

1. Keep `Bridge URL` as `http://localhost:43123/figma-export`
2. Click `Send to /tmp Bridge`

This writes to:

`/tmp/fts-figma-export.json`

You can point the playground importer at that path and re-send anytime without re-pasting.

## Live sync mode

The plugin supports a lightweight session-level live mode:

- `Enable Live Sync` re-runs export when selection or document changes.
- The latest payload remains in the text area and can be copied any time.

This gives a real-time authoring loop while you continue iterating in Figma.

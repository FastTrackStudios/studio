---
name: ui-design-color-theming
description: "Use when picking or reviewing UI colors, palettes, tonal ramps, accents, semantic colors, dark mode, light/dark theming, or chart series colors."
---

# UI color and theming

Distilled from the STA-518 catalogue of UI design tips (YouTube creators @whosajid and kolejain). Treat as strong defaults, not law: apply what fits the product, and say when you depart from them.

## Rules of thumb

- Start from one primary hue and build a full tonal ramp (HSL or OKLCH: vary saturation/lightness). Keep the palette to one or two accents; tint neutrals toward a single hue. Prefer neutrals plus one accent over decorative gradients.
- Use semantic colors (success, warning, destructive) only for meaning and keep them fixed across brand themes. Use color to rank competing buttons; reserve the accent for the primary action.
- Product UIs need layered backgrounds (app frame slightly darker than content; cards with a faint edge, not a black border). Match perceived lightness across hues using OKLCH.
- Theme through CSS variables so light/dark is a lightness flip; respect the system color scheme.
- Dark mode is not an inverted light mode: raised surfaces get lighter (not shadows), space surface tones about twice as far apart, lower border contrast between surfaces but brighten borders against the background, dim text, dim/desaturate bright chips, raise accent lightness, use rich hues instead of pure gray.
- Over images use a gradient overlay for readable text; derive palettes from the hero image; add subtle noise for texture.
- Charts: step series hues evenly at matched lightness and chroma; use color only where the data justifies it. Do not accept AI-default palettes.

## Full list

`references/tips.md` holds every catalogued tip in this area with its rationale, grouped by source video. Read it when you need the reasoning or a specific case. The original catalogue is the `ui-design-tips-catalogue` document on STA-531. Sixteen @whosajid videos are not yet catalogued (YouTube IP block), so coverage is partial.

## In this repo

Studio's UI renders through Blitz (CLAUDE.md, "GUI rendering"), so where a tip assumes a browser, the `blitz-design` skill wins: inline styles only, no `position: fixed`, no `<dialog>`, `<details>`, `<datalist>`, `<progress>`, `<meter>` or `<select>`, no `placeholder`, no `text-overflow`, no CSS `fill`/`stroke` on SVG. Build those from `div`s, as the skill shows. Signal Touch's tokens and sizes (`app_shell::tokens`, `docs/signal/DESIGN.md`) are the palette and the scale; a tip about colour or spacing refines them rather than replacing them.

Copied from the fleet's Paperclip skill library (company Starcommand, `ui-design-color-theming`, revision 2, 2026-10-27). Paperclip holds the source: change it there and copy it again, so the two do not drift.

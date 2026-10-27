---
name: ui-design-motion
description: "Use when adding or reviewing animation and micro-interactions: easing, perspective, scroll reveals, feedback, morphs, hover effects."
---

# UI motion and micro-interactions

Distilled from the STA-518 catalogue of UI design tips (YouTube creators @whosajid and kolejain). Treat as strong defaults, not law: apply what fits the product, and say when you depart from them.

## Rules of thumb

- Favor small, tasteful motion; keep elements static when motion adds nothing; make motion show a real state change. Give every user action (including keyboard) a visible response.
- Timing: ease-in-out for natural movement, custom cubic-bezier when defaults feel flat, transition highlights rather than jumping.
- Feedback patterns: confirm copy/save/add-to-cart with micro-interactions (e.g. fly the item to the cart, update count after it lands, collapse into a toast), shake invalid inputs and confirm success with the same pattern, progress toward commitment in pull-to-refresh and swipe-to-delete.
- Entrances: simple load animations, scroll-triggered section reveals, sequenced headline text, deliberate illustration entrances; use parallax only within generous margins.
- 3D and morphs: set perspective to make rotation believable and control intensity; keep point counts matched when morphing shapes; reveal text only after a morph finishes; draw outlines with stroke animation.
- Use the eye's focal point to guide motion; a repeated motif can serve as brand identity. Trigger animated controls from real buttons and respect hover-capable devices.

## Full list

`references/tips.md` holds every catalogued tip in this area with its rationale, grouped by source video. Read it when you need the reasoning or a specific case. The original catalogue is the `ui-design-tips-catalogue` document on STA-531. Sixteen @whosajid videos are not yet catalogued (YouTube IP block), so coverage is partial.

## In this repo

Studio's UI renders through Blitz (CLAUDE.md, "GUI rendering"), so where a tip assumes a browser, the `blitz-design` skill wins: inline styles only, no `position: fixed`, no `<dialog>`, `<details>`, `<datalist>`, `<progress>`, `<meter>` or `<select>`, no `placeholder`, no `text-overflow`, no CSS `fill`/`stroke` on SVG. Build those from `div`s, as the skill shows. Signal Touch's tokens and sizes (`app_shell::tokens`, `docs/signal/DESIGN.md`) are the palette and the scale; a tip about colour or spacing refines them rather than replacing them.

Copied from the fleet's Paperclip skill library (company Starcommand, `ui-design-motion`, revision 2, 2026-10-27). Paperclip holds the source: change it there and copy it again, so the two do not drift.

---
name: ui-design-components-interaction
description: "Use when designing or reviewing UI components and interaction patterns: buttons, inputs, menus, popovers, shadows, icons, undo/autosave, native HTML elements, tap targets and accessibility."
---

# UI components and interaction patterns

Distilled from the STA-518 catalogue of UI design tips (YouTube creators @whosajid and kolejain). Treat as strong defaults, not law: apply what fits the product, and say when you depart from them.

## Rules of thumb

- Buttons need default, hover, pressed, disabled and loading states; inputs need focus, error and warning states. Use ghost buttons for secondary actions. Make buttons look like buttons and links like links; show selected/inactive/checked states beyond color alone.
- Shadows: soft in light mode, stronger on popovers than cards. Match corner rounding across images and buttons. Size icons to line height, use one consistent icon library, replace emojis with real icons.
- Prefer undo toasts to confirm modals for reversible actions; keep warning modals for novel or irreversible ones; autosave instead of save/cancel; commit press-and-release buttons on release; open popover buttons on press.
- Menus: keep submenus open on diagonal pointer movement, show adjacent tooltips instantly after the first, expand collapsed sidebars only on dwell, preview drop position when reordering.
- Keep search prominent and reachable; collapse advanced form options; use modals for sparse create forms; collapse card actions into an overflow menu.
- Follow established conventions; refine proven patterns rather than inventing new ones.
- Use native HTML where it fits: dialog, details/summary, datalist, progress/meter, SVG icons.
- Accessibility: tap targets at least 44px, tooltips on icons and ambiguous labels, rem units for text, disable (don't hide) unavailable sections, hover effects only on hover-capable devices, animated controls triggered from a real button.

## Full list

`references/tips.md` holds every catalogued tip in this area with its rationale, grouped by source video. Read it when you need the reasoning or a specific case. The original catalogue is the `ui-design-tips-catalogue` document on STA-531. Sixteen @whosajid videos are not yet catalogued (YouTube IP block), so coverage is partial.

## In this repo

Studio's UI renders through Blitz (CLAUDE.md, "GUI rendering"), so where a tip assumes a browser, the `blitz-design` skill wins: inline styles only, no `position: fixed`, no `<dialog>`, `<details>`, `<datalist>`, `<progress>`, `<meter>` or `<select>`, no `placeholder`, no `text-overflow`, no CSS `fill`/`stroke` on SVG. Build those from `div`s, as the skill shows. Signal Touch's tokens and sizes (`app_shell::tokens`, `docs/signal/DESIGN.md`) are the palette and the scale; a tip about colour or spacing refines them rather than replacing them.

Copied from the fleet's Paperclip skill library (company Starcommand, `ui-design-components-interaction`, revision 2, 2026-10-27). Paperclip holds the source: change it there and copy it again, so the two do not drift.

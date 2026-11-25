---
"studio-engine" = "minor"
---

LISTEN_AB listens to a point, makes a change (any list of action calls, one undo step), listens again and puts the mix back unless kept — faders, sends, FX switches, held values and plugin states, as MIX_RESTORE does, Signal's own edits included — answering both sets of figures, their difference and a spectrum-difference PNG. LISTEN_RENDER writes the audio at any point to a WAV, through `listen::render_at`, the entry point an offline Signal block's analysis renders `block:<id>:pre` through. Every listen reads `block:<id>:pre|post` inside a Signal instance (rendered on a cut copy). MIX_STATE embeds each Signal instance's signal.chain/1 (session)

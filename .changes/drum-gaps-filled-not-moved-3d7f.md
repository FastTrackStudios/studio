---
"session-engine" = "minor"
---

A drum edit never moves a hit to fill a gap any more: the gap a move opens is filled by the placement algorithm (daw.edit 0.3, rhythm's) — the slice before plays on to just before the next attack in its recording, the slice after reaches back into its own take, the two crossfade, and only what is left is stretched (the last span, never slower than 0.8×), the rest left silent. The engine sends the COMP lane's takes and renders the crossfades and partial stretches as asked; QUANTIZE, HITS_PLACE and DRUMS_APPLY all get it. DRUMS_APPLY's `moved_up` is gone; `fills` says how each gap was filled (session)

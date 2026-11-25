---
"studio-engine" = "minor"
---

LISTEN_ALIGN finds how each mic of a drum piece should be moved, turned over and phase-rotated in one band so the piece's sum is fullest — scored on the sum over the hits (its level, its low end, its attack's transients), not on how alike two mics are — and with `apply: true` sets the fixes (through Signal's `align` block where the track's instance has one, else the polarity through the track), hears the hits again, and keeps them only if the sum is better (session)

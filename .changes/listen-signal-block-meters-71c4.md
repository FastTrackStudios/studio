---
"session-engine" = "minor"
---

LISTEN's `dynamics` gives a Signal instance's `blocks`: each block's own meters over the span (a compressor's gain reduction as `gr`, a gate's opening, a dynamic EQ band's or a transient shaper's gain), read from the render copies through SIGNAL_METERS, no playback (session)

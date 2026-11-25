---
"studio-engine" = "minor"
---

Add SIGNAL_ANALYZE and SIGNAL_ANALYSIS: a Signal offline block (the vocal leveler, the aligner) analysed over the audio arriving at it, read offline with no playback (stopped or playing), its lanes written at once as the FX's own parameter automation in one undo step, with how long it took; after each edit step a Signal FX made from audio that moved marks it stale (session)

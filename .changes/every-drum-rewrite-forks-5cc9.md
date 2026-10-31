---
"session-engine" = "patch"
---

Every drum rewrite forks a checkpoint whatever its size, and the other mass drum writers (HITS_DETECT, HITS_RENEW_IDS, DRUMS_LEVEL, DRUMS_SAMPLES/_ALIGN, DRUMS_REMOVE, COMP_MAIN, COMP_SEAMS, COMP_BEST over the whole song) do too; what a key runs over a selection forks none; SESSION_CHECKPOINTS_PRUNE lets go of old automatic checkpoints, never named ones (session)

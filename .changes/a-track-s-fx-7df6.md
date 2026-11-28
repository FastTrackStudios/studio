---
"arrangement-view" = "patch"
"daw-extension" = "patch"
"daw-standalone" = "patch"
"daw-view" = "patch"
"daw-view-kit" = "patch"
"engine-core" = "patch"
"engine-mix" = "patch"
"mixer-view" = "patch"
"studio-engine" = "patch"
---

A track's FX chain is edited from the window on desktop, iPad and web: the FX button on a strip or a track row (and a rack close-up's FX chain tab) opens the chain close-up, where an FX is added (the FTS processors, Signal where the engine builds it, any CLAP bundle on a desktop), moved, bypassed and removed, each one undo step. Any plugin without a face of its own gets a generic panel of its parameters, each with its name, value and range, written as the racks write theirs. New engine actions FX_REMOVE and FX_MOVE; FX_LIST says each FX's guid, and every FX action takes one (#98) (session)

---
"session" = "minor"
"session-engine" = "patch"
---

A song's step reads only what the engine's journals say changed since the last read: the items written, made or removed and the recordings whose take markers were written, the rest kept from the model read before (`engine::Since`, `whole::changed_since`). A hit move's step on Unbreakable went from ~600 ms to 260–340 ms; a test holds that the step's model is the one a whole read gives (session)

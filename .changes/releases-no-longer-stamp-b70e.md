---
"app-shell" = "patch"
"fts-version" = "patch"
"guitar" = "patch"
"session-desktop" = "patch"
"session-engine" = "patch"
"studio-desktop" = "patch"
---

Releases no longer stamp the version into every crate: fts-version carries it to the binaries only, so a release recompiles just what changed (kache keys stay stable) (session)

---
"studio-shell" = "patch"
"xtask-workflows" = "patch"
---

Studio no longer aborts about 4 s after launch on iOS 27: a scene that connects without a link has nil URLContexts there, which objc2-ui-kit 0.3.2 (the newest) binds as never nil, so the scene delegate reads it through a nullable message. CI's iOS job is required again (in CI passed), and its room test taps iOS 27's "Open in Studio?" alert for the link through idb (studio#787).

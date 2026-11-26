---
"engine-runtime" = "patch"
"daw-extension" = "patch"
"studio-app" = "patch"
---

FTS_ENGINES_DIR moves where engines say where they are (their studio-<pid>.json and last-open files), for engine and window alike. just test-unbreakable sets it in its sandbox, so a test's engine no longer leaves files in the person's ~/.config/fts/engines, where a window could link to it.

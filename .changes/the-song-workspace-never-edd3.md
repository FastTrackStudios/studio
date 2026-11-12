---
"setlists" = "patch"
---

The song workspace never writes an old chart over a saved one: the .kf is the source of truth, the note's copy is shown read-only when the .kf cannot be read (with Read the .kf again), every save reads the .kf again first, a chart with no bars is not saved over one with bars without Save anyway, the .kf as opened is backed up once in the extension's own folder; lyrics show without a chart; the workspace follows the app's theme; a shared song says its page shows the chart as imported (studio#430) (session)

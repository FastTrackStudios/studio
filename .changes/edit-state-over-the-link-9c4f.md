---
"session-engine" = "patch"
"session-ui" = "patch"
---

The window's edit cursor, time selection and selection reach its engine (`show_edit`, the latest change only), so EDIT_QUANTIZE, EDIT_LEVEL and every action that reads the selection act on what is selected in a window in another process; where an action puts the cursor or selection comes back to the window (an `edits` stream) (session)

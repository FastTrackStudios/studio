---
"session-engine" = "patch"
"session-ui" = "patch"
---

Automation lanes commit what a gesture did as new AUTOMATION_POINTS_EDIT, AUTOMATION_ITEMS_EDIT and AUTOMATION_LANE_SET (points, items, a lane's hide, mode and height), their live edits one undo step with it; the Editor writes its take back through new NOTES_WRITE (session)

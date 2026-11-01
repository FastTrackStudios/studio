---
"dock" = "patch"
"session-desktop" = "patch"
"session-engine" = "patch"
---

The dock's own controls — screenset recall, save and save as, Arrange, panels moved, attached, opened, duplicated, closed, shown, maximised, their tabs, dividers, edge docks and floating windows — ask for a DockIntent; the Session window routes each to a named action (new SESSION_PANEL_MOVE/OPEN/CLOSE/SHOW/MAXIMIZE/TABS, SESSION_DOCK_SPLIT/EDGE/FLOAT, and SESSION_SCREENSET*, SESSION_ARRANGE), and a dock with no router applies them itself (session)

---
"app-shell" = "minor"
"library" = "patch"
"session-ui" = "patch"
"session-desktop" = "patch"
"studio-desktop" = "patch"
---

One form factor for every screen, by size alone: a desktop window sized like a phone is laid out as a phone (`app_shell::ui::form`, measured from the window's own events instead of three 300 ms polls); touch only sets larger targets. Panes that could not scroll on Blitz (`overflow:auto`) now scroll, and overlays that used `position:fixed` are mounted absolutely (session)

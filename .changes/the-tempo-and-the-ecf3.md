---
"arrangement-view" = "minor"
"daw-extension" = "minor"
"daw-standalone" = "minor"
"daw-view" = "minor"
"daw-view-kit" = "minor"
"midi-editor" = "minor"
"studio-app" = "minor"
"studio-engine" = "minor"
"studio-panels" = "minor"
---

The tempo and the meter are set from the ruler's TEMPO lane on desktop, iPad and web: a click makes a tempo change, a drag moves it or (up and down) sets its tempo, and a click on a change opens its menu (its meter, half or double time, remove). Each gesture is one undo step, and the engine's undo now takes the tempo map back with the rest. The window follows a tempo or meter change made anywhere else (an action over MCP, a member of the room, the engine's tempo mapping) without a reload (#97), and a touchscreen's slim ruler shows the lane through SESSION_OPTION's tempo_lane. SESSION_TEMPO reads the tempo map as the ruler does. In a browser the Editor opens the expression editor on the item the arrangement picks, writing each edit back through the engine in the page (NOTES_WRITE), and the page has Undo and Redo (#98)

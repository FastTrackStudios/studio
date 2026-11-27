---
"engine-collab" = "minor"
"room" = "minor"
"studio-app" = "minor"
---

A browser joins a desktop-hosted room: fasttrackstudio.app/join/<room> in the web build finds the room's file in its organization's Drive, dials the leader over iroh through its relay and follows what it loads and its transport. The leader serves the room, and the live set it edits, behind a permissions gate that admits only a call presenting the room's pass, made new each time the room is led and kept in the room's file in the organization's Rooms folder; ROOM says how many calls it refused and which load each member has opened (studio#711, studio#362).

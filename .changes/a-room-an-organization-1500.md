---
"daw-extension" = "minor"
"engine-collab" = "minor"
"engine-core" = "minor"
"room" = "minor"
"studio-engine" = "minor"
---

A room: an organization's lasting place to play together, with one link that never changes (ROOM_CREATE, ROOMS). Its leader hosts it (ROOM_LEAD) and loads a setlist or a lone session into it (ROOM_LOAD); a member joins once by the link (ROOM_JOIN) and follows the leader's setlist, song and playhead, and keeps following when the leader loads another setlist, on the same connection (ROOM says how it goes). Covered by Seam 2's base harness (just test-room): a leader and a member, each its own engine, over MCP.

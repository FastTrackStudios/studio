---
"room" = "minor"
"studio-app" = "minor"
"studio-shell" = "minor"
"xtask-workflows" = "patch"
---

Studio on iPhone and iPad joins a room from its link: fasttrackstudio://join/<room> (the URL type the iOS build declares) opens Studio and joins the room, and `Studio --join <link>` does at launch on any native build. The engine linked into the app keeps and finds rooms in the app's own Studio folder, and the room crate reads a link in the app's scheme as the same room. Covered by Seam 2 with the simulator (apps/studio/ios/sim-room-test.py, in CI's ios job): a desktop engine leads, the phone follows its setlist, song, playhead and stop, shows in its room, and moves to the next setlist without reconnecting.

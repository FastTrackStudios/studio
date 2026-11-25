---
"daw-view-kit" = "none"
"midi-editor" = "none"
"studio-panels" = "none"
"engine-host" = "none"
"studio-app" = "none"
"daw-view-tools" = "none"
---

The MIDI editor view (the expression editor and the Editor panel) moved out of daw-view-kit into a new crate, midi-editor, on top of the kit; the kit takes the editor's theme tokens through a slot and needs only the editor's painter (session)

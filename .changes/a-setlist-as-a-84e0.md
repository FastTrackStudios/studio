---
"setlists" = "minor"
"studio-island" = "minor"
---

A setlist as a streaming app's playlist in Architect's web app: the island's web bundle has a setlist view (mount("setlist")), drawn at the head of a note of type setlist (its manifest's [[embeds]] claim): each set a label, each song a row with its number, its key's tile, title over artist and length, the song playing marked and followed, a row pressed plays it; a song with no recording stays listed. Given the note and the playhead's state as props; asks only PLAYHEAD_SET, PLAYHEAD_ITEM, PLAYHEAD_PLAY and PLAYHEAD_PAUSE (architect.playhead v0.1, architect#589). setlists::row holds a row's key and colour for the share page and the view (session)

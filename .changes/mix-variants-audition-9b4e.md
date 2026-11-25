---
"studio-engine" = "minor"
---

Mix variants: the agent makes variants of one mix decision (MIX_VARIANTS_CREATE, MIX_VARIANT_ADD from the live mix or a base plus changes); you switch between them while the song plays with no undo steps (MIX_VARIANT_SWITCH, MIX_VARIANT_NEXT/PREV), loudness-matched (MIX_VARIANTS_MATCH) and blind if you like (MIX_VARIANTS_BLIND), rate or pick one (MIX_VARIANT_RATE/PICK), and the winner is kept as one undo step (MIX_VARIANT_KEEP). Mix snapshots take a scope of tracks and category filters (volume, pan, mute, solo, polarity, sends, fx) on save and recall, and record their parent (session)

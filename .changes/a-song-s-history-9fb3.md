---
"engine-core" = "patch"
---

A song's history keeps a step made on another device in a room under its own name: the local step's append stops at its own commit and reads the doc once, and the recorder counts a step before it hands it on, so an undo never undoes a phantom step.

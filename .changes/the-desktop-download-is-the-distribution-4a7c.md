---
"studio-architect" = "minor"
"xtask-workflows" = "none"
---

Changed the desktop download: the Linux AppImage and the macOS .dmg are Studio as a distribution of Architect (`studio-architect`, installed as `studio`), with its engine (the `daw` extension, `daw-extension`) shipped beside it and loaded as a built-in, in place of the earlier `studio-desktop` app. `studio --version` prints the release's version and opens nothing. Needs architect v0.15.1 (`Distribution::shipped`), which every Architect pin moves to. Not in the download yet: Ignition's room renderer, and a first-run layout (a fresh install opens on the Project Launcher) (studio)

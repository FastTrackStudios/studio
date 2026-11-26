---
"song-file" = "patch"
"studio-app" = "minor"
"xtask-workflows" = "patch"
---

Studio builds for iPhone and iPad with the engine linked into the app (driven in-process, Door::Here); the iOS simulator build and Seam 3 boot test run in CI (ios in checks.yml); its spans and crashes reach the telemetry (checked in the simulator; the app's own spans were filtered under the wrong target, `studio_app` for `Studio`, and reached no collector on any platform); the iOS build and TestFlight scripts are rewritten under apps/studio/ios. song-file puts replaced files in a folder beside the song on iOS, which has no Trash.

---
"utils" = "minor"
"keys" = "patch"
---

Rig files on the old disks outside Signal are in Drive too: the rig-models captures (`/run/media/Development/ci-assets/rig-models/{bass,guitar}`) read as `Studio/Signal/rig-library/{bass,guitar}/models`, and AudioHaven's raw `Sampled` and `Kontakt` libraries as `Studio/Libraries/{Sampled,Kontakt}` (the Libraries home, `<Studio home>/Libraries`). A file that isn't on this computer names the `drive pin` path for its place, quoted; a missing sample library says to pin only the one a song plays. `utils::home` maps them through one table (`LEGACY_ROOTS`, `place`, `Homes`). Keys' raw sample root defaults to AudioHaven's `Sampled` where this computer has it, else Drive's `Studio/Libraries/Sampled`.

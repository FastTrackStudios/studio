---
"fts-version" = "minor"
"studio-app" = "minor"
"studio-shell" = "minor"
---

Desktop Studio updates itself: it finds a newer studio-v* release (pre-releases included), verifies its SHA256SUMS against the org's minisign key and the download against those sums, and installs the .dmg's bundle (code signature and team checked) or the AppImage (its studio link and desktop entry moved); STUDIO_UPDATE checks or installs now

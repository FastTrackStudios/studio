---
"lyrics-ui" = "none"
---

Fixed the release's node, web bundle and image builds, which v26.11.5 shipped without: the island's lints (only the tag's build ran its clippy), the web bundle's check for Studio's distribution (it looked for the earlier name, Studio Architect), and the Nix build's vendored sources, which left out a crate Signal carries for its own patch (studio)

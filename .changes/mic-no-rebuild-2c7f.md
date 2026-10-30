---
"mic-plugin" = "none"
---

The mic plugin's build no longer watches a models folder that is not there: cargo counted it as changed every time and rebuilt the plugin, the built-in effects, the engine and the apps on every build (session)

---
"app-shell" = "minor"
"fts-clap-host" = "minor"
"guide-plugin" = "minor"
"engine-host" = "minor"
"studio-engine" = "minor"
"engine-runtime" = "minor"
"session-ui" = "minor"
"xtask-workflows" = "minor"
---

Restructured: apps/ holds only apps/desktop (Studio, package studio-desktop, binary studio, was studio-architect) and apps/web; Session's crates moved to features/engine, the plugins to plugins/; the old rig app (and its iOS build), Signal's apps, themer and Session's sites removed; no Docker image (studio)

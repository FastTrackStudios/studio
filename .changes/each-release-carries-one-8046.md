---
"xtask-workflows" = "none"
---

Each release carries one SHA256SUMS over its installers, as T3 Code does; the installer checks against it and no longer calls the GitHub API for asset digests (session)

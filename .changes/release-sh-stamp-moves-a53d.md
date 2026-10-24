---
"xtask-workflows" = "none"
---

release.sh stamp moves a workspace member's "name version" lockfile references too, so cargo --locked accepts a stamped lockfile when a member shares its name with a dependency (studio)

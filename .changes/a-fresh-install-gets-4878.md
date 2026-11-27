---
"studio-app" = "minor"
---

A fresh install gets from signing in to an open setlist with no setup screens (#699). With nothing naming a Drive node, desktop Studio reaches FastTrack's (wss://drive.fasttrackstudio.app/sync) and signs in through the account server in the system's browser, back on a loopback port (Drive pinned at 2d07bee for drive#206); iPhone and iPad keep to the device's copies until Drive can sign in there (drive#183). Signed in, the organizations listed are the account's as the node says, none included, never the made-up ones, and Home lists each organization with its setlists, each one tap from open; in none, it offers to create one. Over MCP: STUDIO_ACCOUNT and STUDIO_SIGN_IN (answered on the sign-in screen too), STUDIO_FIRST_RUN, and STUDIO_ROUTE now says which setlist file a page shows. Covered by a Seam 1 test on desktop (just test-first-run).

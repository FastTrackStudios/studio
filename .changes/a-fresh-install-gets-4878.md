---
"studio-app" = "minor"
---

A fresh install gets from signing in to an open setlist with no setup screens (#699). With nothing naming a Drive node, desktop and phone Studio reach FastTrack's (wss://drive.fasttrackstudio.app/sync) and sign in through the account server in the system's browser, back on a loopback port (Drive pinned at 2d07bee for drive#206). Signed in, the orgs listed are the account's as the node says, none included, never the made-up ones, and Home lists each org with its setlists, each one tap from open; in no org, it offers to create one. Over MCP: STUDIO_ACCOUNT and STUDIO_SIGN_IN (answered on the sign-in screen too), STUDIO_FIRST_RUN, and STUDIO_ROUTE now says which setlist file a page shows. Covered by a Seam 1 test on desktop (just test-first-run).

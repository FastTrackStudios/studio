---
"studio-app" = "patch"
---

Creating an organization sends the app's own sign-in (Drive's first-party access token) to the account server, which takes it for creating since FastTrackStudios/auth#4: no more trading it for a session at `/oauth2/token`, which the `drive` client is not allowed (#697). A 403 still says the account server does not let this app create organizations; a refused sign-in refreshes once and tries again.

---
"studio-app" = "minor"
---

An organization's owner or admin invites people from Studio, by email or by a link (Organization settings, `STUDIO_ORG_INVITE`). The invited person joins with any account, from the auth server's page or by pasting the link into Studio (All orgs and the org switcher, `STUDIO_ORG_JOIN`). The owner removes a member, whose access then ends (`STUDIO_ORG_REMOVE`). The member list comes from the auth server, by account and never matched by email (`STUDIO_ORG_MEMBERS`). All four calls use the app's own sign-in, Drive's first-party access token, which the auth server accepts for exactly these calls since FastTrackStudios/auth#5. The `auth*` crates are pinned at `39f4bf2` (#698).

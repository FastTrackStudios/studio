---
"patchbay" = "patch"
---

Since #477 an engine bound to a hostname reports and advertises the address it resolved to: localhost:4046 shows as 127.0.0.1:4046, and a LAN hostname bind is now advertised on mDNS, which it was not before. On a phone the engine no longer warns that the family views it holds have no family, and a configured device says it connects on the machine that runs it (studio)

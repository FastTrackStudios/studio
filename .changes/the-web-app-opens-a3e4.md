---
"studio-app" = "patch"
---

The web app opens offline again at any of its paths: the service worker keeps the page under its scope whichever route it was opened at, so a reload of /?session=… with no network finds it after / became /home.

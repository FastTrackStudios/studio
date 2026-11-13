---
"patchbay-antelope" = "minor"
---

The Antelope family hears announces through an AnnounceSource (the Manager Server multicast by default; a test hands it a fake), and Galaxy32Adapter::connect_from_announces; no test reaches the announce listener, and under PATCHBAY_LAN=off it refuses to bind (studio)

---
"guitar" = "patch"
---

The guitar rig's embedded captures, frozen Cores and profiles are copied into OUT_DIR and included by a path inside it, so its generated table is the same in every checkout and right wherever the target dir is. A path from OUT_DIR out to the checkout broke when kache replayed a build-script run recorded with the target dir outside the checkout.

---
"session-ui" = "patch"
"session-desktop" = "patch"
"studio-desktop" = "patch"
---

The window no longer crashes on a long song: its GPU device asks for buffers as large as the adapter has, where wgpu's default 128 MiB was too small for Unbreakable's arrangement in one frame (`create_bind_group`: "range 268435456 exceeds … 134217728") (session)

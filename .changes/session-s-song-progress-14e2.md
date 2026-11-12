---
"session" = "minor"
"transport-ui" = "minor"
---

Session’s song progress bar and transport controls are their own crate, transport-ui (Dioxus only), so a share page’s island draws the same view; the transport can leave out Loop (show_loop) where a surface cannot loop (studio#445) (session)

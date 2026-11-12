---
"studio-island" = "none"
---

The island's web bundle ships only the modules its glue reaches (no Dioxus devtools console forwarder) and is refused at build if it names WebRTC, a WebSocket, storage or cookies; just island-web takes cargo, wasm-bindgen and wasm-opt from the dev shell (session)

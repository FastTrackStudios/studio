---
"patchbay" = "minor"
---

ui::PatchbayHandle and ui::EngineLink keep their clients crate-private (PatchbayHandle::new, EngineLink::new and same_connection for shells), so a family view reaches the engine through ui::kit only; a ParamWriter writes to the device it was drawn for and drops a deferred write after the page moved on (studio)

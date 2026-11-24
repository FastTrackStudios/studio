---
"mixer-view" = "minor"
"studio-view-link" = "minor"
"studio-mixer" = "patch"
---

The mixer's Signal strips read the engine: `signal::engine::EngineSource` answers the mixer's SignalSource over SIGNAL_CHAIN, SIGNAL_RUN, SIGNAL_SESSION, Signal's SIGNAL_ALGORITHMS and FX_SET on the linked engine (or this process's), its live feed polled from SIGNAL_FEED every 100 ms and kept; installed by `mixer_host::install`, and SIGNAL_INSERTS registered and claimed with the mixer's window actions (signal)

# rig-library

What the guitar and bass rigs ship with that cannot be public: the captures
and cab IRs (`<rig>/models/`, linked in from the private rig-models repo by
`just assets`) and the guitar's frozen Cores (`guitar/frozen/`).

The `guitar` crate (FastTrackStudios/signal) embeds them at build time. Its
build script reads them from `$SIGNAL_RIG_LIBRARY/<rig>/{models,frozen}`,
which `.cargo/config.toml` points here. Without `just assets` a debug build
has a rig with no captures, and a release build refuses to start.

```
rig-library/
  guitar/models   -> .assets/rig-models/guitar   (just assets)
  guitar/frozen/     the frozen Cores the presets play
  bass/models     -> .assets/rig-models/bass     (just assets)
```

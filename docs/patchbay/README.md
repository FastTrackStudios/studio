# Patchbay moved to its own repository

Patchbay's app, CLI, engine, hardware adapters, virtual-device driver,
platform packaging and documentation are maintained at
[FastTrackStudios/patchbay](https://github.com/FastTrackStudios/patchbay).
Its standalone release is independent of Studio. Studio no longer builds
or installs Patchbay; existing installations and settings are unaffected.

```sh
git clone https://github.com/FastTrackStudios/patchbay.git
cd patchbay
nix develop
just check
just test
just run
# When ready to replace and restart an installed app:
just install
```

Register that checkout as its own T3 Code project. Use `nix develop
--command just check`, `nix develop --command just test` and `nix develop
--command just run` as project actions. The host-wide Cargo rustc wrapper
uses kache automatically; do not share a mutable target directory between
repos. Toolchain, target, profile and compiler flags affect cache reuse.

The standalone repo retains the existing PipeWire/Core Audio, Dante,
Yamaha TF and Antelope Galaxy support, desktop and browser interfaces,
CLI, packaging and tests. Future Studio integration should consume its
service contract rather than duplicate the implementation or open a second
hardware connection. SwiftMix's hardware session belongs with Patchbay;
DAW zones, track mapping and automation remain in Studio.

Tracked by [Studio #363](https://github.com/FastTrackStudios/studio/issues/363).

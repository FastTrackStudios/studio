# studio-mobile

Studio on an iPhone or an iPad: Architect's iOS app (`architect_mobile::run`,
Blitz on UIKit) running Studio's distribution
(`studio_extensions::distribution`, the same one the desktop app and
studio-web run: Studio's extensions, the window's views, the launcher as
the home). Its bundle id is `app.fasttrackstudio.studio`, the TestFlight
record the old rig app (`apps/studio/desktop`, removed in `904a04c54`)
shipped to; its icon is that app's.

On any other target the crate builds to a `main` that does nothing, so a
workspace-wide `cargo check` does not need the iOS SDK.

Build it on a Mac (airlock) in the dev shell:

```sh
cargo build -p studio-mobile --target aarch64-apple-ios-sim   # the Simulator
cargo build -p studio-mobile --target aarch64-apple-ios       # a phone
dx serve --ios                                               # from this folder, the Simulator with hot reload
```

Not yet: a TestFlight release. Architect's `apps/mobile/ios/build-app.sh`
and `deploy-testflight.sh` build a distribution crate
(`--manifest-path apps/mobile/Cargo.toml --bin studio-mobile`, `INFO_PLIST=
apps/mobile/ios/Info.plist`) but still copy the binary in as
`architect-mobile` and compile Architect's asset catalog; they need a knob
for the executable's name and the catalog (this folder's `Assets.xcassets`)
before Studio ships from them.

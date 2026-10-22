# scripts/apple — Apple signing and deploy scripts shared by the apps

Session and Signal used to carry their own copies of these under
`apps/{session,signal}/desktop/ios/`: ten byte-identical files (1,138 lines)
and three that differed only in a few app values (STA-393). They now live
here once.

## Used as-is by every app

| Script | What it does |
| --- | --- |
| `setup-keychain.sh` | One-time: builds the dedicated `fts-build.keychain` on a headless box |
| `mint-dist-cert.rb` | Mints the Apple Distribution cert (App Store / TestFlight) |
| `mint-developer-id.rb` | Mints the Developer ID Application + Installer certs (macOS outside the store) |
| `mint-dev-profile.rb` | Mints a provisioning profile for a device and/or bundle id |
| `deploy-macos-plugins.sh` | Builds, signs and notarizes the plugin suite (.zip) |
| `verify-macos-universal.sh` | Checks a .dmg/.zip is universal and correctly signed |
| `invite-internal.rb`, `invite-tester.rb` | TestFlight tester invites |
| `submit-beta-review.rb`, `testflight-status.rb` | TestFlight beta review / build status |

Each app's `deploy-testflight.sh` calls the `mint-*.rb` helpers from here
(`APPLE_DIR="$ROOT/scripts/apple"`). The `.rb` helpers take the bundle id as
an argument; run them directly, e.g.
`ruby scripts/apple/testflight-status.rb app.fasttrackstudio.signal`.

## Shared bodies, run through a per-app wrapper

`deploy-iphone.sh`, `deploy-macos.sh` and `deploy-macos-pkg.sh` hold the
logic. `apps/<app>/desktop/ios/<same name>` is a short wrapper that sets
that app's parameters and `exec`s the shared script, so the old entry paths
still work. Each wrapper value is a default; the environment overrides it.
The shared scripts refuse to run (`${VAR:?}`) without their required
parameters.

| Parameter | Script | Session | Signal |
| --- | --- | --- | --- |
| `DX_APP_DIR` | iphone, macos | `apps/session/desktop` | `apps/signal/desktop` |
| `DX_PACKAGE` | iphone, macos | `session-desktop` | `signal-desktop` |
| `DX_FEATURES` | iphone | `session-domain,charts` | `signal-guitar,signal-keys-rig,signal-drums-rig,signal-ekit-rig` |
| `IOS_APP_NAME` | iphone | `Session-desktop` | `Signal-desktop` |
| `MIC_USAGE` | iphone | setlist-playback wording | guitar-input wording |
| `ICONS_DIR` | iphone, macos | `$DX_APP_DIR/ios/Assets.xcassets` | same |
| `DMG_FALLBACK_VER` | macos | `0.0.4` | `0.0.1` |
| `APP_IOS_DIR` | pkg | the wrapper's own dir (its `deploy-macos.sh` builds the app) | same |
| `VERSION_PACKAGE` | pkg | `session-desktop` | `signal-desktop` |

All the other knobs (`KEYCHAIN`, `KEYCHAIN_PW`, `MARKETING_VER`, `SKIP_BUILD`,
`BUILD_ONLY`, `ADHOC_SIGN`, `PKG_PRODUCT`, `PKG_UNSIGNED`, …) are unchanged
and documented at the top of each script.

## Behaviour changes in the move

- `ROOT` is now `git rev-parse --show-toplevel`. The macOS scripts computed
  it as `$SCRIPT_DIR/../../..`, which pointed at `apps/` once the apps moved
  to `apps/<app>/desktop/ios/`.
- The old defaults named paths that no longer exist (`apps/fasttrackstudio`,
  `apps/desktop`, crate `fasttrackstudio`). The wrappers pass the real crate
  and directory. Moving these scripts into architect is a separate decision.

# Studio on iPhone and iPad: the distribution's iOS app

Studio is a distribution of Architect (`apps/architect`, crate
`studio-architect`, `docs/architect-distribution.md`). This is that
distribution on iOS: the same crate, the same `Distribution`, run by
`architect_mobile::run` instead of `architect_desktop::run`. It is not the
shipping iOS app (`apps/studio/desktop`, bundle
`app.fasttrackstudio.studio`), which it leaves alone: no shared
identifier, signing, CI workflow or TestFlight lane.

| | this app | the shipping app |
| --- | --- | --- |
| crate | `studio-architect` | `studio-desktop` |
| bundle id | `app.fasttrackstudio.studio-architect` | `app.fasttrackstudio.studio` |
| display name | Studio | Studio |
| Info.plist | `apps/architect/ios/Info.plist` | `apps/studio/desktop/ios/app-plist.sh` |
| built by | `apps/architect/ios/build-app.sh` | `ios/build-ios.sh`, `deploy-testflight.sh` |

## The shape

Architect's mobile app (`architect:apps/mobile`) is a library:
`architect_mobile::run(Distribution)` is the whole `main` of any
distribution's iOS app, the counterpart of `architect_desktop::run`
(Blitz in winit's UIKit window, the UIScene delegate, the phone shell).
iOS allows no JIT and no downloaded native code, so the app runs what it
links: every extension in the distribution's bundles. A distribution
crate therefore needs three things, all here:

1. **Its Architect dependency by target** (`apps/architect/Cargo.toml`):
   `architect-desktop` everywhere but iOS, `architect-mobile` on iOS,
   `architect-distribution` (the `Distribution` both are handed) for
   everyone. `main` calls one `run` or the other.
2. **An Info.plist of its own** (an iOS binary cannot be asked who it is
   on the Mac that builds it): `apps/architect/ios/Info.plist`. The
   executable is `architect-mobile` and the scene delegate
   `ArchitectSceneDelegate`, as Architect's bundler and `run` name them.
3. **Every linked crate building for `aarch64-apple-ios` and `-sim`.**

`apps/architect/ios/build-app.sh` runs Architect's own bundler
(`apps/mobile/ios/build-app.sh` of the Architect checkout cargo resolves)
on this crate with that plist and Studio's icon:

```sh
apps/architect/ios/build-app.sh                      # the Simulator, signed ad hoc
apps/architect/ios/build-app.sh --device --release   # a device, unsigned
# on the side-by-side stack (tools/stack/gen-overrides.py):
apps/architect/ios/build-app.sh --config ../stack/overrides.toml
```

## What of the bundle runs on a phone

Checked on the stack of 2026-11-18 (`stack/studio-distribution` in
architect, keyflow, Ignition, session). Everything compiles for both iOS
targets except one crate; what does not *run* is what needs a second
process.

| extension | on iOS | why |
| --- | --- | --- |
| Setlists, Productions, Studio views (`studio()`) | runs | notes and share links through the app's actions |
| Project Launcher | runs: lists the vault's setlists and songs | opening one "in Studio's engine" has no engine to open it in |
| Keyflow (the chart) | runs | linked, drawn in the app |
| Arrangement, Mixer, Lyrics, Navigator (`stage()`) | **stubbed**: the pane says "Not on iPhone and iPad yet…" (`studio_view_link::NO_ENGINE_HERE`) | they are windows on Studio's engine, a process of its own on a desk (`studio engine`); an iOS app is one process and the engine is not in it yet |
| Lighting (Ignition) | **left out** of the iOS bundle | its room is drawn by a program of its own in the pane (a process view, architect#838): `architect_process_view::host` exists on Linux and macOS only, and the crate does not compile for iOS with `pane` |
| Productions' "open in its DAW" | compiles, does nothing useful | it starts another program |
| Studio's island (the share page's bar) | not looked for | it ships beside a desktop binary |

The engine on iOS is the next step for those four panes: in-process (the
shipping app already runs the daw engine in its process for the rigs), not
a second process. Until then the first cut is the listening and managing
app: sign in, the vault's setlists and libraries, a song's chart, the
recordings, share links.

## Audio

Architect's native apps ran the playhead on a **silent clock**
(`architect_playhead::Silent`): the Now Playing bar moved and nothing was
heard; only the web app had a player (its `<audio>` element). The
recordings are Ogg Vorbis, which iOS does not decode, so the app decodes
them itself. `architect_playhead::native` (feature `native-audio`, on
architect's `feat/studio-ios-distribution`) is a `Clock` for native apps:

- Symphonia decodes (Ogg Vorbis, MP3, AAC in MP4, FLAC, WAV) on a thread
  into a ring; cpal (CoreAudio's RemoteIO on iOS) plays it; rates that
  differ are resampled linearly.
- The device is opened at the first play, not at a load.
- On iOS: the `playback` audio session, `UIBackgroundModes` `audio` in
  the Info.plist, the lock screen's card (`MPNowPlayingInfoCenter`:
  title, artist, the source's name, art, length, position) and its
  buttons and scrubber (`MPRemoteCommandCenter`) as the app's own presses.
- An item's media is a file of the app's open vault
  (`architect_app::vault_file`); with no such file the silent clock goes on.

What is not done: a recording that is only on the node (not synced to
the device) is not streamed; interruptions (a call) pause through cpal
but the deck is not told; the song view's own clock
(`setlists::workspace::Clock`) is still its own and silent, so a song's
chart follows the playhead only where the view reads `architect.playhead`.

## Checking it

`features/setlists/examples/demo_vault.rs` writes a small vault (two
songs with a chart and a recording each, one setlist). In the Simulator,
`SIMCTL_CHILD_ARCHITECT_VAULT=<that folder>` opens it, and
`SIMCTL_CHILD_FTS_CONTROL=1 SIMCTL_CHILD_FTS_CONTROL_DIR=<a folder on the
Mac>` serves the app's Action List on a socket there
(`STUDIO_SETLISTS_LIST`, `PLAYHEAD_SET`, `PLAYHEAD_STATE`, …).
`ARCHITECT_AUTH_ISSUER` names another issuer than
`auth.fasttrackstudio.app` (the local stack's, `docs/local-stack.md`).

## Between this and TestFlight

- **An App Store Connect record and a bundle id** for
  `app.fasttrackstudio.studio-architect` (or the decision that this app
  takes over `app.fasttrackstudio.studio` when `studio-desktop` retires),
  a provisioning profile, and a signing lane: Architect's
  `apps/mobile/ios/deploy-testflight.sh` takes `DIST_MANIFEST` and signs
  with `sign-entitlements.py`; it has not been run for this app.
- **Entitlements**: none are needed for background audio (it is the
  Info.plist's `UIBackgroundModes`). Associated Domains
  (`applinks:<the node's host>`) for share links that open in the app, and
  a URL scheme of Studio's own (the plist declares none: `architect://`
  is the Architect app's). Architect's HealthKit entitlement must not be
  copied: the plist still carries Architect's Health, camera and photo
  usage strings because the Architect base distribution links those
  extensions; either drop the extensions from Studio's distribution or
  keep the strings.
- **Review**: sign-in is required to see anything of an account, so a
  demo account for App Review (and account deletion reachable in the
  app, guideline 5.1.1(v)); background audio must be audible content the
  person started (it is); `ITSAppUsesNonExemptEncryption` is declared
  false while the app links TLS and iroh, which wants a second look;
  GPL-3.0 code on the App Store is the owner's own to license.
- **Version and build number**: the plist carries `0.1.0` / `1`; the
  release has to stamp them.
- **The app's first screens are Architect's** (Inbox, the Architect tour,
  the vault navigator). The phone layout with bottom tabs and the
  signed-in dashboard of sets and libraries are the web thread's
  (`feat/share-link-setlist-web`); they are not merged here.

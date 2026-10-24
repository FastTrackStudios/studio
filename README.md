# studio

FastTrackStudio's audio and music software, in one repository: a DAW and
the apps built on it.

- **Session** — the full DAW: tracking, editing and mixing, setlists and
  songs, charts in the keyflow language, and the guide (click, count-in,
  spoken section cues). Desktop, iPhone, and a browser version.
- **Signal** — a live instrument rig: guitar, bass, keys, drums and
  sampled instruments, with tones and patches switched during a set. The
  rig runs headless and every screen — laptop, phone, tablet, browser — is
  a remote attached to it.
- **Patchbay** — studio routing: PipeWire, Core Audio, Dante and digital
  desks, wired and recalled from one window.
- **Plugins** — the FTS effects (EQ, compressor, saturation, delay,
  reverb, modulation, gate, limiter, pitch, NAM amps and more) as CLAP and
  VST3.
- **frame** — the design and control-surface engine the plugin faces and
  control surfaces are drawn with.

Session and Signal run the same engine: one DAW engine per process, with
a rig being a folder of tracks in it
([docs/design/one-engine.md](docs/design/one-engine.md)).

## Install

**Studio** is the app: Signal and Session behind one front screen.

### macOS and Linux

```sh
curl -fsSL https://raw.githubusercontent.com/FastTrackStudios/studio/main/scripts/install.sh | sh
```

The command installs the newest release. On macOS (Apple silicon), the
signed, notarized Studio.app goes into `/Applications`. On Linux (x86_64),
the AppImage goes into `~/.local/share/studio`, with `studio` on your PATH
and an entry in your app menu. For now the Linux build needs glibc 2.42 or
newer (Arch, Fedora 43, Ubuntu 25.10 and later); a build for the LTS
distros is next. Every download is checked against its
published sha256 before anything is installed.

Running the command again updates Studio in place. To install a particular
version, set `STUDIO_VERSION=26.10.1`. You can also download the
installers yourself from
[Releases](https://github.com/FastTrackStudios/studio/releases/latest).

### iPhone and iPad (TestFlight beta)

Studio for iPhone and iPad is in beta on TestFlight, and every release
ships there too. Install [TestFlight](https://apps.apple.com/app/testflight/id899247664)
from the App Store, then join the beta:

**[Join the Studio beta on TestFlight](https://testflight.apple.com/join/TESTFLIGHT_CODE)** <!-- TODO: the public link; `ruby apps/studio/desktop/ios/testflight-status.rb app.fasttrackstudio.studio` prints it -->

## Layout

```
apps/      the applications — session/, signal/, patchbay/, plugins/, themer/
crates/    daw (the DAW core and its standalone engine), studio (the
           umbrella: REAPER adapter, project files, UI, CLI), frame
features/  one crate per feature — the effects, the rigs, audio, MIDI,
           formats, editing, control surfaces, …
vendor/    patched third-party crates
xtask/     build tooling
docs/      design notes and guides
```

## Building

Everything builds from the repository root, in the Nix dev shell:

```bash
nix develop                 # or: direnv allow
just check                  # cargo check --workspace
just test                   # the test suite (cargo nextest)
just app                    # run Session
just guitar                 # run Signal's guitar rig
just plugins-bundle         # build the CLAP/VST3 plugins into target/bundled
```

`just --list` shows every recipe.

## Assets

Large binaries — test fixtures, research captures, the amp captures and
impulse responses the rigs ship with — live in separate private
repositories. `just assets` fetches them and links them into place
(the list is in `assets.txt`). The tests that read those fixtures, and
the rigs' bundled models, need them.

## Licence

GPL-3.0-or-later. Vendored third-party code under `vendor/` keeps its
own licence.

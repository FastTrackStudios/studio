# studio

FastTrackStudio's audio and music software, in one repository: a DAW and
the apps built on it.

- **Studio** — the app: Studio's distribution of Architect, with Session
  (the full DAW: tracking, editing and mixing, setlists and songs, charts
  in the keyflow language, the guide) and Signal's live rigs compiled in
  as extensions; and its web client.
- **Signal** — the live instrument rig's library (guitar, bass, keys,
  drums, sampled instruments) lives in
  [FastTrackStudios/signal](https://github.com/FastTrackStudios/signal);
  here are its extensions.
- **Patchbay** — studio routing, in its own repo:
  [FastTrackStudios/patchbay](https://github.com/FastTrackStudios/patchbay).
- **Plugins** — the FTS effects (EQ, compressor, saturation, delay,
  reverb, modulation, gate, limiter, pitch, NAM amps and more) as CLAP and
  VST3.
- **frame** — the design and control-surface engine the plugin faces and
  control surfaces are drawn with.

Session and Signal run the same engine: one DAW engine per process, with
a rig being a folder of tracks in it
([docs/design/one-engine.md](docs/design/one-engine.md)).

## Install

**Studio** is the app (apps/desktop).

### macOS and Linux

```sh
curl -fsSL https://raw.githubusercontent.com/FastTrackStudios/studio/main/scripts/install.sh | sh
```

The command installs the newest release. On macOS (Apple silicon), the
signed, notarized Studio.app goes into `/Applications`. On Linux (x86_64),
the AppImage goes into `~/.local/share/studio`, with `studio` on your PATH
and an entry in your app menu. For now the Linux build needs glibc 2.42 or
newer (Arch, Fedora 43, Ubuntu 25.10 and later); a build for the LTS
distros is next. Every download is checked against the
release's `SHA256SUMS` before anything is installed.

Running the command again updates Studio in place. To install a particular
version, set `STUDIO_VERSION=26.10.1`. You can also download the
installers yourself from
[Releases](https://github.com/FastTrackStudios/studio/releases/latest).

### iPhone and iPad

Not for now: the iOS build went with the old rig app, and Studio's iOS
target is being built.

## Layout

```
apps/        desktop/ (Studio, the `studio` binary) and web/ (studio-web)
extensions/  Studio's views and engine as Architect extensions
features/    one crate per feature — engine/ (Session: engine, runtime,
             ui, host), editing, timeline, formats, control surfaces, …
plugins/     the plugins that need Studio (guide, host) and verify/
xtask/       build tooling
docs/      design notes and guides
```

## Building

Everything builds from the repository root, in the Nix dev shell:

```bash
nix develop                 # or: direnv allow
just check                  # cargo check --workspace
just test                   # the test suite (cargo nextest)
cargo run -p studio-desktop --bin studio   # run Studio
just app                    # run Session's window
just plugins-bundle         # build the CLAP/VST3 plugins into target/bundled
```

`just --list` shows every recipe.

## Run it locally

```bash
just dev       # everything, rebuilt on save: the issuer, Studio's node on a copy of the
               # Merry & Bright setlist and its share link, the web app, the site
just dev architect=local keyflow=local   # with your ../architect and ../keyflow under it
just demo      # the quickest look: issuer + node + share link, nothing watched
just website   # the public site (Architect's apps/site), hot reload
just phone     # the running node on your tailnet over HTTPS, for a phone
```

The same recipes mean the same in Architect and Keyflow. What each starts,
the URLs, what a save costs: [docs/local-stack.md](docs/local-stack.md).

## Assets

Large binaries — test fixtures, research captures, the amp captures and
impulse responses the rigs ship with — live in separate private
repositories. `just assets` fetches them and links them into place
(the list is in `assets.txt`). The tests that read those fixtures, and
the rigs' bundled models, need them.

## Licence

GPL-3.0-or-later. Patched third-party crates live in
[FastTrackStudios/vendor](https://github.com/FastTrackStudios/vendor),
under their own licences.

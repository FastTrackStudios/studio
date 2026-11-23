# Where Studio keeps things

Studio keeps everything as plain files in one folder, its **home**. Drive
is optional. With Drive installed, the home is a folder in the Drive
mount. Managing projects then means editing files there, and the Drive app
indexes, tags and shares them.

Drive owns space on each disk. Its files can sit on the same NVMe as
before, but every access goes through Drive. Studio uses the main mount
(`~/Drive/Studio/...`), which is the logical tree: the same file can
appear in more than one place in it. `~/Drive/Devices/<device>/<disk>/...`
is a pass-through that shows where the bytes physically are. Studio never
writes a `Devices/` path or a raw disk path into a setlist or a config,
because those paths break when Drive moves data between disks.

Without Drive, the home is an ordinary folder in the config directory, and Studio opens, edits and saves the
same files the same way.

The home is found in this order:

1. `STUDIO_HOME`
2. `home = "…"` in `<config dir>/fasttrackstudio/studio.toml`
3. `~/Drive/Studio` when the Drive mount is there
4. `<config dir>/fasttrackstudio/Studio`

```
Studio/
  Songs/<Title - Artist>/   every song once, generic
  Setlists/                 sets of songs, plus what is specific to where they are played
  Signal/                   the guitar rig: NAM captures, IRs, presets
    Libraries/              libraries in Signal's own format (.signalpack), playable by Signal
      Proxy/                Ogg proxies of the packs, mirroring Libraries/ (not Raw/)
      Raw/                  the raw samples they are built from (one folder per library)
  Sample Libraries/         third-party installs: Kontakt, Modern and Massive, …
  Samples/                  loose samples
```

## Proxies

Proxies follow the same rule everywhere: a `Proxy/` folder mirrors the
folder it sits in, so `X/Proxy/<rel>` is the proxy of `X/<rel>`.

- A song's media: `Media/Proxy/Reference/x.ogg` is the proxy of
  `Media/Reference/x.wav`.
- Signal's libraries: `Signal/Libraries/Proxy/Keys/Grand.signalpack` is the
  proxy of `Signal/Libraries/Keys/Grand.signalpack`. It has the same zones
  and mapping, with its samples transcoded to Ogg
  (`fts signal pack transcode … --codec ogg`). `Raw/` gets no proxies.

A multi-mic library is one pack per mic, as Signal's pack builder already
writes them (`<Section>/<Group>/<Section> - <Group> - <Mic>.signalpack`, with
the FLAC root and the Ogg proxy root mirrored). Hot and cold data then
follow the mics. The mics a rig loads are pinned hot, and the others, such
as the room mics, stay cold until something loads them.

A proxy has the same content as its original and is only smaller. It plays
when the original is not on this machine, and the player swaps to the
original once it arrives: the engine already does this for songs
(`stream_quality`). A "lite" library, with fewer mics or round robins, is
a different library, not a proxy. Proxies are derived data: any capable
machine can regenerate them, and Drive can keep them on laptops and the
iPad while the originals stay on the storage server.

## The rule

**Generic in `Songs/` and `Signal/`, specific in `Setlists/`.**

- A setlist never copies a song. It names the song's folder and adds
  overrides only where that venue or that set needs them.
- A song never knows which sets play it.

## Songs/

There is one folder per song, `Songs/<Title - Artist>/`, holding exactly
one `<Title>.session` and its media:

```
Songs/Drive To LA - JT/
  Drive To LA.session     the song: id, engine project, timing, charts,
                          Ignition cues, Signal patch markers, arrangements
                          as branches
  Media/                  the originals (audio and video), e.g. the
                          session's recordings
    Reference/            the reference recordings, raw WAV
    Proxy/                generated proxies (Ogg Vorbis), mirroring Media/:
      Reference/          Media/Proxy/Reference/x.ogg is the proxy of
                          Media/Reference/x.wav
```

`<Title>.session` is the whole song, one file. It is a SQLite (Turso)
store, and nothing sits beside it: no `.fts-session`, no `.kf`, and no
`-wal` once the song is closed. Closing a song folds its write-ahead log
into the file, so the file alone can be copied or opened from an external
drive. Inside it:

- `meta.id` is the song's id, the UUID that setlists name it by. A
  converted song keeps the id that its old `.fts-session` had.
- The engine's project: tracks, items, FX state as blobs, and undo history.
- The session: the tempo map and meters, markers, sections, `bar_one`,
  Keyflow's chart (typed data, not text: there is no `.kf` export),
  Ignition's generic cues, and Signal's generic patch changes.
- Arrangements and variations, as branches. Each branch has its own
  project and its own session. A set will name one as `<id>@<branch>`.

The design is in the session repo, `docs/design/one-file.md`.

**Until a folder is converted**, it still holds the old pair:
`<Title>.session` (the engine's store) beside `<Title>.fts-session` (the
session's own document). Readers decide which they have by each file's
first bytes: `SQLite format 3` is a store, `ARCHDOC` is the old session
document. They never decide by the extension. A folder with an `ARCHDOC`
file is an old pair. `song-file convert <folder>` folds the pair into one
file. It verifies the new file before it moves the old pair to the Trash,
and it never deletes or overwrites anything. It also copies the old
proxies (`Media/Proxies/…`, `Media/Proxies/Reference.ogg`) to their places
under `Media/Proxy/`. It copies them and never moves them.

The folder is `Media/`, not `Audio/`, because a song can include video.
Everything under `Media/Proxy/` is derived: any capable machine can
regenerate it from the original. A workstation or an iPad can play from
proxies alone while the originals stay on the storage server.

## Setlists/

A set is `Setlists/<Set>.setlist`. It names songs by identity, not by
path, so the same file works on every device and from an external drive.
Each line is a song's id (`meta.id` in its `.session`; in a folder not
converted yet, the UUID in its `.fts-session` header) and a readable title. The file carries its own id:

```text
# id: 9b1c…
# Act I: The Originals
45b794d1-d6ba-4d66-ab2d-6be35df2f6bc  Drive To LA
= Amazing Grace
```

`= Title` is a song with no session yet. A comment directly above a song
names the act it opens. A commented-out song is kept but not played. Old
lines that hold a path are still read.

Studio finds a song from its id by looking in these places, in order:

1. The setlist's own folder tree and the `Songs/` beside it, so a set
   brought in on an external drive opens with its songs.
2. The home's `Songs/`.
3. Drive's index, when Drive is installed.
4. The path in an old line.

The truth is in the files. Any id-to-path map (in memory, a cache file, or
Drive's index) can be rebuilt from them.

What a set adds for where it is played sits beside the file, in a folder
of the same name:

```
Setlists/
  JT's Senior Recital.setlist
  JT's Senior Recital/
    ignition/          venue-specific lighting: the room's fixtures and cue overrides
    signal/<song>.…    this set's guitar version of a song's generic patch changes,
                       only for the songs that need one
```

## Signal/

The generic guitar rig and everything it plays with: NAM captures and
IRs, block presets, module presets, rig presets and profiles. Today these
live in signal's `default-config` (styx), linked from
`~/.config/signal/rig`, and the NAM files under `~/.config/signal/nam`.
Their home is `Studio/Signal/`, and signal reads them through
`SIGNAL_RIG_DIR` or that link.

## Moving in

Existing material is **copied** in, verified and then used; nothing is
moved or deleted. On THEBATTLESHIP:

- `Setlists/` already holds JT's Senior Recital and Merry and Bright 2026.
  For now their lines are absolute raw-disk paths to the song folders.
  That is a stopgap until those songs are in Drive.
- The songs then come into Drive's own space on the same NVMe, under
  `Studio/Songs/`, and each set's lines become song ids. This is
  done with the owner and is copy-only: copy, verify, and only then Trash
  the originals.
- `Signal/` is filled from signal's `default-config` and
  `~/.config/signal/nam`, and the link is repointed afterwards.

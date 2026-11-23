# Studio (apps/studio)

**Developing it locally** (building, running, the engine, driving it through its actions, overriding an extension while it runs, co-developing with Architect, Keyflow or Ignition): [docs/development.md](docs/development.md).

Studio, remade from the launcher prototype as a plain Dioxus 0.8 app: phone first, then desktop and web. It was its own repository, studio-proto, until 2026-11-22; its history is kept here. On the web it runs on [Drive](https://github.com/FastTrackStudios/drive), served by a Drive node at `/studio`. The way to it replacing the rest of this repo's apps: [docs/ROADMAP.md](docs/ROADMAP.md).

It is a member of the studio workspace (package `studio-app`). Drive's crates
come from FastTrackStudios/drive by rev and app-shell by tag (the root
`Cargo.toml`); both are on crates.io's dioxus 0.8.0-alpha.3, which the root
`[patch.crates-io]` hands the workspace's dioxus revision, so one dioxus is
built. Build it with dx 0.8.0-alpha.3 from this folder:

```bash
dx build --platform web --release --base-path studio   # the web app, for a Drive node's /studio
cargo check -p studio-app --features desktop           # the desktop build (Drive's agent through drive-local)
cargo test -p studio-app -p session-sync
```

Never `-p app-shell`: features/app/shell (the older apps' shell) has the
same package name.

## The window (2026-11-22)

The window is [app-shell](https://github.com/FastTrackStudios/app-shell)
(`v0.3.1`, `src/shell.rs`). Tabs stay off.

- **Desktop:** the sidebar holds the org switcher, Studio's places, the
  org's people and settings, the weekly events, the shows and the loaded
  rig. The top bar has back and forward, the page's title, Open anything
  and who is here. Now Playing is docked under the content.
- **Phone (under 600 px):** each page keeps its own bar, with a menu
  button for the drawer on the root pages. The foot has the tab pill (a
  set's own tabs inside a set), the round button (Open anything, Play the
  set, or Open in your rig) and the accessory: Now Playing, or the
  transport in Chart and Lyrics.

The prototype's sheets open as app-shell's sheets: a bottom sheet on a
phone, a centred panel on a desktop. Its toasts are app-shell's toasts.

## A song's files (2026-11-22)

Each song's files are a Drive project in the song's org, and its parts
point at files and folders **where they already are**, on any computer.
Linking writes Drive's registry and one Studio document. Nothing is copied,
moved or deleted, and unlinking leaves the file alone. `src/files.rs` is the
model and its tests. `src/files_live.rs` reads Drive's node, and
`src/pages/files.rs` is the UI.

- `<org>/drive:space`, project `studio-song-<id>` (`MusicRecording`): one
  part per linked file or folder (a location and a path, or the Drive tree
  for uploads). The part's label is its kind's name, so Drive's own project
  page and the orgs it is shared with group it the same way. Its documents
  are the song's session and Studio's notes, and its `shared_with` follows
  the song's: **sharing a song shares its files**. The share mirror keeps
  the name and sharing current.
- `<org>/studio:files:<id>`: Studio's notes. For each part, its kind, whether
  it is a folder, and its location's space when that is another org's (a
  person's own computers are their personal org's). Also a kind for any
  one file inside a folder, and the song's other names (aliases).
- **Kinds:** chart PDF, keyflow chart, lyrics (LRC), click, guide, backing
  track, stems, multitrack, reference mix, MIDI, session (REAPER, Ableton,
  Logic, Pro Tools, …), video, other. They are recognised from Drive's
  extensions, then the name (`Click`, `CLK`, `Cues`, `Kick In`, `Drums`,
  `Instrumental`, `Rough mix`) and the folders above it (`… Stems`,
  `Multitracks`, `Audio Files`, `Bounced Files`). A folder whose name says
  nothing gets its kind from what is in it. Any kind can be changed by hand.
- **Find files:** searches every computer's indexes (the song's org and the
  person's own by default; any org can be added) for the title, its shorter
  forms and the aliases. Matches are ranked: named after the song, the
  artist in the path, a whole folder; old copies, backups and deep inbox
  folders rank lower. Mirrors and copies fold into one match, and what a
  matching folder holds comes with it. The box also takes Drive's search
  language (`holy kind:audio NOT backup`, `path:Multitracks/`). Each match
  can be previewed before it is linked.
- **Upload** puts a small file (a chart, an LRC, up to 50 MB) into the org's
  Drive tree under `Studio/Songs/<title>/` and links it.
- **The Files section** (song page): grouped by kind. Each file shows where
  it is (computer, folder, on or off, how many copies), its readiness, and
  its kind. A preview shows a waveform (the peaks sidecar, made on the
  computer that has the file) with a player streaming through the node, the
  first page of a PDF with Open chart, an LRC's or chart's text, and every
  other place the content is.
- **Readiness:** each file is Missing (in no index any more), Computer off
  (only on computers that are off), Reachable (a computer that has it is on),
  Kept on Drive (the node keeps a copy) or On this device. A song is its
  worst file, and also says what a set wants that it lacks (a chart; a click,
  or stems or a backing track). The set page's Files card says how many
  songs are ready offline, what is not yet kept, and which computers must be
  switched on.
- **Keep on Drive / Warm:** on the web, Keep on Drive marks the set's parts
  available, and each computer that has them puts them on the node. A
  desktop build with `--features local` warms them onto this device through
  Drive's agent (`drive-local`). They stay pinned while the set is open,
  and Open gives a real path.

Tests (`cargo test -p studio-app`): kinds; linking leaves a read-only folder's names,
bytes, times and modes exactly as they were; the song-files code holds no
file-writing call; sharing a song shares its project (and stops); finding
ranks and folds matches; the search language; readiness for files, songs
and sets. `cargo test -p studio-app --features local -- --ignored warm_and_hold` runs
against a running agent (`DRIVE_AGENT_DIR`, `STUDIO_WARM=TARGET,…`).

Gaps in Drive, filed: parts on another org's computers
(FastTrackStudios/drive#52), a nested `Doc::edit` that lost an edit
(FastTrackStudios/drive#53), inline PDFs and tree parts in `warm --project`
(FastTrackStudios/drive#54).

## Setlists (2026-11-22)

Sets are Drive documents too. `src/setlist.rs` is the model and its tests;
`src/sets.rs` holds the live hooks:

- `<org>/studio:setlist:<id>` holds one set:
  - `set`: title, kind (service, rehearsal, show, template), date, start
    and call times, place, recurring event, leader, group, the number of
    songs it wants, notes.
  - `entries`: a movable list. Each entry is a song (`owner/id` and its
    face), a section, or an item, with its overrides, notes and transition.
  - `band`, `rehearsals` and `shared_with`.
- `<org>/studio:sets` is the org's index: each set's face and the org's
  recurring events.
- What a song is played as comes from the entry, then the org's use, then
  the song.
- The set page:
  - On a desktop: drag a row to move it. The picked entry is edited in a
    column beside the list.
  - On a phone: the entry opens in a sheet, which also has Up and Down.
  - Add songs (the library and what is shared with the org, by search),
    sections and items.
  - The band, your part (keys, capo hints, what you lead) and rehearsals.
  - Share with another org, plain text to copy, and a printable page
    (`/print/<org>/<id>`).
- Chart and Lyrics follow the set's songs, with the key each is played in.
- A set shared from another org opens at `/at/<org>/<id>`. That org finds
  it through Drive's `/shared`, and the band's names travel in the set.
- Ported from Studio's `features/setlists`: the key labels and how a set's
  key moves them (`src/keys.rs`), named runs of a set, an arrangement per
  entry, notes per song, and who plays what.

## Organizations (2026-11-22)

Studio's orgs are Drive's orgs, which come from the auth server. The node's
`/orgs` lists the ones you are in, and switching org moves every app on
the address. Each org keeps its own documents (`src/orgs.rs` is the model,
`src/team.rs` the live hooks):

- `<org>/studio:org` is the org's music team:
  - `settings`: the name Studio shows, the default arrangement, the
    rehearsal room.
  - `groups`: a worship team, a band, a teacher's students.
  - `people`: each person's role (leader, member, teacher, student, tech,
    guest), the parts they play, the groups they are in and the groups
    they lead, and the issuer `account` that is them. A signed-in person
    finds themselves by that account, or claims a person with "This is me".
- `<org>/studio:library` holds the org's own songs. Each song has a
  `shared_with` map of the orgs it is shared with. `uses` records how this
  org plays a song, whether its own or one shared with it (`owner/id` →
  key, arrangement, who leads). The key an org plays a song in is its
  use's key, falling back to the song's own.
- Sharing:
  - The owner's library is the source of truth.
  - Whoever has the owner's library open mirrors it into
    `<owner>/studio:shared:<to>` (the shared songs' faces) and into a
    `studio-shared-<to>` project in the owner's `drive:space`.
  - That project's documents are the digest and the songs' sessions, and
    its `shared_with` is `to`. The node lets members of `to` open exactly
    those documents.
  - The receiving org finds its owners through Drive's `/shared`.
- Versions (`drive_spine::schema`):
  - Library version 2: each built-in song lives in its home org. Christmas
    songs go to CBU, the recital's songs to Rockstars of Tomorrow and the
    worship rotation to Crossroads Church. If another org had changed a
    song's key, that key is kept as that org's use. Songs people added
    stay where they are. The home orgs start sharing a few songs.
  - The org document's version 1 seeds a team based on the kind of org.

Pages:

- All orgs (`/orgs`): every org's songs, people, leaders, what it shares
  and what is shared with it.
- People (`/people`).
- Organization (`/org`): settings and groups.
- Library → Shared with us (`/library/shared`), and a shared song's page
  (`/shared/:owner/:id`).
- On every song page, the "In this org" row opens the key, arrangement,
  leader and sharing for this org.

Without Drive (the desktop and phone builds for now), the same documents
live in memory, and the org is picked in the app.

Tests: `cargo test -p studio-app -p session-sync` (the org model, the songs,
session-sync).

## Songs are sessions (2026-11-21)

The library and every song live on Drive. Each song is a **session
document**: `crates/session-sync`, the engine-free half of the studio
repo's `features/session/src/sync` (the `SessionDoc` schema, the model,
the diff, presence, transport, crop, hold, align), copied unchanged
except for paths, `web_time::Instant`, and a local `Position` in place of
the DAW repo's. Its 12 tests run here. It is the block everything else
agrees on: the song page edits the same document a DAW session of the
song opens.

- `studio:library`: each song's face (title, artist, key, tempo, tags,
  energy), seeded once from the built-in list; new songs from the
  Library page.
- `studio:song:<id>` (id: the library's `slugify` of the title): a
  `SessionDoc`. Its tempo map, its sections as regions on ruler lane 1
  (laid end to end in bars), its chart (a Loro text: concurrent typing
  merges), the key in the project's ext-state (`studio/key`).
- The song page (`/song/:id`): key, tempo and meter (changing the tempo
  keeps every section's length in bars), the form (add, rename, resize,
  move, repeat, remove sections), the chart, and who else is in the song
  and on which section. Without Drive (desktop and phone builds for now)
  the same documents live in memory.
- `src/songs.rs` tests: a song round-trips through its session, tempo
  and meter keep bars, and two people's concurrent edits both land.

Known: two people adding a section at the same moment can leave two
sections overlapping until the next edit lays the form out again.

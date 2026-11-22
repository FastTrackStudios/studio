# studio-proto

Studio, remade from the launcher prototype as a plain Dioxus 0.8 app: phone first, then desktop and web. On the web it runs on Drive (`../drive`), served by a Drive node at `/studio`. The way to it replacing Studio: [docs/ROADMAP.md](docs/ROADMAP.md).

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

Tests: `devenv shell -- cargo test --workspace` (the org model, the songs,
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

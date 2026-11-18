# The project launcher

Status: design, with a working first version (`extensions/launcher`,
id `studio.launcher`). Written 2026-11-18. Mockups:
[`project-launcher/index.html`](project-launcher/index.html).

The launcher is where a person chooses what Studio works on: a setlist to
play or rehearse, a song to work on, a rig to load. It replaces
`stack/run-studio.sh` in the local stack, which today makes those choices
by hand (a vault, a setlist file, a chart, a cue file, a layout over three
displays), and it replaces `studio-desktop`'s welcome screen when
`studio-desktop` goes.

It is an Architect extension, so a development copy shadows the compiled-in
one and reloads on save (`docs/architect-distribution.md`).

## Three rules from the owner (2026-11-18)

These shaped the design after a first draft that got them wrong.

1. **Architect's shell and navigator are the framework.** The launcher
   draws no sidebar, top bar, org switch or phone tab bar of its own. On a
   desk it is a navigator provider and a pane; on a phone it is a tab of
   Architect's phone shell, which iOS already has.
2. **The song list already exists.** A setlist is drawn as Architect's
   media list, the Spotify-style list a setlist's own page plays from. The
   launcher uses that list; it does not design another.
3. **It is a list to look at and load from.** No readiness dots, no
   counts of what each song has, no "what is missing" panel. A nice song
   list, a press to load a set or a song.

## Who uses it, and for what

| Person | Moment | What they need |
|---|---|---|
| The music director, days before | Planning the set | The running order in its sets, to reorder and annotate |
| The same person, at the venue | Opening the show | One press that opens the set in the engine |
| An engineer or editor | Opening a project | What was open last, recent songs |
| A player | Loading a rig | Their guitar or keys rig |
| A band member on a phone, or a share link's guest | Looking at the set | The setlist and its charts |

## What it is made of

Three existing pieces and one new extension that joins them.

| Piece | Whose | What the launcher does with it |
|---|---|---|
| **The shell**: rail, tab strip, docks on a desk; the phone body and its foot of tabs on a phone (`architect_shell::FormFactor`, `features/shell/src/shell/bar.rs` `PhoneFoot`, `features/workspace/src/host/overlay.rs` `PhoneBody` in Architect) | Architect | Nothing. The launcher's view is a pane in it, and its provider is a row in the navigator, so the phone layout comes with the shell |
| **The navigator** and its providers (`architect_navigator_api::NavigatorProvider`) | Architect; the setlists extension already provides **Setlists** and **Libraries** (`features/setlists/src/nav.rs` `SetlistsNav`, `LibrariesNav`), productions provides projects | Adds one provider, **Open** (`extensions/launcher/src/nav.rs` `LauncherNav`): Recent, Setlists, Rigs |
| **The media list** (`architect_media::ui::MediaList` in Architect: a pill, each section's name, each row's number, 48 px tile, title over artist, length; a row with nothing to play greyed; a phone grid of its own) and the tile rule of the setlist's share page (`setlists::row::tile_hue`, `features/setlists/src/share.rs` `tile`: the song's key on the song's colour) | Architect and the setlists extension | The launcher's page for a setlist is this list. See "The song list" for how |
| **The engine** (`extensions/daw`: `DAW_OPEN`, `DAW_RUN`, `DAW_STATE`) | Studio | What a press on the pill or a row runs |

## The objects, where each lives and how it is found

| Object | Where it lives | How the launcher finds it |
|---|---|---|
| **Setlist** | A vault note of type `setlist`, `Setlists/<name>.md`: `title`, `date`, `songs`, `sets` (`[{name, songs}]`), `project` (`features/setlists/src/setlist.rs` `Setlist`, `Set`) | A vault query on `type == "setlist"` through the host (`HostRequest::QueryVault`) |
| **Song** | A vault note of type `song`, `Libraries/<lib>/<Song>/<Song>.md`: `artist`, `key`, `key_end`, `tempo`, `length`, `chart`, `lyrics`, `media`, `session` (`features/setlists/src/song.rs` `Song`) | A vault query on `type == "song"` |
| **Project** | A vault note `Projects/<name>/project.md` (`features/productions/src/model.rs` `keys`), and a heavy file root `<org>/Projects/<name>/` beside the vault | The setlist's `project` link, else its title |
| **Session** (a song's tracks) | A `.session` or `.RPP` in the project's file root, such as `songs/drive-to-la/Drive To LA.session` | The song note's `session` when it has one; otherwise by name in the project root (`extensions/launcher/src/find.rs` `song_session`). The real song notes do not set it: decision 1 |
| **Setlist file** | A text file of session paths the engine opens as a set (`apps/session/engine/src/setlist.rs` `read_setlist`) | The setlist note's `session`, else a `.setlist` in the project root, else written by the launcher from its songs' sessions (`find.rs` `write_setlist`) |
| **Recent** | The engine's list, `recent.tsv` (`apps/session/engine/src/projects.rs` `recent`) | `DAW_RUN {id: "SESSION_RECENT"}` |
| **Rig** | An instrument (`signal_rigs_proto::Rig`) and its saved profiles, `~/.config/signal/<instrument>/profiles/<name>.styx` (`guitar::RigHome`) | The profile files' names (`find.rs` `rigs`) |

The vault is the light layer (notes, charts, lyrics, recordings), synced to
every device. The project file root is the heavy layer (sessions and their
audio), held only by machines that pin it
(`/run/media/Development/vaults/README.md`). So every device can show a set,
and only a machine with the project can load its tracks. The list shows that
the way the media list already shows "nothing to play": the row is greyed.

## The screens

### In the navigator

The **Open** provider sits at the top of Architect's navigator
(`extension.toml` `[[navigator_providers]]`, `section = "top"`).

```
Open
├── Recent      what the engine opened lately; a press opens it (DAW_OPEN)
├── Setlists    each setlist; a press shows its song list in the launcher
└── Rigs
    └── guitar  its saved profiles
```

The vault's own trees stay where they are: Setlists and Libraries are the
setlists extension's, Projects is productions', the note tree is
Architect's. Open is the short list a person reaches for first; the others
answer "where is this note".

### The song list ([desk](project-launcher/desktop-setlist.html), [phone](project-launcher/phone-setlist.html))

A setlist's page is the media list:

- The setlist's title, then the pill, **Open set**, and one line of what it
  holds (songs, total time, date).
- Each set's name ("Act I: “The Originals”") with its count and time.
- Each row: its number, a 48 px tile with the song's key on the song's
  colour (a note glyph when the song has no key yet), the title over the
  artist, the tempo, the length.
- The row of the song the engine has open is in the accent, as the playing
  row is in the media list.
- A song with no tracks on this machine is greyed and its press does
  nothing, as a row with nothing to play is in the media list.

A press on a row opens that song. When the engine already has the set, it
goes to the song in it, so the set stays open.

On a desk a song can also show beside the list (Architect's right dock):
its chart's first page, **Open this song**, and links to its chart, lyrics,
reference and lighting cues. It is a place to go from, not a status report.

On a phone it is the same list in one column, in Architect's phone shell.
The tempo drops out, as the media list drops its length at that width.

### Plan ([mockup](project-launcher/desktop-plan.html))

The same list with a grip on each row to drag, a line of notes per song
("Key of A. Key of C and Numbers are arrangements"), sets that can be
added, renamed and reordered, and the libraries' other songs in the
navigator to drag in. A set can be marked as not in the running order
(Merry and Bright's "More Concert Set options"): it draws dimmed and Open
set leaves it out.

Every edit is a write to the setlist note through the setlists extension
(`STUDIO_SETLISTS_MOVE`, `_ADD`, `_REMOVE`), so it syncs and is undoable
like any note edit.

### The window ([mockup](project-launcher/desktop-chooser.html))

On a desk the launcher can be a window of its own, as Logic's project
chooser is. It is an Architect window holding the launcher's pane, not a
different view: `STUDIO_LAUNCHER_WINDOW` runs `WORKSPACE_OPEN` with
`at: {window: {}}`. Its home page, when no setlist is chosen: **Carry on**
(what the engine had open last), Recent, the setlists, the rigs.

### Rigs ([mockup](project-launcher/desktop-rigs.html))

An instrument's saved profiles. **Play this rig alone** opens the rig view.
Loading one beside an open song is not built: see Rigs below.

## What each action does

"Launcher" actions are `extensions/launcher`'s own; the engine's are reached
through `extensions/daw`.

| Action | What runs | State |
|---|---|---|
| Open the launcher as a pane (⇧⌘O, ⌘K "Launcher: open") | `STUDIO_LAUNCHER_SHOW` → `EXT_VIEW_OPEN` | Built |
| Open it as a window | `STUDIO_LAUNCHER_WINDOW` → `WORKSPACE_OPEN {view, at: {window: {}}}` | Built |
| Show a setlist's song list | `STUDIO_LAUNCHER_CHOOSE {setlist}`; the navigator's rows run it | Built |
| Open a set | `STUDIO_LAUNCHER_OPEN_SETLIST` → `DAW_OPEN {path}` on the setlist's file, or on one written from its songs' sessions; answers which songs were left out | Built |
| Open a song | `STUDIO_LAUNCHER_OPEN_SONG` → `DAW_RUN {id: "SESSION_SONG", args: {to}}` when the open set has it, else `DAW_OPEN {path}` | Built |
| Recent | `STUDIO_LAUNCHER_RECENT` → `DAW_RUN {id: "SESSION_RECENT"}`; a row runs `DAW_OPEN` | Built |
| The navigator's tree | `STUDIO_LAUNCHER_NAV_SNAPSHOT` | Built |
| List setlists as data, for a script or an agent | `STUDIO_LAUNCHER_LIST` | Built |
| List rigs | `STUDIO_LAUNCHER_RIGS` (profile files) | Built, listing only |
| Carry on | `DAW_OPEN` on the engine's last open (`session_engine::studio_engine::link::remember_open`) | Missing 4 |
| Reorder, add, remove a song | `STUDIO_SETLISTS_MOVE`, `_ADD`, `_REMOVE` (`features/setlists/extension.toml`) | Exist; the view does not call them yet |
| New setlist; share with the band | `STUDIO_SETLISTS_CREATE`; `STUDIO_SETLISTS_SHARE` | Exist |
| Rename, reorder or bench a set; a note on a song | — | Missing 2 |
| Arrange the views for the show | `WORKSPACE_APPLY` with a named layout, then `HOME_SLOT_SET`, as `run-studio.sh` does | Missing 6 |
| Point the chart and lighting at the song | `KEYFLOW_SESSION_FOLLOW`, `IGNITION_SESSION_SONG`, `IGNITION_SESSION_ROOM`, as `run-studio.sh` and `follow-song.sh` do | Missing 7 |
| New song | `DAW_RUN {id: "SESSION_NEW"}`, then a song note | Missing 5 |
| Load a rig | `RIG_LOAD` | Missing 8 |

### Missing, and where each belongs

1. **A song note that names its session** (`session`, relative to the
   project root), written by whatever makes the session. The key exists in
   `features/setlists/src/song.rs`; nothing resolves it into
   `<org>/Projects/<name>`. Until then the launcher finds sessions by name.
2. **Set and note edits** in `features/setlists`: `STUDIO_SETLISTS_SET_RENAME`,
   `_SET_MOVE`, `_SET_BENCH {section, on}`, `_NOTE {setlist, song, note}`.
3. **Recents as data.** `SESSION_RECENT` answers text lines for people,
   which the launcher parses (`extensions/launcher/src/model.rs`
   `Recent::parse`). It should answer rows; in
   `apps/session/engine/src/actions.rs`.
4. **`DAW_LAST`** in `extensions/daw`: what the engine opened last and where
   its playhead was, for Carry on.
5. **New song into a library**: one action that makes the song note, its
   folder in the project root and an empty session there (`SESSION_NEW`
   makes an untitled song and saves nowhere). In `features/setlists`.
6. **Named layouts**: `STUDIO_LAUNCHER_ARRANGE {layout}` applying a saved
   workspace layout and the title slot. `run-studio.sh`'s embedded Python,
   which computes the splits from the display's shape, becomes layout files
   ("Stage", "Rehearse", "Edit"). In `extensions/launcher`.
7. **The song changed, as an event** from `extensions/daw`, so Keyflow and
   Ignition follow by themselves and `follow-song.sh` (which polls
   `DAW_STATE`) is deleted.
8. **Rigs in the app's engine**: below.
9. **The media list for a view that is not a note's head** (in Architect,
   not changed here). `MediaList` takes rows that play a recording through
   the playhead and reads the shell from context (`use_shell`), which a
   development copy (a process) does not have. For the launcher to mount it
   rather than follow it, it needs a press it can be given (`on_press`) and
   a row's art to be a tile (key and hue), and to be reachable from a
   process view. Until then `extensions/launcher/src/view.rs` draws the same
   grid by hand, with the same tile rule. This is the one piece of
   duplicated drawing, and the first thing to remove.
10. **A start view** (in Architect): nothing lets a distribution name the
    view shown when nothing is open (`features/workspace/src/host/mod.rs`
    `EmptyCenter` shows "Nothing open").

## Rigs

A rig is identified by instrument (`signal_rigs_proto::Rig`, a slug such as
`guitar` or `keys`) and, within it, by a saved profile, a styx file under
Signal's config folder (`guitar::RigHome`). No service lists rigs, and
nothing ties a rig to a setlist outside `features/library/src/model.rs`
(`Rig`, `SongPreset`), which only its fixture fills.

One is loaded today by `studio-desktop --guitar` or `--keys`
(`apps/studio/desktop/src/front.rs` `preset_from`), which opens the rig view
on its own engine; inside a song, `RIG_LOAD {track, instrument, profile,
patch}` puts a rig on a track (`apps/session/engine/src/rig_actions.rs`),
for guitar and bass only, and only when `session-engine` is built with its
`signal` feature. `extensions/daw/Cargo.toml` builds it without, so the
app's engine has no `RIG_LOAD`.

So the first version lists rigs and says loading is not built. Loading
needs, in order: `extensions/daw` built with `signal` (the guitar crate and
its captures come into the engine's build, `rig-library/README.md`); Signal
as an extension providing a rigs service, so the launcher asks it instead of
reading Signal's folder, and keys, drums and synth get a load path; and
parts (which rig a setlist uses, which patch a song takes) stored on the
setlist note so they sync (decision 5).

## How it shows

| Where | Form |
|---|---|
| Desktop | A pane in Architect's shell, with Open in the navigator; ⇧⌘O or ⌘K opens it |
| Desktop, as a chooser | The same pane in a window of its own (`STUDIO_LAUNCHER_WINDOW`) |
| Web app at 390 px, iOS | A tab of Architect's phone shell: the same view, one column |

Relation to what exists:

- **The songs bar and the set navigator** (`extensions/navigator`
  `SongsPane`, `NavigatorPane`; `session_ui::shell::SongTabs`,
  `session_ui::navigator::Navigator`) show the set the engine has open, with
  the playhead, during the show. The launcher shows the sets in the vault,
  before it. They meet at one point: a press on a song whose set is open
  goes to it (`SESSION_SONG`), exactly as a tab does.
- **The setlist's own page** (the note, with the media list at its head) is
  for reading and for playing recordings, and is what a share link shows.
  The launcher's list looks the same and loads tracks instead.
- **`studio-desktop`'s welcome screen** (`features/library` `ui`, mounted by
  `apps/studio/desktop/src/library_home.rs` `LibraryHome`; and
  `apps/session/desktop/src/native/start.rs` `Start` with its `Hero`,
  `Shelf`, `Cover`, `RecentRow`) has the right ideas over the wrong store
  (Task's library, files on the device) and its own shell. The launcher
  keeps Carry on and Recent from it and drops the shell.
- **The library editor** (`apps/session/desktop/src/native/library.rs`
  `LibraryEditor`) is Plan's ancestor: its lists, order and songs-to-add
  panes are the navigator, the list and the navigator's libraries.
- **The setup view** (`apps/session/ui/src/setup.rs` `SetupView`) keeps
  routing and settings; its song table is superseded by this list.

## A share link's guest ([mockup](project-launcher/phone-guest.html))

A guest is the same app with fewer capabilities, never another page. A
guest sees the shared setlist as the same song list and plays its
recordings, which is the media list as it is today. A guest does not get
the Open provider's Recent and Rigs, and no row loads tracks: those need the
engine's actions and a member's disk, which a guest is not granted
(`architect_extensions::share_needs::guest_capabilities` drops `actions`).
So for a guest the launcher adds nothing to the setlist's own page, and
should not try to.

## Decisions for the owner

1. **How a song says where its session is.** (a) The song note names it,
   relative to the project root. (b) By convention: a folder named for the
   song. *Recommend (a), with (b) as the fallback it is today.* A name is a
   guess ("DTLA Cinematic Intro" is in `songs/cinematic-intro`).
2. **One vault at a time, or every org.** An Architect window is on one
   vault, and the two real collections are in two orgs. (a) The launcher
   shows the open vault; Architect's org switch changes it. (b) Recent and
   upcoming setlists across every org. *Recommend (a) now, (b) later for
   Recent only.*
3. **What launch shows.** (a) Always the launcher. (b) The launcher only
   when the engine has nothing open; otherwise the app as it was.
   *Recommend (b):* at a venue the app must come back as it was.
4. **Does the window close when something opens.** *Recommend yes as a
   window, no as a pane.*
5. **Where a note on a song in a set, and a rig's parts, live.** *Recommend
   the setlist note's frontmatter* (`notes`, `parts`): both are about this
   set and must sync to the band.
6. **What Open set arranges.** (a) Nothing. (b) The setlist's saved layout
   ("Stage" or "Rehearse"), which is the part of `run-studio.sh` people
   will miss most. *Recommend (b), remembered per setlist.*
7. **The tile.** The setlist's page shows cover art where a song has it and
   a key tile where it does not. The launcher's first version always shows
   the key tile, since the key is what a player reads. *Recommend the cover
   with the key over its corner once the media list takes a tile (missing
   9).*

## Left out of the first version

- Plan: no reordering, sets or notes in the view.
- The song beside the list, Carry on, search, New song, New setlist.
- Loading a rig; parts.
- Arranging the views and pointing the chart and lighting at the song.
- Mounting `MediaList` itself (missing 9): the view follows its grid.
- Opening on another machine's engine from a phone.

## The first version, as built

`extensions/launcher` (`studio-launcher`, id `studio.launcher`), a process
extension that is also linkable (`studio_launcher::Launcher`), with no
dependency on Session's crates.

- `src/nav.rs`: the Open provider (Recent, Setlists, Rigs).
- `src/view.rs`: a setlist as the song list; the pill opens the set, a row
  opens its song; the foot says what the last press did and what the engine
  has.
- `src/model.rs`: setlists, sets and songs from the vault's rows. Pure.
- `src/find.rs`: the project root beside the vault, a song's session by its
  note or its name, rig profiles, the written setlist file. The only code
  that reads the disk.
- `src/host.rs`: the vault query, `VAULT_STATUS`, `DAW_RUN`.
- `src/lib.rs`: the actions.

Checked as a development copy in a second, isolated instance of the app
(`studio dev --name launcher-test --extension <package>` on a copy of the
vault's notes, in a headless desktop session, with its own engine):
`extensions check` passes and renders the view; the Open provider shows in
the navigator; `STUDIO_LAUNCHER_OPEN_SONG` opened a song,
`STUDIO_LAUNCHER_OPEN_SETLIST` opened a four-song set and named the songs it
left out, a second song press went to song 3 of the open set, and a rebuilt
program was picked up without restarting the app.

To link it into the distribution, two lines in `apps/architect` (not done
here: that file is changing):

```toml
# apps/architect/Cargo.toml, [dependencies] (and "dep:studio-launcher" in `gui`)
studio-launcher = { workspace = true, optional = true }
```

```rust
// apps/architect/src/lib.rs, in `stage()`
        .member(&studio_launcher::Launcher)
```

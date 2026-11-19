# The project launcher

Status: built (`extensions/launcher`, id `studio.launcher`, linked into
Studio in `apps/architect`), with its decisions taken. Written 2026-11-18.
Mockups: [`project-launcher/index.html`](project-launcher/index.html).
It needs Architect's `feat/media-list-press` (the media list's press and
tile).

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
| **The media list** (`architect_media::ui::MediaList` in Architect: a pill, each section's name, each row's number, 48 px tile, title over artist, length; a row with nothing to play greyed; a phone grid of its own) and the tile rule of the setlist's share page (`setlists::row::tile_hue`, `features/setlists/src/share.rs` `tile`: the song's key on the song's colour) | Architect and the setlists extension | The launcher's page for a setlist is this list, mounted (`extensions/launcher/src/view.rs` `LauncherView`), given the launcher's press and each song's tile |
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
through `extensions/daw`. All of these are built.

| Action | What runs |
|---|---|
| Open the launcher as a pane (⇧⌘O, ⌘K "Launcher: open") | `STUDIO_LAUNCHER_SHOW` → `EXT_VIEW_OPEN` |
| Open it as a window | `STUDIO_LAUNCHER_WINDOW` → `WORKSPACE_OPEN {view, at: {window: {}}}`, or `WORKSPACE_MOVE` when it is already a pane |
| At launch | `STUDIO_LAUNCHER_STARTUP`, on the app's `startup` hook: the window when the engine has nothing open |
| Show a setlist's song list | `STUDIO_LAUNCHER_CHOOSE {setlist}`; the navigator's rows run it |
| Open a set | `STUDIO_LAUNCHER_OPEN_SETLIST {setlist, layout?}` → `DAW_OPEN {path}` on the setlist's file, or on one written from its songs' sessions; then the layout, then the chart and lighting; answers which songs were left out |
| Open a song | `STUDIO_LAUNCHER_OPEN_SONG` → `DAW_RUN {id: "SESSION_SONG", args: {to}}` when the open set has it, else `DAW_OPEN {path}`; then the chart and lighting |
| Lay the panes out | `STUDIO_LAUNCHER_ARRANGE {layout}`: Stage, Rehearse, or a saved one |
| Save the panes as a layout | `STUDIO_LAUNCHER_LAYOUT_SAVE {name}` → `WORKSPACE_EXPORT`, kept on the device; `STUDIO_LAUNCHER_LAYOUTS` lists them |
| Point the chart and lighting at the song | `STUDIO_LAUNCHER_FOLLOW`, on the `app:playhead.changed` hook → `KEYFLOW_SESSION_FOLLOW`, `WORKSPACE_NAVIGATE`, `IGNITION_SESSION_SONG`, `IGNITION_SESSION_ROOM` |
| A note on a song in a set | `STUDIO_LAUNCHER_NOTE {setlist, song, note}` → `VAULT_SET_PROPERTY` on the setlist's `notes` |
| A rig a set is played with | `STUDIO_LAUNCHER_PART {setlist, player, rig, profile?, song?, patch?, remove?}` → the setlist's `parts` |
| Name each song's session in its note | `STUDIO_LAUNCHER_NAME_SESSIONS {setlist}` → `VAULT_SET_PROPERTY` on each song's `session` |
| Recent | `STUDIO_LAUNCHER_RECENT` → `DAW_RUN {id: "SESSION_RECENT"}`; a row runs `DAW_OPEN` |
| The navigator's tree | `STUDIO_LAUNCHER_NAV_SNAPSHOT` |
| List setlists as data, for a script or an agent | `STUDIO_LAUNCHER_LIST` (each song's `session_from`: `note` or `name`) |
| List rigs | `STUDIO_LAUNCHER_RIGS` (profile files; loads nothing) |

Reordering, adding and removing songs are the setlists extension's
(`STUDIO_SETLISTS_MOVE`, `_ADD`, `_REMOVE`, `features/setlists/extension.toml`);
the view does not call them yet.

### Layouts

A layout is applied by name (`extensions/launcher/src/arrange.rs` `arrange`).

- **Stage** is the layout `stack/run-studio.sh` built by hand: the panes
  of the last layout closed; the arrangement, mixer, chart, lighting room,
  lyrics, progress bar and cues opened; the Now Playing bar, status bar,
  bottom dock and tab bars hidden and the set's songs put in the title row;
  the chart and the room moved to windows of their own; then the
  workspace's export rewritten and applied (`src/layout.rs` `stage`). The
  main window is the arrangement with the mixer beside it when the window
  is at least 2.4 times wider than tall, under it otherwise, below the
  song's progress bar; the chart is over the lyrics; the room has its cues
  to the right. The script's embedded Python is `layout.rs`, with tests.
- **Rehearse** is one window: the arrangement over the mixer, the chart
  over the lyrics beside them, the app's bars as they are (`layout.rs`
  `rehearse`).
- **A saved layout** is the workspace's export as it was
  (`STUDIO_LAUNCHER_LAYOUT_SAVE`), kept in the extension's data on the
  device, since displays differ by machine. Applying it closes the panes
  before it, applies it, and points the chart at the engine's song.

A step whose extension is not there (no lighting, no chart) is skipped and
named in the answer; the rest still lays out. The layout a set was opened
in is written to its note (`layout`), so Open set applies it next time
without being told. The name syncs; the layout itself is each device's.

Not carried over from the script: the Space key bound to play and pause in
every pane (it wrote the app's `keys.json` directly; there is no action for
a key binding yet).

### The chart and the lighting follow the song

`stack/follow-song.sh` polled the engine and kept a table of the names that
differ between a session's folder, its song's note and its cue file. The
launcher needs no table: the song whose session the engine has is a place
in a setlist (`src/follow.rs` `target`), and that place has its chart (the
song note's `chart`, else the `.kf` beside the note) and its cues
(`Projects/<project>/lighting/cues/<slug of its label>.json`). The
project's Live set is `show/<name>.als` in the project root, or
`IGNITION_LIVE_SET`.

It runs after every open the launcher makes, after a layout is applied,
and on the deck's moves (`[[hooks]] on = "app:playhead.changed"`), where it
asks the engine at most once in 900 ms and does nothing while the song is
the one last pointed at.

### Still missing, and where each belongs

1. **The engine's save writing a song's `session`** into its note. The
   launcher reads it and has an action that writes it for a whole set
   (`STUDIO_LAUNCHER_NAME_SESSIONS`); a session made later is found by name
   until that action runs again.
2. **Set edits** in `features/setlists`: rename, reorder and bench a set.
3. **Recents as data.** `SESSION_RECENT` answers text lines for people,
   which the launcher parses (`src/model.rs` `Recent::parse`); in
   `apps/session/engine/src/actions.rs`. Recents are the engine's, so they
   are one machine's and one org's; across orgs needs the account.
4. **`DAW_LAST`** in `extensions/daw`: what the engine opened last and where
   its playhead was, for Carry on.
5. **New song into a library**: one action that makes the song note, its
   folder in the project root and an empty session there.
6. **The song changed, as an event** from `extensions/daw`. The deck's
   moves stand in for it, and fire far more often than a song changes.
7. **A key binding as an action** (Architect), for Stage's Space.
8. **Rigs in the app's engine**: below.

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

So the launcher lists rigs, and a setlist's note keeps its **parts**
(`parts: [{player, rig, profile, patches}]`, written by
`STUDIO_LAUNCHER_PART`), and nothing is loaded. Loading takes, in order:

1. **The engine built with Signal.** `extensions/daw/Cargo.toml` asks
   `session-engine` for `features = ["native"]`; it needs `"signal"` too
   (`apps/session/engine/Cargo.toml`: `signal = ["native", "dep:signal-fx",
   "session-runtime/signal"]`). That brings `signal-fx` and the guitar crate
   into the engine's build, whose build script embeds the captures and
   frozen Cores from `SIGNAL_RIG_LIBRARY` (`rig-library/`, linked by `just
   assets`; a release build refuses to start without them). The engine's
   binary grows and its build needs the private `rig-models` assets on every
   machine and runner that builds it.
2. **A track to put it on.** `RIG_LOAD {track, instrument, profile, patch}`
   loads onto an existing track, so the launcher first runs
   `TRACK_ADD_INSTRUMENT {instrument}` (Guitar, Bass or Drums), then
   `RIG_LOAD` through `DAW_RUN`. Both exist once step 1 is done.
3. **The right profiles.** `RIG_LOAD` reads profiles from Session's own
   copy (`signal_fx::signal_rig::library_dir`, `<config>/fts/<instrument>-rig`),
   not from the player's live rig folder the launcher lists
   (`~/.config/signal/<instrument>/profiles`). One of the two has to become
   the other's source, or the launcher lists what `RIG_BROWSE` answers.
4. **Keys, drums and synth.** They have no `RIG_LOAD` at all: only guitar
   and bass are `signal_fx` rigs. They need Signal as an extension with a
   rigs service, or `one-engine.md`'s step 5 (Session hosting rigs as
   `signal_rig_host::engine::Engine::add_rig` does).
5. **Per song.** A part's patch for the engine's song applied on each song
   change, from the same hook the chart and lighting follow.

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

## Decisions (taken 2026-11-18)

1. **A song's note names its session** (`session`, a path in the project
   root); a session found by the song's name is the fallback.
   `STUDIO_LAUNCHER_NAME_SESSIONS` writes the names found.
2. **The open vault now; Recent across orgs later.** Nothing built for the
   second: it needs the account's orgs, not one engine's list.
3. **At launch the launcher shows only when the engine has nothing open.**
   The engine comes up after the window, so it is asked for up to 8 s (90 s
   while it is opening a set); a song there ends the wait.
4. **As a window it closes when something opens; as a pane it stays.** "A
   window" is the launcher alone in a window other than the main one
   (`src/lib.rs` `has_own_window`).
5. **A note on a song in a set, and a set's rig parts, are the setlist
   note's frontmatter** (`notes`, `parts`).
6. **Open set applies a layout, remembered per setlist** (`layout`).
7. **Cover art with the key over its corner.** Built in the media list; a
   song with no cover shows the key on its colour.

## Left out

- Plan in the view: no reordering, no set edits, and a note is set by its
  action, not typed in the list.
- The song beside the list, Carry on, search, New song, New setlist.
- Loading a rig.
- A name field to save a layout from the view (the action saves one).
- Opening on another machine's engine from a phone.

## As built

`extensions/launcher` (`studio-launcher`, id `studio.launcher`), linked into
Studio (`apps/architect/src/lib.rs` `stage`) and runnable as a process, so
a development copy shadows it and reloads on save. No dependency on
Session's crates.

- `src/nav.rs`: the Open provider (Recent, Setlists, Rigs).
- `src/view.rs`: a setlist as Architect's media list; the pill opens the
  set in the layout picked beside it, a row opens its song; the foot says
  what the last press did and what the engine has.
- `src/model.rs`: setlists, sets, songs, notes and parts from the vault's
  rows. Pure.
- `src/find.rs`: the project root beside the vault, a song's session by its
  note or its name, cues, the Live set, covers, rig profiles, the written
  setlist file. The only code that reads the disk.
- `src/layout.rs`: Stage and Rehearse as rewrites of the workspace's
  export. Pure.
- `src/arrange.rs`: the steps that open the panes and apply a layout.
- `src/follow.rs`: the chart and the lighting pointed at the engine's song.
- `src/plan.rs`: what a setlist's note keeps (notes, parts, layout) and a
  song's session, written through the vault.
- `src/host.rs`: the vault query, the vault's properties, the extension's
  data, `DAW_RUN`.
- `src/lib.rs`: the actions, the launch rule, the opens.

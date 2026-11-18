# The project launcher

Status: design, with a working first version (`extensions/launcher`,
id `studio.launcher`). Written 2026-11-18. Mockups:
[`project-launcher/index.html`](project-launcher/index.html).

The launcher is where a person chooses what Studio works on: a setlist to
play or rehearse, a song to work on, a rig to load. It is also where a live
set is planned: what is in it, in what order, and what each song still
lacks. It replaces `stack/run-studio.sh` in the local stack, which today
makes those choices by hand (a vault, a setlist file, a chart, a cue file,
and a layout over three displays), and it replaces `studio-desktop`'s
welcome screen when `studio-desktop` goes.

It is an Architect extension, so a development copy shadows the compiled-in
one and reloads on save (`docs/architect-distribution.md`).

## Who uses it, and for what

| Person | Moment | What they need from it |
|---|---|---|
| The music director, at a desk, days before | Planning the set | The running order in its sets, each song's key, tempo and length, the set's total time, notes, and a list of what is missing |
| The same person, at the venue | Opening the show | One press that opens the set in the engine and puts the views where they belong on the displays |
| An engineer or editor | Opening a project to work on | What was open last, recent songs, a song's versions and checkpoints |
| A player | Loading a rig | Their guitar or keys rig, the patch each song takes |
| A band member on a phone, or a share link's guest | Looking at the set | The setlist, the charts, what is ready |

The first two are the same person in two modes, so the launcher has two
modes over one list: **Open** (read, choose, go) and **Plan** (reorder,
annotate, fix).

## The objects, where each lives and how it is found

| Object | Where it lives | How the launcher finds it | Exists today |
|---|---|---|---|
| **Setlist** | A vault note of type `setlist`, `Setlists/<name>.md`. Frontmatter `title`, `date`, `songs`, `sets` (`[{name, songs}]`), `project`, `share` (`features/setlists/src/setlist.rs` `Setlist`, `Set`) | A vault query on `type == "setlist"` through the host (`HostRequest::QueryVault`) | Yes |
| **Song** | A vault note of type `song`, `Libraries/<lib>/<Song>/<Song>.md`. Frontmatter `key`, `key_end`, `tempo`, `meter`, `length`, `chart`, `lyrics`, `media`, `session`, `arrangements` (`features/setlists/src/song.rs` `Song`) | A vault query on `type == "song"` | Yes |
| **Library** | A note of type `song-library` (`features/setlists/src/layout.rs` `Library`, `features/setlists/src/vault.rs` `LIBRARY_KIND`) | `STUDIO_SETLISTS_LIBRARIES` | Yes |
| **Project** | A vault note `Projects/<name>/project.md` of type `project` with a `stage` (`features/productions/src/model.rs` `keys`, `Stage`), and a heavy file root `<org>/Projects/<name>/` beside the vault | `STUDIO_PRODUCTIONS_LIST` | Yes |
| **Session** (a song's multitracks) | A `.session` or `.RPP` in the project's file root, for example `Projects/JT's Senior Recital/songs/drive-to-la/Drive To LA.session` | The song note's `session` property when it has one; otherwise by name in the project root (`extensions/launcher/src/find.rs` `song_session`) | The property exists; the real song notes do not set it. See decision 1 |
| **Setlist file** | A text file of session paths the engine opens as a set (`apps/session/engine/src/setlist.rs` `read_setlist`) | The setlist note's `session`, else a `.setlist` in the project root, else written by the launcher from the songs' sessions (`find.rs` `write_setlist`) | The reader exists; the note key is new |
| **Lighting cues** | `Projects/<name>/lighting/cues/<slug>.json` in the vault (Ignition's) | By the slug of the song's label in the set (`find.rs` `cues`) | Yes, by convention |
| **Recent** | The engine's list, `recent.tsv` in the app's data folder (`apps/session/engine/src/projects.rs` `recent`, `opened`) | `DAW_RUN {id: "SESSION_RECENT"}` | Yes, as text lines |
| **Versions and checkpoints** | Beside the song (`docs/design/song-history.md`) | `SESSION_VERSIONS`, `SESSION_CHECKPOINT`, `SESSION_HISTORY` (`apps/session/engine/src/actions.rs`, `history.rs`) | Yes, for the open song only |
| **Rig** | An instrument (`signal_rigs_proto::Rig`: guitar, keys, drums, bass, …) and its saved profiles, `~/.config/signal/<instrument>/profiles/<name>.styx` (`guitar::RigHome`, `guitar::library::rig_dir`) | The profile files' names (`find.rs` `rigs`) | Listing yes; loading into the app's engine no. See Rigs |
| **Part** (a rig attached to a setlist, with a patch per song) | Modelled only in `features/library/src/model.rs` (`Rig`, `SongPreset`), filled only by its fixture | Not found: nothing stores it | No |

Two layers matter everywhere (`/run/media/Development/vaults/README.md`):
the **light** layer is the vault (notes, charts, lyrics, renders, reference
audio), synced to every device; the **heavy** layer is the project file
root (sessions and their audio), held only by machines that pin it. A phone
has the light layer only. So the launcher always knows what a song *is*,
and knows whether it can be *played from its tracks* only on a machine that
holds the project.

## Readiness

"Organizing a live set" comes down to five questions per song. The launcher
answers each with a mark, in the same column on every screen.

| Mark | Ready when | Read from |
|---|---|---|
| **Chart** | The song has a Keyflow chart | The note's `chart` (or its arrangement's) |
| **Lyrics** | It has timed lyrics | The note's `lyrics` (`.lrc`) |
| **Reference** | It has a recording to learn from or rehearse to | A playable file in the note's `media` (`reference/` or `renders/`) |
| **Tracks** | It has a session on this machine | The session found for it (above) |
| **Lights** | The project has cues for it | The cue file |

A mark is filled or empty. No percentages: a chart that exists but is wrong
is a note on the song, not a state the launcher can know. `features/library/src/model.rs`
`Ready` already has a three-way readiness (chart, audio, sounds) for the
fixture library; the launcher's five replace it when that crate is retired.

The real collections, read on 2026-11-18, show why this is the centre of
the design:

- **JT's Senior Recital** (16 songs in three acts, 1:01:42): 10 have a
  chart, 14 have lyrics, all have a reference, 10 have a session in the
  project root, all 16 have cues. Seven are ready on every count. Act II
  ("Royal Blood", 5 songs, 21:05) has no charts and no sessions at all.
- **Merry and Bright 2026** (12 songs in three sets, 1 December): 9 have a
  chart, and nothing else exists yet. No song has a length, so the set
  cannot be timed. The third set is options, not running order.

## Information architecture

One list of things to open on the left, one page for the thing chosen in
the middle, one inspector for the row chosen on the right. Narrow, the
three become a stack of pages.

```
Open (the launcher)
├── Carry on                 what the engine had open last, at its bar
├── Recent                   songs, setlists and rigs, newest first
├── Setlists ─► a setlist    Open mode / Plan mode
│                └─► a song  inspector: readiness, parts, versions
├── Libraries ─► songs not in a set
├── Projects  ─► a project's setlist, stage, documents
└── Rigs ─► an instrument    profiles, the patch per song
```

### The chooser window ([mockup](project-launcher/desktop-chooser.html))

What opens at launch when the engine has nothing open, and on ⇧⌘O at any
time. A small window, like Logic's project chooser.

- **Carry on**: the last thing the engine opened and where it was. One
  press reopens it. This is the most common action, so it is the largest
  thing in the window.
- **Recent**: a mixed list of songs, setlists and rigs, each with its kind,
  its org and when.
- **Coming up**: setlists with a `date` in the future, nearest first, with
  how ready they are in a sentence ("9 of 12 charted, none timed").
- **Setlists** and **Rigs**: the short lists.
- The foot: New song, New setlist, Open a file.

Choosing a setlist turns the chooser into the full launcher on that setlist.
Opening something closes the window (see decision 4).

### A setlist, Open mode ([mockup](project-launcher/desktop-setlist.html))

- **Head**: the title, a sentence of facts (sets, songs, total time, date,
  how many are ready), and three actions: **Open the set**, **Rehearse from
  song N** (the selected row), **Plan**.
- **The set to scale**: one block per song, as wide as the song is long,
  grouped by set, coloured green when every mark is filled and amber when
  not. The block of the song the engine has is outlined. This is the one
  picture of the show's shape: how long each act runs, where the unready
  songs sit. A setlist with no lengths shows equal hatched blocks, which
  says "untimed" without a word.
- **The running order**: a row per song with its number, title and artist,
  key (`C→Db` when it ends in another), tempo, length, and the five marks.
  Set names are rules across the list with the set's song count and time.
  A press selects; a double press or Return opens the song.
- **The inspector** for the selected song: its chart's first page, its
  facts, **Open in the engine**, each readiness line with what it found and
  a way to it (Open chart, Open lyrics, Play, Versions, Open cues), its
  parts (which rig, which patch), and its latest saves and checkpoints.

### A setlist, Plan mode ([mockup](project-launcher/desktop-plan.html))

The same list, editable.

- Rows gain a grip to drag, and a **notes** column (one line per song in
  this setlist: "Key of A. Key of C and Numbers are arrangements").
- Sets can be added, renamed, reordered and marked **not in the running
  order** (Merry and Bright's "More Concert Set options"), which draws them
  dashed and leaves them out of the total and of Open the set.
- **Before the show**: a short list above the rows of what is missing,
  grouped by kind, each with the action that fixes it ("3 songs have no
  chart: … Start a chart"). It is computed from the marks, so it empties as
  the work is done.
- The left column lists **songs not in this set** from the libraries, to
  drag in.
- **Share with the band** makes the setlist's share link
  (`STUDIO_SETLISTS_SHARE`).

Every edit is a write to the setlist note through the setlists extension,
so it syncs and is undoable like any note edit.

### Rigs ([mockup](project-launcher/desktop-rigs.html))

An instrument's saved profiles, which setlists use each, and for an attached
rig the patch each song takes. **Play this rig alone** opens the rig view.
**Load** adds it to what is open. Loading is not built; see Rigs below.

### Phone width and iOS ([home](project-launcher/phone-home.html), [a setlist](project-launcher/phone-setlist.html))

The same view at 390 px: the left list is the home page, a setlist is a
page pushed over it, the inspector is a sheet. Rows are 48 px tall. The
marks keep their five columns (30 px each) so the pattern of gaps reads at a
glance; key, tempo and length move under the title. The set to scale stays,
without numbers.

On a phone the engine is on another machine, and so are the sessions and the
rigs. The launcher says so once on the home page, lists the rigs with the
machine's name, and **Open** asks that machine's engine. The phone then
shows the chart, lyrics and transport, which is what a phone is for.

## What each action does

In terms of actions that exist. "Launcher" actions are `extensions/launcher`'s
own; the engine's are reached through the `daw` extension's `DAW_OPEN` and
`DAW_RUN` (`extensions/daw/src/lib.rs`).

| Action in the launcher | What runs | State |
|---|---|---|
| Open the launcher as a pane | `STUDIO_LAUNCHER_SHOW` → `EXT_VIEW_OPEN {extension, view}` | Built |
| Open it as a window | `STUDIO_LAUNCHER_WINDOW` → `WORKSPACE_OPEN {view, at: {window: {}}}` | Built |
| List setlists with readiness | `STUDIO_LAUNCHER_LIST` (a vault query, then `find.rs`) | Built |
| Open a setlist | `STUDIO_LAUNCHER_OPEN_SETLIST` → `DAW_OPEN {path}` on the setlist's file, or on one written from its songs' sessions; answers which songs were left out | Built |
| Open a song | `STUDIO_LAUNCHER_OPEN_SONG` → `DAW_RUN {id: "SESSION_SONG", args: {to}}` when the open set has it, else `DAW_OPEN {path}` | Built |
| Recent | `STUDIO_LAUNCHER_RECENT` → `DAW_RUN {id: "SESSION_RECENT"}`; a row opens with `DAW_OPEN` | Built (see missing 3) |
| Carry on | `DAW_OPEN` on the engine's last open (`last-open.json`, `session_engine::studio_engine::link::remember_open`) | Missing 4 |
| Rehearse from song N | Open the setlist, then `SESSION_SONG {to: N}` | Designed |
| Arrange the views for the show | `WORKSPACE_APPLY` with a named layout, then `HOME_SLOT_SET`, as `run-studio.sh` does | Missing 6 |
| Point the chart and lighting at the song | `KEYFLOW_SESSION_FOLLOW`, `IGNITION_SESSION_SONG`, `IGNITION_SESSION_ROOM`, as `run-studio.sh` and `follow-song.sh` do | Missing 7 |
| Reorder, add, remove a song | `STUDIO_SETLISTS_MOVE`, `_ADD`, `_REMOVE` (`features/setlists/extension.toml`) | Exists |
| New setlist | `STUDIO_SETLISTS_CREATE` | Exists |
| Rename, reorder or bench a set; a note on a song | — | Missing 2 |
| Share with the band | `STUDIO_SETLISTS_SHARE` | Exists |
| Open chart, lyrics | `EXT_VIEW_OPEN` on Keyflow's chart and `studio.lyrics` | Exists |
| Versions, checkpoint | `DAW_RUN` `SESSION_VERSIONS`, `SESSION_CHECKPOINT` | Exists for the open song |
| New song | `DAW_RUN {id: "SESSION_NEW"}`, then a song note | Missing 5 |
| List rigs | `STUDIO_LAUNCHER_RIGS` (profile files) | Built, listing only |
| Load a rig | `RIG_LOAD` | Missing 8 |

### Missing, and where each belongs

1. **A song note that names its session** (`session`), written by whatever
   makes the session, relative to the project root. Belongs to
   `features/setlists` (the key exists in `song.rs`; nothing resolves it
   into `<org>/Projects/<name>`) and to the engine's save. Until then the
   launcher finds sessions by name, which is a guess.
2. **Set and note edits**: `STUDIO_SETLISTS_SET_RENAME`, `_SET_MOVE`,
   `_SET_BENCH {section, on}` and `_NOTE {setlist, song, note}`, in
   `features/setlists`. The note needs a place in the setlist's frontmatter
   (see decision 6).
3. **Recents as data**: `SESSION_RECENT` answers text lines for people
   (`name, ago — path (alternatives: …)`), which the launcher parses
   (`extensions/launcher/src/model.rs` `Recent::parse`). It should answer
   rows (`name`, `path`, `at`, `kind`, `alternatives`); in
   `apps/session/engine/src/actions.rs`. Rigs and setlists opened from the
   vault should enter the same list.
4. **`DAW_LAST`** in `extensions/daw`: what the engine opened last and where
   its playhead was, so Carry on can say "at bar 37" before anything opens.
5. **New song into a library**: one action that makes the song note, its
   folder in the project root and an empty session there
   (`SESSION_NEW` makes an untitled song and saves nowhere). Belongs to
   `features/setlists` calling the engine.
6. **Named layouts**: `STUDIO_LAUNCHER_ARRANGE {layout}` applying a saved
   workspace layout (`WORKSPACE_EXPORT` / `WORKSPACE_APPLY`) and the title
   slot (`HOME_SLOT_SET`). The layouts themselves ("Stage", "Rehearse",
   "Edit") are data: `run-studio.sh`'s embedded Python, which computes the
   splits from the display's shape, becomes three layout files. In
   `extensions/launcher`.
7. **The song changed, as an event**: the engine should publish
   `session.song.changed` so Keyflow and Ignition hook it themselves, and
   `follow-song.sh` (which polls `DAW_STATE` and re-runs the follow actions)
   is deleted. In `extensions/daw` (its `[capabilities] events`).
8. **Rigs in the app's engine**: see below.
9. **In Architect** (not changed here): no view can be named as what the app
   shows when nothing is open (`features/workspace/src/host/mod.rs`
   `EmptyCenter` shows "Nothing open"), and `EXT_VIEW_OPEN` cannot place a
   view in a window of its own (only `WORKSPACE_OPEN`'s `at` can). A
   distribution should be able to name its start view, and a view should be
   able to say it prefers a window (`[[windows]] kind = "window"`,
   `features/extensions/src/interop.rs` `WindowDecl`, covers the second once
   the launcher declares one).

## Rigs

How a rig is identified today: by instrument (`signal_rigs_proto::Rig`, a
slug such as `guitar` or `keys`) and, within it, by a saved profile, a styx
file under Signal's config folder (`guitar::RigHome`). There is no
catalogue: no service lists rigs, and nothing ties a rig to a setlist
outside `features/library`'s fixture.

How one is loaded today: `studio-desktop --guitar` or `--keys` opens the rig
view on its own engine (`apps/studio/desktop/src/front.rs` `preset_from`,
`rigs.rs`); inside a song, `RIG_LOAD {track, instrument, profile, patch}`
puts a rig first in a track's chain (`apps/session/engine/src/rig_actions.rs`),
for guitar and bass only, and only when `session-engine` is built with its
`signal` feature. `extensions/daw/Cargo.toml` builds it without, so the
app's engine has no `RIG_LOAD`.

So the first version **lists** rigs (the profile files' names) and says that
loading is not built. What loading needs, in order:

1. `extensions/daw` builds `session-engine` with `signal`, so `RIG_LOAD`
   exists there. Cost: the guitar crate and its captures in the engine's
   build (`SIGNAL_RIG_LIBRARY`, `rig-library/README.md`).
2. Signal as an extension providing a service (`signal.rigs`: `list`,
   `load`, `parts`), so the launcher asks it instead of reading Signal's
   folder. Keys, drums and synth have no load path at all until then.
3. Parts stored where they sync: on the setlist note (decision 7).

## How it shows

| Where | Form | How |
|---|---|---|
| Desktop, at launch | The chooser, a window of its own | `STUDIO_LAUNCHER_WINDOW`; shown at start when the engine has nothing open (missing 9 for doing it without a script) |
| Desktop, any time | ⇧⌘O, or "Launcher: open" in ⌘K | `STUDIO_LAUNCHER_SHOW` (its `shortcut` in `extension.toml`) |
| Desktop, docked | A pane like any other; narrow in a side dock it is the phone layout | The same view: two columns that wrap into one, no media query (`extensions/launcher/src/view.rs` `LauncherView`) |
| Web app, 390 px | A view inside the app, full width | The same view |
| iOS | The same view; the app's first tab | The same view; opening asks the engine on the machine that has one |

As a window it closes when something is opened. As a pane it stays, and its
row for the engine's song is marked, so it doubles as an overview of the set.

### Relation to what exists

- **Architect's navigator** stays the tree of the vault: every note,
  including setlists and libraries (the setlists extension's
  `[[navigator_providers]]`). It answers "where is this note". The launcher
  answers "what do I open, and is it ready". The launcher's rows open notes
  in the navigator's way when asked (a song's note, a project's note); it
  does not replace the tree and does not grow one.
- **The songs bar** (`extensions/navigator` `SongsPane`,
  `session_ui::shell::SongTabs`) and **the set navigator** (`NavigatorPane`,
  `session_ui::navigator::Navigator`) show the set **the engine has open**:
  a tab or a row per song, the playhead, a press to go there. They are for
  during the show. The launcher shows sets **in the vault**, whether or not
  the engine has them, and is for before it. They meet at one point: a
  press on a song in the launcher, when the engine already has that set,
  goes to the song (`SESSION_SONG`) exactly as a tab does.
- **`studio-desktop`'s welcome screen** (`apps/studio/desktop/src/library_home.rs`
  `LibraryHome`, `apps/session/desktop/src/native/start.rs` `Start`) has the
  right shelves (ways in, recent, setlists, an org switcher) over the wrong
  store: Task's library through `session_engine::task_account`, and files on
  the device. The launcher keeps its ideas (Carry on is `start.rs`'s `Hero`;
  the recent row is its `RecentRow` over `session_engine::projects::Recent`)
  and reads the vault instead. Its components cannot be mounted as they are:
  they are private to `session-desktop` and call the engine in-process.
- **The library editor** (`apps/session/desktop/src/native/library.rs`
  `LibraryEditor`: lists, the one being edited, songs to add) is Plan mode's
  ancestor. Its three panes are Plan mode's three columns.
- **The setup view** (`apps/session/ui/src/setup.rs` `SetupView`, its
  `SongTable`) is the open set's table plus routing and settings. Routing
  and settings stay there. Its song table is superseded by the launcher's
  running order.
- **The setlist view** of the setlists extension (`features/setlists/src/ext.rs`
  `SetlistView`) is the setlist as a page to read and play from, and what a
  share link shows. The launcher links to it; it does not embed it.

## A share link's guest ([mockup](project-launcher/phone-guest.html))

A guest is the same app with fewer capabilities, never another page. For
the launcher that means:

- A guest sees **the shared setlist and its songs**, with their marks, and
  can open the charts. That is the launcher's setlist page, reading only the
  share's documents.
- A guest does **not** see other setlists, recent projects, rigs or the
  engine: those need the vault's index, the engine's actions and this
  machine's disk, none of which a guest is granted
  (`architect_extensions::share_needs::guest_capabilities` keeps
  `reads = ["documents"]` and drops `actions`). The page says so in one
  sentence instead of showing empty sections.
- No Plan mode, and no Tracks or Lights marks that could be filled (they
  are read from a member's disk), so those two columns are hidden for a
  guest in the built version rather than shown empty as in the mockup.

Not built: no host runs a share link's extensions yet, and a vault query
answers "unsupported" for a guest. Until then a guest gets the setlists
extension's own page.

## Decisions for the owner

Each has the options and a recommendation. None blocks the first version.

1. **How a song says where its session is.**
   (a) The song note names it: `session: songs/drive-to-la/Drive To LA.session`,
   relative to the project root. (b) By convention: the launcher finds a
   folder named for the song. (c) The project note lists them.
   *Recommend (a), with (b) as the fallback it is today.* A name is a guess
   ("DTLA Cinematic Intro" is in `songs/cinematic-intro`), and a song can be
   in two setlists of two projects. The engine's save writes the key.

2. **One vault at a time, or every org at once.** The chooser mockup shows
   Merry and Bright (tombrooksmusic) beside the recital (fasttrackaudio).
   An Architect window is on one vault. (a) The launcher shows the open
   vault and an org switch that reopens the app on another. (b) It lists
   setlists across every org the account can reach.
   *Recommend (a) first, then (b) for "Coming up" and "Recent" only*, since
   those two are the lists a person scans without knowing which org.

3. **What launch shows.** (a) Always the chooser. (b) The chooser only when
   the engine has nothing; otherwise the last layout as it was. (c) Never,
   the app opens where it was.
   *Recommend (b).* At a venue the app must come back as it was after a
   crash; at a desk with nothing open the chooser is the right first thing.

4. **Does the window close when something opens.** *Recommend yes as a
   window, no as a pane*, with Option held to keep the window (Logic's
   behaviour).

5. **Which marks count for a set.** A charts-only gig (Merry and Bright) has
   no tracks or lights by design, so two empty columns say "not ready"
   forever. (a) Five fixed columns. (b) The setlist says what it needs:
   `needs: [chart, reference]`, and only those columns show and count.
   *Recommend (b), defaulting to all five when the setlist has a project
   with sessions and to chart alone otherwise.*

6. **Where a song's note in a set lives.** (a) A `notes` map in the
   setlist's frontmatter, keyed by the song's link. (b) In the body's
   `studio-setlist` fence, a ninth field. (c) On the song, which is wrong:
   the note is about this set.
   *Recommend (a)*; the fence is generated from the frontmatter and should
   stay so.

7. **Where parts live.** (a) On the setlist note: `parts: [{player, rig,
   profile, patches: {song: patch}}]`. (b) In Signal's own files per
   machine, as today (`guitar::profiles::SetlistDef`).
   *Recommend (a)*: a part is the band's plan and must reach the player's
   device; Signal reads it through the vault.

8. **What "Open the set" arranges.** (a) Nothing: it opens the set and
   leaves the panes. (b) It applies the setlist's layout ("Stage" for a
   show, "Rehearse" otherwise), chosen in the head beside the button.
   *Recommend (b), with the layout remembered per setlist*, which is the
   part of `run-studio.sh` people will miss most.

## Left out of the first version

- Plan mode: no reordering, sets, notes or "Before the show". The actions
  to reorder exist; the view does not call them yet.
- The inspector, the chart thumbnail, versions and checkpoints.
- The set to scale.
- Carry on, Coming up, search, New song, New setlist.
- Loading a rig; parts.
- Arranging the views and pointing the chart and lighting at the song.
- Libraries and Projects as sections; other orgs.
- The chooser as a distinct small window: the window is the same view.
- Opening on another machine's engine from a phone, and the guest view.
- Readiness by `needs`; the five marks are fixed.

## The first version, as built

`extensions/launcher` (`studio-launcher`, id `studio.launcher`), a process
extension that is also linkable (`studio_launcher::Launcher`), with no
dependency on Session's crates, so it builds in about a minute cold and
seconds warm.

- `src/model.rs`: setlists, sets, songs and marks from the vault's rows.
  Pure, tested.
- `src/find.rs`: the project root beside the vault, a song's session by its
  note or its name, cue files, rig profiles, the written setlist file. The
  only code that reads the disk. Tested on scratch folders.
- `src/host.rs`: the vault query, `VAULT_STATUS`, `DAW_RUN`.
- `src/lib.rs`: the actions (above), `Found` (a setlist with what was found
  for each place).
- `src/view.rs`: the view. Left: setlists, recent, rigs. Right: the chosen
  setlist's running order with the five marks; a press on a row with a
  session opens it. The foot says what the last press did and what the
  engine has.

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

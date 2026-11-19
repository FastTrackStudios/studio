# A setlist's share link, on the web and on a phone

The goal, in the owner's words: "Get share links for JT's Recital and
Merry and Bright. Where like a streaming service someone can listen to
all of the songs in that set, but they can click, then they can also see
the chart, synced lyrics, arrangement view etc. for the whole thing. On
the web." People will open these links on a phone.

This note is the audit of what exists (2026-11-18), every gap between
that and the goal, the order to close them in, and the decisions that are
the owner's. Mockups of the phone experience are in
[`share-link-setlist/`](share-link-setlist/index.html).

Fixed points, not reopened here: a share link opens **the app itself** as
a guest (Architect's `guide/shares.md`); Studio is a distribution of
Architect with everything compiled in; nothing is deployed or published
by this work.

## Where it stands (2026-11-18, after the owner's answers)

The owner took the recommendations below with four changes, and asked
for a different phone layout after seeing the first one. This section is
the current state; the audit and the gap tables under it are as first
written, and say where they were overtaken.

**Decided.**

- A guest reads the text the link's build writes, plus the frontmatter
  keys the view declares (architect#648, option 1).
- Downloads: charts and lyrics yes, recordings and videos no. Download
  all must not include the lighting videos.
- **No AAC rendition.** "We want the app to handle playback." The web
  app decodes the recordings as they are (Ogg Vorbis) on every browser.
  Gap S6 is dropped.
- **Hosting is one server and one domain.** Architect itself, headless,
  for every org; Studio's web app at `fasttrackstudio.app`, a share link
  a path on it. No node per org, no org subdomains.
- **Media are attachments in the vault** and nothing else: no second
  file store for what a link plays.
- The recital's link is for invited people or expires; the Archive set
  is left out for guests; Merry and Bright is not sent as a listening
  link while it has no audio; no real link before iPhone checks.
- **The phone screen** (asked for on seeing the first build): the top is
  one thin line with the setlist's name centred and quiet; no Notes /
  Files toggle, no "Shared with you" line, no large title (those are in
  the sidebar); no editing toolbar; the bottom tabs are **Setlist,
  Charts, Lyrics, Arrangement**, and the round button plays and pauses.

**Built since, and seen in a browser at 390 px** (screenshots `30` to
`45` and `smoke2/` in the private folder; `just dev-smoke` green):

| What | Where | Seen |
| --- | --- | --- |
| A song's lighting video is a Full-layer file: a listener's link refuses it | `features/setlists/extension.toml` | Files 88 to 72, none a video; `403`, `x-architect-layer: full` by URL |
| `<origin>/<code>` opens the link (a redirect to `/s/<code>/`) | `scripts/local-stack-front.py` | `302`, then the set |
| A row of the set opens its song | the island; Architect `NOTE_OPEN` | the second row's control opens Drive To LA's chart from its top |
| A guest is shown no control that needs a vault or a write | Architect `apps/web/src/permits.rs` | no Files, Open…, New note, Vault, Search; signed in they are back |
| The thin title, the link in the sidebar, no editing toolbar | Architect `apps/web/src/app/guest.rs` | `41-set-390`, `45-sidebar-390` |
| The four tabs and the round Play, from the island's declared favourites | the island's manifest; Architect `Distribution::favourites_of` | `42-charts-390` (the playing song's chart, following), `43-lyrics-390`, `44-arrangement-390` |
| The Arrangement view, first version: the song playing on a timeline, its sung lines, the playhead; sections where the playhead has them | the island, `arrangement.rs` | `44-arrangement-390`: "The whole song", 48 lines, the playhead at 0:25 |
| An island's files gzipped by the node | Architect `shares.rs` | the web bundle's wasm: 8.0 MB to 3.72 MB on the wire |
| A song's view is handed its own files, not every other song's cover | Architect `shares.rs` | by test of the rule only |
| The lock screen's cover and scrubber | Architect `audio.rs` | Media Session `artwork: 1` |

**Built and NOT yet run in a browser: the in-app decode.** Architect's
player now decodes Ogg Vorbis in the page (symphonia's Ogg reader and
Vorbis decoder) and plays it through an AudioWorklet whose own count of
played frames is the clock the chart and lyrics follow. The same shape
as Session's browser audio (`apps/session/runtime/src/web_audio.rs`),
written again in Architect because Architect is MIT / Apache and Studio
is GPL. The decoder is proven natively: a whole decode, and seeks to six
frames compared sample by sample (they found a real fault first: after a
seek the first packet comes out short, so the reader now goes back a
block and discards up to the frame asked for). **The worklet itself has
not played**: no browser on the build host starts an AudioContext in the
test sandbox (headless Firefox, a null ALSA device, a private PulseAudio
with a null sink and a virtual display were all tried). There the player
waits two seconds for the context and hands the recording to the
browser's own player, which is what the screenshots and the smoke test
show ("played by the browser's player"). It needs one run in a browser
with an audio device: a desktop, or a phone over `just phone`. On an
iPhone, playing on with the phone locked needs checking too: iOS may
stop a page's Web Audio when it locks, and the known answer is to route
the worklet into an `<audio>` element through a `MediaStream`.

**Hosting, as it now stands.** One origin serves the bundle and the node
(the local front does exactly this). A link is `/s/<code>/…`; the short
form `<origin>/<code>` is a redirect at the proxy, not a second mount,
because the node's paths and the app's own URLs hang off `/s/`. A node
serves links of every vault it holds, whatever org each is of; hosting
several orgs' enrolment on one node is architect#538. [infra#26] was
written for one node per org and needs that line changed.

**Left, in order.**

1. The in-app decode heard in a real browser, and on an iPhone locked
   through a song change (above).
2. Recordings streamed but not offered for download: the link still
   lists the 18 recordings under Files and in Download all. It is a
   permission Architect does not have yet ("plays, is not offered").
3. Covers at the size they are drawn (7.8 MB of originals for 48 px
   tiles), the app's own 6.9 MB wasm, a bundle a view (architect#695),
   the fonts (keyflow#76).
4. The arrangement's sections for every song, its markers and waveform:
   A1 below. Today a song opened from the set shows one section, "The
   whole song", because the set's queue carries no sections.
5. Lyrics exported from the session (C5), declared properties for a
   guest (architect#911).
6. On a desk, the sidebar holds the link as on a phone; the layout there
   has not been looked at since the change.
7. Studio's own web build (W1).

## The audit

**What it ran on.** Studio at `stack/studio-distribution` (`5f78e7cd`)
with the stack's local overrides, Architect at the same branch plus one
build fix (below), on Studio's local stack: the node, the issuer and the
web app on loopback. The node held a **copy** of the real recital vault
(`fasttrackaudio/Vault`, 1.0 GB, copied once into the stack's state;
the vault itself is read and never written), with a share link of
`Setlists/JT's Senior Recital.md` for anyone with the link:

```sh
LOCAL_STACK_VAULT=/run/media/Development/vaults/fasttrackaudio/Vault \
LOCAL_STACK_SETLIST="Setlists/JT's Senior Recital.md" \
LOCAL_STACK_SHARE_CODE=jtrecitaxstackaa \
LOCAL_STACK_OVERRIDES=<the stack's overrides.toml> \
LOCAL_STACK_ARCHITECT_PATH=<an Architect checkout at the same branch> \
just dev watch=off
```

The browser was a headless Firefox 159 over WebDriver BiDi
(`scripts/share-audit-browser.mjs`), at 390 x 844 and 1280 x 800, in a
mount namespace with no audio socket, as the smoke test's is. So "plays"
below means the `<audio>` element loaded the recording and its clock ran;
nothing was heard, and the bar says "Not heard here" in the shots for
that reason. No Safari and no real phone were available: every Safari and
iOS line below is from the code, and is marked so.

**The screenshots are not in this repository.** They show the vault's
covers, charts and lyric lines, and the vault's README forbids those in a
repo, a PR or an upload (this repository is public). They are on
THEBATTLESHIP in
`/run/media/Development/t3-worktrees/studio/share-link-setlist-audit/`,
named as below.

### The stack did not start

Before any step of the goal, five things stopped the local stack on this
base. All are fixed on this branch:

| What I saw | Cause | Fix |
| --- | --- | --- |
| `Cargo.lock has no architect-desktop from https://github.com/…` | Since studio#539 the lock names Architect on the `www.` host | `scripts/local-stack.py` matches either spelling |
| `the setlist names songs the vault does not have: ['…/Not Too Much Longer#Live.md', …]` | The share seeder took `Song#Arrangement` links as file names | it shares the song's note |
| `error[E0433]: cannot find shot in the crate root` building the node | Architect's `stack/studio-distribution` calls a native-only module from the headless build | one `#[cfg]` on Architect branch `feat/share-link-setlist-web` (`fb128b72`) |
| `…/studio-architect is not Studio Architect`, then `invalid value 'node' for '<PROCESS>'` | The distribution is named Studio now; Architect's dev stack runs its node as `app --headless` (architect#825) | `scripts/studio-architect.sh`, `scripts/local-stack.sh` |
| The link's page stayed on "Opening the shared link…" | The page is the node's and loads the web app from **its own origin**; the stack served the bundle on one port and the node on another, joined by nothing | `scripts/local-stack-front.py`: one origin, the node's paths to the node, as the hosted proxy does |

`just dev-smoke` checked the share page that architect#646 retired
(server-rendered rows, `[data-track]`) and failed on this base: gap S3,
rewritten on this branch.

### The goal, step by step

Two columns of evidence: **as found** (the base, once the stack ran) and
**with the claim fix** (commit `276dc7fb` on this branch, gap S1).

| Step | As found | With the claim fix |
| --- | --- | --- |
| **Open the link** | Works. The web app opens as a guest: "Shared with you, Anyone with the link" (`01-open-390`). | The same. |
| **See the set** | Works, as the app's **own** list: 18 rows with covers, the acts as headings; keys and lengths are blank pills. Studio's `setlist.playlist` view is served and never mounted: `/_ext/islands.json` had `"embeds":[]`. | Studio's playlist draws: number, cover with the key on it (`C→Db`), title over artist, length, the acts (`10-island-set-390`, `24-set-1280`). |
| **Play a song** | Works. Play starts the first song; the audio element holds `…/renders/cinematic-intro.ogg`, `readyState 4`, time advancing (`02-playing-390`). The recording is served with ranges: `206`, `content-range: bytes 1000-1999/3492679`. | A row pressed plays it; the row is marked and a line under it follows the playhead (`11-island-playing-390`). |
| **Play through to the next** | Works. Seeking to the end of song 1 with the app's bar, song 2 starts by itself: row, bar and Media Session all say "Drive To LA" (`05-next-song-390`). | Works: song 2 ends, "For You" starts (`12-island-next-390`). |
| **Title, art and progress showing** | Works in the Now Playing bar: 40 px cover, title, artist and set, a progress line; opened, a seek bar with times and previous / next (`04-nowplaying-open-390`). **Not on the lock screen**: Media Session has title, artist and album, `artwork: []`, no position state. | The same. |
| **Open a song's chart** | **Broken.** The song's page is the note as plain blocks: the chart is absent, the recording is a line of text ("drive-to-la.ogg: in the Now Playing bar"), its vault path is printed (`06-song-390`). | Works. The chart is engraved in the browser, 4 pages for Drive To LA (`14-song-chart-390`). There is **no way to a song from the set**: the playlist's rows play, so the path is the Notes tab's list under it. |
| **See the chart follow** | Does not exist as found. | Works. A bar pressed plays from there; 20 s into Fire (120 bpm) bar 10 is washed (`19-fire-follow-390`). Playing from the set and then opening the song, the chart follows the set's own queue (`22-set-then-song-390`). |
| **Open its lyrics** | As a numbered list of 48 static lines in the note. | The Lyrics tab lists the timed lines (`20-lyrics-follow-390`). |
| **See them follow** | Does not exist as found. | Works: the line being sung is marked and kept in view (`21-lyrics-follow-b-390`). |
| **Open its arrangement** | **Does not exist.** No view draws an arrangement in the web bundle. | Does not exist. |

Also seen, at both widths:

- **A guest is shown a member's controls.** Buttons on screen: Files,
  Open…, New note, Commands, Settings, Vault, Search. Two rows of bottom
  chrome sit under the Now Playing bar: 190 px of an 844 px screen.
- **Files (88)** offers every file of the link and "Download all (88)":
  each song's recording, chart PDF, `.kf` source, `.lrc`, cover, and its
  lights video (12 to 73 MB each, about 700 MB in all) (`25-files-390`).
- **The original artists' recordings are not served** (`reference/… (original).ogg`: `404`): the link's layer holds them back. The show
  renders are served.
- A song opened from the Notes list keeps the list's scroll offset and
  opens 3,280 px down, looking like nothing happened (`16-fire-390`).
- Songs with no chart draw their lyrics alone; a song with one
  arrangement and no `.lrc` draws its chart alone
  (`26-song-no-chart-390`, `27-song-arrangement-link-390`).
- The first load fetched each of the 18 covers twice, at full size.

### First load, measured

| What a guest's phone fetches | Raw | gzip -9 | As served |
| --- | ---: | ---: | --- |
| the web app, `architect_web_bg.wasm` | 23.06 MB | 6.91 MB | compressed only if the proxy does |
| Studio's island, `web-studio-island_bg.wasm` | 8.00 MB | 3.68 MB | **uncompressed** by the node |
| the 18 covers | 7.8 MB | (JPEG) | fetched twice: 15.6 MB |
| the first song | 1.4 MB | (Ogg) | ranged |

About 30 MB before the first note, on a set whose 18 recordings are
63 MB in all. At 5 Mbit/s that is close to a minute.

## The gaps

Size: **S** a day or less, **M** a few days, **L** a week or more.

### Slice 1: a guest on a phone opens the recital's link and listens through the set, with title, art and progress showing

| # | Gap | Where | Size | Depends on |
| --- | --- | --- | --- | --- |
| S1 | The node listed none of the island's claims, so its views never mounted for anyone | this repo | S | **done** (`276dc7fb`, with a test) |
| S2 | The local stack did not run on this base and could not open a link in a browser | this repo, and one line in Architect | S | **done** (`08c97b63`; Architect `fb128b72`) |
| S3 | `just dev-smoke` checked the retired share page | this repo | S | **done**: it opens the link as a guest, sees the playlist list the set's songs, plays one and sees its chart follow; green on the recital copy |
| S4 | The link's first load is about 30 MB | Architect ([architect#908], with architect#602 and architect#695), Keyflow (keyflow#76), infra ([infra#26]: compress at the proxy), content (covers at 600 px) | M | nothing; the proxy and the covers are the two cheap ones |
| S5 | The lock screen has no cover and no scrubber; whether the next song starts on a locked iPhone is untested | Architect ([architect#909]) | S to M | a real iPhone |
| S6 | Recordings are Ogg Vorbis, which an iPhone plays only from iOS 18.4 (from memory of Safari's release notes; **check on a device**). Older phones would be told "Can't be played" and skip every song | content (an AAC rendition beside each Ogg), this repo (`setlists` prefers the rendition the listener can play, or lists both) | S | the owner's choice of format (below) |
| S7 | A guest's screen carries a member's controls and two extra bars | Architect ([architect#910]; part of architect#804 and architect#805) | M | nothing |
| S8 | The hosted node and bundle for the two orgs | infra ([infra#26]) | M | the owner's go; a Studio release with S1; one node per org or architect#538 |

S1 to S3 are what this branch built. With S1 alone, on a desktop
browser, the smallest slice already holds end to end. What stands between
that and a phone someone was sent a link to is S4 to S8, and none of it
is in this repository except half of S6.

### Slice 2: the chart, the lyrics and the set as one place

| # | Gap | Where | Size | Depends on |
| --- | --- | --- | --- | --- |
| C1 | No way from the set to a song: a frame may ask only the playhead's seven asks | Architect ([architect#910]: an ask to open a note of the same link; the Now Playing card opening its item's note) | S | nothing |
| C2 | A setlist's favourites (Setlist, Lyrics, the round Play) are not listed: the node reads compiled-in extensions' manifests, and these are in the island's | Architect ([architect#910]) or this repo (move them when Studio is one extension) | S | nothing |
| C3 | A guest's view gets no declared properties, so the island derives key, tempo and cover from the body and the files | Architect ([architect#911]) | S | the owner's choice on architect#648 |
| C4 | Two songs of the recital's 16 have no `.lrc` (DTLA Cinematic Intro, Not Too Much Longer); 6 have no chart (the covers of Act II and No One Knows) | content | S each | nothing |
| C5 | Lyrics live in each song's session; nothing writes them out (below) | this repo | M | nothing |
| C6 | The song view's tabs are clipped at the top when the frame scrolls; the title is drawn three times | this repo (the island), Architect ([architect#910]) | S | nothing |

### Slice 3: the arrangement

| # | Gap | Where | Size | Depends on |
| --- | --- | --- | --- | --- |
| A1 | Nothing writes a song's arrangement in a form a guest can be handed | this repo (`setlists`, from the session) | M | nothing |
| A2 | No view draws it | this repo (the island, a view `arrangement`) | M | A1; C1 for the tab |
| A3 | The real arrangement in a browser, for a member | this repo and Architect (`share-arrangement.md`) | L | architect#805, architect#847 |

### Slice 4: the phone layout as mocked, and Studio's own web build

| # | Gap | Where | Size | Depends on |
| --- | --- | --- | --- | --- |
| P1 | The player as a screen of its own, with the song's views one tap away | Architect (the Now Playing bar's open state as a screen; a slot for the source's views) and this repo (the views) | L | S7, C1, C2 |
| P2 | The section strip in the bar and the player | this repo (`transport-ui`, already the island's `progress` view) and Architect (the bar draws an item's `sections`) | M | nothing |
| W1 | Studio's own web build ("compiled together") | this repo, then infra | L | below |
| M1 | Merry and Bright has no audio, no covers and no lyrics | content | — | the owner (below) |

## The order

1. **S1 to S3**: done on this branch. Slice 1 has a test (`just
   dev-smoke`), not only screenshots.
2. **S6**: an AAC rendition, and the playlist choosing it. Cheap, and the
   one thing that could make a link silent on an older iPhone.
3. **S4**, in this order: compression at the proxy (infra, a config
   line), covers at 600 px (content, or `setlists` writing a small cover
   when it writes the note), then architect#602, architect#908's
   one-fetch and own-files-first, architect#695 and keyflow#76.
4. **S5** on a real iPhone over `just phone`, with architect#909.
5. **S7**, then **S8** when the owner says go: the first real link.
6. **C1 to C3**, **C6**: the chart and the lyrics become reachable from
   the set. **C5** beside them.
7. **A1, A2**: the arrangement, pre-rendered.
8. **P1, P2**: the layout in the mockups.
9. **W1**, and **A3** behind it.

What blocks what: S8 waits for the owner and for a release that carries
S1. P1 waits for S7, C1 and C2. A2 waits for A1. A3 and W1's value for a
guest both wait for architect#805. Nothing in slice 1 waits for slice 2.

## The questions the brief asked

### What a guest must be allowed to read

For the slice to work a guest needs, of the setlist note and each song
note the link lists:

| What | Why | Today |
| --- | --- | --- |
| the note's text, as the link's build writes it (comments out, links outside the link as words, no frontmatter) | the set's running order is its `studio-setlist` fence; a song's chart is its `kf` fence | given, when an island claims the type and declares `documents` |
| the link's images and `.lrc` files, as bytes | covers and timed lyrics | given (2 MiB a file, 8 MiB and 32 files a note), when the island declares `files` |
| the recordings, streamed | the listening | given, by the link's layer (`listen: Reference`) |
| the declared frontmatter keys (`date`; a song's `key`, `tempo`, `length`, `artist`, `art`) | so the view need not parse the body for them | **not given**: always `{}` ([architect#911]) |

So architect#648's question has been answered in code as its option 1
without the frontmatter clause, and is still open as a decision.

### Audio

- **Which file.** The song's first playable file in its `media`
  property: `renders/<slug>.ogg`, the show's stereo render. It is the
  fifth column of the setlist's fence and the `media=` of the song's
  chart fence, and it is what played in the audit. It is not the
  engine's Reference Mode bounce (`Media/Reference.wav` and its proxies,
  inside the session folder, in the heavy layer) and not the desktop's
  `Reference` folder of tracks: those need an engine. A listener's phone
  gets a file.
- **Where it lives.** In the org's vault, the light layer, beside the
  song's note. The VPS holds it; no file root is involved.
- **Size.** 1.4 to 4.9 MB a song (Vorbis, about 140 kbit/s), 63 MB for
  the 18. An AAC rendition at 160 kbit/s is the same order.
- **How it is served.** By the node at
  `/s/<code>/<vault path>`, checked against the link for every request,
  with ranges (`206`, `Accept-Ranges`, `Content-Range`; single range
  only). `cache-control: no-cache` and no validator, so a replay fetches
  again ([architect#908]).
- **What a guest is not served.** The original artists' recordings
  (`reference/`), held back by the layer. The lights videos **are**
  served and listed for download.

### Synced lyrics when they live in the session

In the session a lyric line is an item on the track named `Lyrics`
(with tags, `Lyrics +LYRICS +CC [large]`: the stack branch reads the
tags off; `main` matches the bare name only). On the web a guest's view
is handed `.lrc` text, and nothing writes one from a session: the two
`.lrc` writers are `studio lyrics fetch` (LRCLIB) and the chart's own
lyric lane.

The recital's 14 `.lrc` files came from the same Ableton clips the
sessions' tracks did, a day before the sessions were split, so they
agree today and will drift on the first edit in a session.

Proposed (C5): the session is the source, the `.lrc` beside the song's
note is its **export**. `setlists` writes it from the session's Lyrics
track when it writes the song's note and when a link is created (an
action, `STUDIO_SETLISTS_LYRICS`, and part of `STUDIO_SETLISTS_SHARE`),
and says in the file's header where it came from. Nothing in Architect
changes: a guest already gets `.lrc`.

### The arrangement for a listener

**Recommendation: a pre-rendered arrangement, not the engine.** The
real arrangement in a browser exists (`session-daw-web`: the engine in
wasm, an AudioWorklet, WebGL2, about 7 MB of wasm more) and needs the
song's session and its media, which are the heavy layer and never reach
the VPS. A listener needs to see the form, not edit it.

- **A1, what is written.** Beside the song's note, by `setlists`, from
  the session: the sections (name, colour, start and end in seconds and
  bars), the markers (count-ins, cues), the tempo map and bar 1, and the
  reference's peaks at about 400 points a channel. A few kilobytes. As a
  fence in the song's note (`studio-arrangement`) it reaches a guest
  with no change in Architect; as a file it needs Architect to hand a
  view a kind beyond images and `.lrc`.
- **A2, what draws it.** A view `arrangement` in the island: the ruler,
  the section lane, the marker lane and the waveform on one timeline,
  the playhead from the app's playhead, a section pressed seeks, nothing
  editable. `transport-ui` already draws sections; the waveform is an
  SVG path.
- **What it gives up.** Track lanes, items and the mixer. Those are the
  stems, which the link does not share; the mockup says so in the view.
- **A3, later.** For a member with the session, the real
  `WebArrangement`, locked, on the page's clock:
  `docs/design/share-arrangement.md` has its six pieces. A guest could
  follow a live session read-only since architect#847, but a guest runs
  no linked extension until architect#805.

For the 10 recital songs with a chart, sections already exist on the web
(Keyflow's section marks, on the chart's clock). A1 is what gives the
other 6 a form, and all 16 their markers and waveform.

### The phone layout

The mockups are the proposal; open
[`share-link-setlist/index.html`](share-link-setlist/index.html).

- **The screen is the set**, then the player, then a song's views.
  The link opens on the set: its name, "16 songs in three acts, 1 hr
  2 min", Play, the acts and their rows.
- **One bar.** The Now Playing bar is the only thing docked above the
  home indicator, padded by `env(safe-area-inset-bottom)` (the app
  already measures the insets and sets `viewport-fit=cover`). A guest's
  screen has no rail and no phone foot: there is nothing in them a guest
  can use.
- **The player** is a tap on the bar: the cover, the title, the song's
  sections as a strip that is also the scrubber, previous / play / next,
  then Chart, Lyrics and Arrangement as three targets, then what is
  next.
- **A song's views are tabs of the song**, under the same strip, with
  the bar still docked: the chart with the bar being played washed, the
  lyrics with the line being sung large, the arrangement scrolled
  sideways.
- **The section strip is the thread**: the same sections, in Keyflow's
  colours, on the bar, the player and every view. It is Session's
  progress bar; the island already draws it.
- **Lock-screen controls** are the Media Session's: [architect#909].
- **At desk width** the set is a column on the left, the chart has the
  lyrics or the arrangement beside it, and the player runs along the
  bottom.
- Targets are 44 px or more; text follows the system's light or dark
  theme, as the app does.

### Bundle size and first load on a phone connection

Measured above: about 30 MB. The budget to aim for is under 5 MB before
the first note on a cold cache:

| Step | Saves | Where |
| --- | ---: | --- |
| compress `/_ext/` and `/assets/` (brotli at the proxy, then in the node) | about 4.3 MB of the island, and the app's 16 MB if the proxy was not already compressing it | infra, architect#602 |
| covers at 600 px, fetched once, a note's own files first | about 15 MB | content, [architect#908] |
| the setlist view without the engraver (a bundle a view) | about 3 MB gzipped | architect#695 |
| the engraver's fonts subset | about 1.5 MB gzipped, for the song view | keyflow#76 |
| the app's own wasm | 6.9 MB gzipped is the floor for every guest until it is split | [architect#908] |

The last line is the one with no cheap fix: after everything else a
guest still fetches about 7 MB to see a list of songs.

### Safari and iOS without JSPI

From the code, not from a device:

- The app-as-guest draws an island's views from its **web bundle**
  (wasm-bindgen, `--target web`) in a sandboxed frame, never from the
  WASI component, so JS Promise Integration is not needed. In the audit
  the page fetched `web-studio-island_bg.wasm` and no
  `studio_island.core.wasm`.
- `just island-web` builds that bundle with only the wasm features
  Safari 15 has, and fails the build otherwise.
- Not verified anywhere: the app's own 23 MB wasm starting on an iPhone
  (memory), the frame's sheet and `crypto.subtle` checks on Safari, Ogg
  (S6), and the song change on a locked phone (S5). `just phone` puts
  the local stack on the tailnet over HTTPS for exactly this.

### What "the studio-web image compiled together" has to contain

Three different things carry the name today:

| | What it is | State |
| --- | --- | --- |
| `studio-web` (`nix/modules/packages/studio-web.nix`, the `Images` workflow) | an image of the **old** web apps: `/signal/`, `/session/` (`session-daw-web`), a front door | built on every tag; not Architect, and not what a share link opens |
| `studio-architect-web-<tag>.tar.gz` (`scripts/studio-architect.sh web`) | **Architect's stock web app** with Studio's `distribution.json`: one wasm, no Studio code in it | released; what the VPS serves at `/app/` |
| `studio-architect-node-<tag>…tar.gz` | Studio's node with `studio-island/` beside it | released; where every Studio view on the web comes from today |

"Compiled together" on the web means the second line becomes Studio's
own wasm, as the desktop is Studio's own binary. Architect has the route
(`guide/your-own-architect.md`, "The web"):

1. `apps/architect` gains a `web` feature and
   `crate-type = ["cdylib", "rlib"]`, with
   `architect_web::run(studio_architect::distribution())` as the page's
   one start, and `architect-desktop` only off the browser.
2. **Every linked extension builds for `wasm32-unknown-unknown`.**
   `setlists`, `productions` and `studio-views` are the candidates. The
   desktop's `stage` bundle (arrangement, mixer, lyrics, navigator,
   Keyflow, Ignition) is `gui` only: each is a window on the native
   engine. Their components mostly are not: `session-ui` has a `web`
   feature that compiles the progress bar, the navigator, the song tabs,
   the lyrics panel, `WebArrangement` and `WebMixer`. The work is a web
   host for each extension, with the engine either in the page
   (`session-engine/web`) or absent (a listener).
3. `cargo xtask dist web --manifest-path apps/architect/Cargo.toml`
   builds it; `scripts/studio-architect.sh web` calls that instead of
   Architect's `web-bundle.sh`, and the release's web tarball is then
   Studio's.
4. The image the VPS runs is that tarball and the node, one release,
   one origin ([infra#26]). The old `studio-web` image keeps its name
   until it is retired or renamed; the two should not share one.

Two things about the desktop distribution's state decide the order:

- **It builds only with local overrides.** `stack/studio-distribution`
  in five repos, pinned to nothing. `scripts/studio-architect.sh release`
  refuses overrides, rightly. A web release waits for those branches to
  land and for tags.
- **A guest runs no linked extension** (architect#805). So for a share
  link, Studio's own wasm changes nothing a guest sees until that
  closes: the island is the vehicle for guests, and everything in slices
  1 to 3 goes through it. W1 is for members first.

## Decisions for the owner

Each with a recommendation; the rest of the plan does not wait on them
except where said.

1. **What a guest may read of a shared note (architect#648).**
   Options: (1) the text as the link's build writes it, plus the
   frontmatter keys the claiming view declares; (2) the whole text for
   types that opt in; (3) never, typed data only.
   **Recommend (1).** It is what runs today minus the declared keys, a
   guest gets nothing the page did not already show, and the charts and
   running order are the point of the link.
2. **What a listener may download.** Today: everything the link lists,
   and "Download all". Options: (a) as is; (b) listening only, no Files
   tab, for links of this kind; (c) charts and lyrics yes, recordings
   and videos no. **Recommend (c)** for the recital, and the lights
   videos out of the link either way: 700 MB nobody asked for, on a
   2-vCPU VPS. The covers of other artists' songs in Acts II and III are
   the show's own performances; whether those recordings go to "anyone
   with the link" or to invited people is the same decision.
3. **Who the link is for.** "Anyone with the link" or invited
   addresses. **Recommend** invited, or an expiry, for the recital;
   the link's audience can be changed later without a new link.
4. **The Archive set** (two songs not played) shows to a guest today.
   **Recommend** leaving it out of the shared setlist.
5. **A second audio rendition.** AAC (`.m4a`) beside each Ogg, so any
   iPhone plays it. **Recommend yes**; 16 files, a batch encode.
6. **Whether Merry and Bright ships without audio.** It has 12 songs in
   three sets and 16 charts, and no recording, cover or `.lrc`. As a
   link today it is a list whose rows cannot play, with charts that draw
   and follow nothing. **Recommend** not sending it as a listening link
   until at least a reference a song exists; if it has to go out for the
   band before then, send it as what it is, a chart book, and say so in
   its title.
7. **When the first real link is made.** Not before S5 and S6 are
   checked on an iPhone. **Recommend** the recital first, to invited
   people.

## What this branch built

- The node lists the island's claims and reads
  (`apps/architect/src/island.rs`), with a test that holds them to the
  island's manifest (`apps/architect/tests/node.rs`). This is S1: it
  turned five of the audit's steps from broken or missing to working.
- The local stack runs on the distribution branch, takes a set of local
  overrides, serves a copy of any vault's setlist
  (`LOCAL_STACK_SETLIST`, `LOCAL_STACK_SHARE_CODE`), and opens share
  links in a browser (`scripts/local-stack-front.py`).
- `just dev-smoke` on the app-as-guest flow (`scripts/local-stack-smoke.*`):
  the link answers with the web app beside it, the island claims setlist
  and song notes, a headless Firefox plays a song from the playlist and
  sees its chart follow.
- `scripts/share-audit-browser.mjs`: the browser the audit was walked
  with, kept for the next one.
- The mockups.

Issues filed for what is not this repository's: [architect#908],
[architect#909], [architect#910], [architect#911], [infra#26].

[architect#908]: https://github.com/FastTrackStudios/architect/issues/908
[architect#909]: https://github.com/FastTrackStudios/architect/issues/909
[architect#910]: https://github.com/FastTrackStudios/architect/issues/910
[architect#911]: https://github.com/FastTrackStudios/architect/issues/911
[infra#26]: https://github.com/FastTrackStudios/infra/issues/26

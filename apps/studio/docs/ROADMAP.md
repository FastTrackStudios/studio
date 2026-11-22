# Roadmap: apps/studio replaces Studio's other apps

apps/studio (studio-proto until 2026-11-22: a plain Dioxus 0.8 app on Drive)
becomes Studio. The session's engine and views (session-engine, session-ui,
timeline, editor, daw-ui) are this repo's crates, freed from Architect's
app layer as each phase needs them. Each phase
ends with the prototype working on Drive, on the web at desktop width and
390 px, before the next one starts.

## Phase 1: Drive, app-shell, organizations (2026-11-22)

- Current with Drive main: org-qualified documents, the node's orgs,
  Drive's org switch.
- The window on app-shell: the sidebar and top bar on a desktop; the tab
  pill, round button, accessory (Now Playing, the transport), drawer and
  sheets on a phone.
- Organizations on Drive's orgs (the auth server's). Each org has its own
  library, team (people, groups, roles, parts, who leads) and settings. A
  song belongs to one org and can be shared with others, which keep their
  own key, arrangement and leader for it. There is an All orgs view.
- The library's schema version 2 moves each built-in song to its home org.

The model is in `src/orgs.rs`. How it maps to Drive is in the README
("Organizations").

## Phase 2: setlists (2026-11-22)

- Each set is a Drive document in its org (`<org>/studio:setlist:<id>`):
  its running order is a Loro movable list of entries (songs, named
  sections, and items that are not songs: a reading, an announcement, a
  prayer, a video). Moves from two devices merge into one order.
- An entry points at a song by `owner/id`, so a set can hold songs shared
  from other orgs. Each entry carries the song's face as it was when added,
  and its own key, arrangement, leader and tempo, notes and a transition
  into the next. The lookup order is the entry, then the org's use, then
  the song (`setlist::resolve`).
- The band comes from the org's team, with each person's parts in this set.
  Each person sees their part: their parts, the key of every song, a capo
  hint for a guitar, and the songs they lead.
- The org's index (`<org>/studio:sets`) lists each set's face and the org's
  recurring events. A meeting's set has a fixed id (`<event>_<date>`). A
  new set can start from last time's set or from a template.
- An open set keeps its face, its calendar events (its date and
  rehearsals, in Drive's spine) and its sharing project current. Sharing
  goes through a Drive project, `studio-set-<id>`, the same way songs do.
- Schema step: index version 1 holds the built-in recurring events and the
  built-in sets' faces. Set version 1 writes the built-in set into its
  document. The prototype's old live keys (`studio:set:<id>`) carry over
  once.
- Not ported from Studio: the vault notes, share pages, engraving and media.
  Those are Architect and Keyflow code. The rig patches per part come back
  with Signal.

## Phase 3: a song's files (2026-11-22)

- Each song's files are a Drive project in its org (`studio-song-<id>`).
  Its parts point at files and folders where they are, on any computer, or
  at uploads in the org's Drive tree. Linking never copies, moves or
  deletes anything. Studio's notes (kinds, folders, another org's
  computers, aliases) are a document of the same project.
- Kinds: charts (PDF, keyflow), lyrics, click, guide, backing track,
  stems, multitrack, reference mix, MIDI, sessions, video. They are
  recognised by extension, name and folder, and can be changed by hand.
- Finding: every computer's indexes are searched by title, aliases and
  artist, or in Drive's search language. Matches are ranked and folded
  (mirrors, copies, folder contents) and can be previewed before linking.
- Availability: per file, where its copies are and whether a computer that
  has it is on. Per set, how many songs are ready whatever is switched off.
  The web build keeps a set's files on Drive (the node). A desktop build
  warms them onto the device through `drive-local` and holds them while the
  set is open.
- Sharing a song shares its files (the project's `shared_with`). The orgs
  it is shared with see its files read-only.
- Open in Drive: FastTrackStudio/drive#52 (parts on another org's
  computers, and access to them for the orgs a song is shared with), #53,
  #54.

## Phase 4: the session primitive

The song's session (`session-sync`'s `SessionDoc`) is the one document
every view edits. It is built up one view at a time:

1. **Arrangement view.** The timeline of the song's form, sections and
   markers, with tempo and meter, edited live by everyone in the song.
2. **Mixer.** The session's tracks, faders and sends.
3. **Chart.** The keyflow chart beside the form, following the playhead.
4. **Ignition.** Running the set live: the engine, the click and cues, the
   chart following whoever leads.

Each view is a feature of the one app (web and mobile features, not
separate apps). A device with the engine opens the same `SessionDoc` that
the browser edits.

## Not yet decided

- Recurring events in Drive's calendar (FastTrackStudios/drive#51). Today
  each meeting is a dated event once its set exists.
- An inspector pane in app-shell (FastTrackStudios/app-shell#32). The set
  page draws its own.

- Songs that belong to several orgs at once (multi-org projects or spaces),
  beyond one owner plus sharing.
- An org's member list from the issuer. People are Studio documents today,
  matched to an account by its email (FastTrackStudios/drive#50).
- Mirroring shares when nobody is in the owning org. Today the digest and
  the Drive project are kept up to date by whoever has the owner's library
  open.

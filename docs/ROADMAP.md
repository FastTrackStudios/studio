# Roadmap: studio-proto replaces Studio

studio-proto (a plain Dioxus 0.8 app on Drive) becomes Studio. Each phase
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

## Phase 2: setlists

- Setlists as Drive documents in their org, in place of the built-in data
  (`data.rs`: the series, the shows, the weeks' sets).
- An entry points at a song by `owner/id`, so a setlist can hold songs
  shared from other orgs. Its key and arrangement override sits on the
  entry. The lookup order is the entry, then the org's use, then the song.
- Who plays what in a set comes from the org's team: the parts each person
  plays, and who leads the set and each song.
- Rehearsals and set dates go in Drive's calendar (the spine).
- Moving the prototype's sets into documents is a schema step.

## Phase 3: a song's files in Drive

- Each song is a Drive project in its org, holding its session, charts,
  stems, the reference recording and the multitracks. The parts can live
  on any computer.
- The files reach apps through Drive's app files (`docs/app-files.md`): a
  real local path for an audio engine.
- Sharing a song also shares its files, through the same project
  `shared_with`.

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

- Songs that belong to several orgs at once (multi-org projects or spaces),
  beyond one owner plus sharing.
- An org's member list from the issuer. People are Studio documents today,
  matched to an account by its email (FastTrackStudios/drive#50).
- Mirroring shares when nobody is in the owning org. Today the digest and
  the Drive project are kept up to date by whoever has the owner's library
  open.

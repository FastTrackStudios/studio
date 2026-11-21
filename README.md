# studio-proto

Studio, remade from the launcher prototype as a plain Dioxus 0.8 app: phone first, then desktop and web. On the web it runs on Drive (`../drive`), served by a Drive node at `/studio`.

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

# music-convention

A convention for naming, grouping, colouring and ordering the tracks in a
music production session — and a reference implementation of it in Rust.

The problem it solves is mundane and universal: two engineers open the same
multitrack and one has `Kick In`, the other has `KICK_IN_01`; one puts
percussion under drums, the other beside it; nobody agrees what colour the
bass is. Templates fix this per-studio and break the moment a session
leaves the building. A published convention fixes it between studios.

## What is here

| crate | what it is |
|---|---|
| `music-catalog` | the taxonomy — instrument groups, song sections, abbreviations |
| `color-palette` | the colours those groups map to |
| `monarchy` | a generic metadata-hierarchy sorter (+ `monarchy-derive`) |
| `music-convention` | the convention itself: track schema, naming rules, equipment, icons, layouts, Pro Tools mapping |

`monarchy` is domain-neutral on purpose — it sorts anything with a metadata
hierarchy. The music-specific part is the taxonomy and the rules fed to it.

## What it covers

- **Track naming** — how a track is named, and how to recover structure from
  a name someone else wrote (`song_name`, `metadata_patterns`).
- **Grouping and order** — which instruments belong to which group, and the
  order groups appear in (`groups/`, `track_schema`).
- **Colour** — a stable colour per group, so a session is readable at a
  glance and two sessions look like each other (`colors`, `color-palette`).
- **Equipment and icons** — mics, DIs, amps, and the icon a track gets
  (`equipment`, `icons`).
- **Cross-DAW mapping** — how the convention reads and writes Pro Tools
  naming, so a session survives the round trip (`protools`).

## What it deliberately does not cover

Anything that needs a DAW. There is no REAPER, no Pro Tools API, no audio
I/O here — those belong in an implementation. FastTrackStudio's REAPER
implementation lives in
[FastTrackStudio](https://github.com/FastTrackStudios/FastTrackStudio) as
`dynamic-template`, and consumes these crates.

That boundary is the point: adopting the convention should not require
adopting a DAW, a language runtime, or an application.

## Using it

```toml
music-convention = { git = "https://github.com/FastTrackStudios/music-convention", tag = "v0.1.0" }
```

```rust
let config = music_convention::default_config();
```

You do not have to use the Rust crates. The tables are the convention; the
crates are one way to read them. Implementing the same grouping and colour
rules in a Logic template, a Python script, or a Pro Tools session template
is a legitimate adoption.

## Contributing

Corrections and additions are welcome — an instrument in the wrong group, a
missing articulation, a colour that reads badly on a dark theme, a Pro Tools
spelling we get wrong.

One known rough edge, stated plainly: **the convention is currently written
as Rust code** — groups as types, colours as constants, naming rules as
functions. That is convenient for the reference implementation and hostile
to a contributor who only wants to move an instrument between groups. Moving
the tables to data (styx/TOML) that the crates read is the intended
direction, and would turn most contributions into a few lines of data.

## Licence

MIT OR Apache-2.0.

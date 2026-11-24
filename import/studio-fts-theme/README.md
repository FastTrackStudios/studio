# studio-fts-theme

FastTrackStudio's own theme for Studio's views: the [arrangement](https://github.com/FastTrackStudios/arrangement-view), the [mixer](https://github.com/FastTrackStudios/mixer-view) and the [MIDI editor](https://github.com/FastTrackStudios/midi-editor). It is [Frame](https://github.com/FastTrackStudios/frame) documents that implement daw's theme spec ([daw#52](https://github.com/FastTrackStudios/daw/issues/52), `daw-theme`). Every view loads the same pack, so a mute or solo button looks the same in the track panel as on a mixer strip.

It's one of the `studio-*-theme` packs (`studio-reaper-theme` is REAPER's look, others such as `studio-protools-theme` to come). Anyone can make a pack, and Studio installs one from a GitHub link.

## What's in it

| File | What it draws | Used by |
|---|---|---|
| `themes/console/tokens.frame` | colours, sizes and type: the `Var`s every other file reads | every view |
| `themes/console/controls.frame` | each control as a component: `mute`, `solo`, `arm`, `monitor`, `phase`, `pan`, `volume`, `fx`, `routing`, … | every view |
| `themes/console/tcp.frame` | the track control panel beside the arrangement | arrangement |
| `themes/console/strip.frame` | one mixer channel strip, placing the controls by name | mixer |
| `themes/console/rack.frame`, `rails.frame` | the FX rack and the mixer's rails | mixer |
| `themes/console/editor.frame` | the editor's keys and lanes | MIDI editor |

## How a theme is built

- **Tokens** (`<Variables>`): named values (`mute`, `solo`, `meter.safe`, …), in one or more modes (`dark`).
- **Components** (`<Component name="…">`): one control each, placed with `<Instance of="…">`.
- **Pages**: what a view asks for, such as `mcp.strip` for a mixer strip.
- **Bindings** (`bind:track.name`, `bind:button.on`, …): what the view fills in at run time. The spec lists every field a view provides.

## Layers

A theme is a list of files or folders, base first. Each one replaces tokens, components and pages of the ones before it **by name**. So a theme built on this one is only the files it changes: one control, the strip, a colour.

Today Studio reads the layers from `FTS_THEME` (`:`-separated) and reloads them when a file changes:

```sh
FTS_THEME=themes/console Studio
FTS_THEME=themes/console:my-theme/ Studio   # this pack, then your changes on top
```

## Making your own pack

Start a repo with a `themes/` folder. Either copy `themes/console` and change it, or write only the files you want to differ and layer them over this pack. Studio installs a pack from its GitHub link; the install flow is part of daw's theme spec work.

## Status

The mixer strip draws from these files today (`FTS_THEME`). The track control panel, the arrangement, the rails, the Tone rack and the MIDI editor are still drawn natively, and move onto the theme as each view moves to its own repo. The copy of these files in the studio repo (`features/engine/ui/themes`) goes once the views load the pack.

## Licence

GPL-3.0-or-later.

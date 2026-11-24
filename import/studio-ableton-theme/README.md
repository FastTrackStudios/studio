# studio-ableton-theme

A flat, low-contrast look for Studio's views, in the style of Ableton Live: the [arrangement](https://github.com/FastTrackStudios/arrangement-view) and its track headers, and the [mixer](https://github.com/FastTrackStudios/mixer-view). It is [Frame](https://github.com/FastTrackStudios/frame) documents that implement daw's theme spec ([daw#52](https://github.com/FastTrackStudios/daw/issues/52), `daw-theme`), with a dark and a light theme.

It's a look-alike in style only. Nothing in it is taken from Ableton: no logos, icons, fonts or artwork. The colours are its own, and its type is Inter, which every Frame renderer bundles. Ableton and Ableton Live are trademarks of Ableton AG; this pack is not affiliated with or endorsed by them.

## Themes

| Theme | Mode |
|---|---|
| `ableton-dark` | dark greys |
| `ableton-light` | light greys |

Both are the same documents in two modes (`pack.styx`).

## What it looks like

- **Flat greys, one step apart**, every edge a one-pixel outline. No gradients, glows or shadows.
- **A fixed clip palette** of sixteen softened hues (`clip.0`…`clip.15`). A track's own colour snaps to the nearest, on its clips, its header bar and its strip title.
- **Compact type**: 10 px names, 8–9 px everything else.
- **The track header**: the track's colour as a thin bar at its left edge, its name, then one line of fields: routing, FX, a volume field filled from the left, a pan field drawn from the centre, the activator carrying the track's number, solo and arm.
- **The mixer strip**: the title in the track's colour, the FX and routing buttons, the pan knob, the gain, then a thin fader with a flat pointer beside a two-bar rectangular meter, and the activator, solo and arm at the foot.
- **The activator is lit while the track plays** and dims when it's muted, so `$mute` is the dim colour. The views' own painters use the same token, so every mute in Studio reads the same way.

## Files

| File | What it draws | Used by |
|---|---|---|
| `themes/ableton/tokens.frame` | every colour, size and font, in `dark` and `light` | every view |
| `themes/ableton/controls.frame` | the shared controls: `mute` (the activator), `solo`, `arm`, `monitor`, `phase`, `pan`, `pan.field`, `volume`, `fx`, `routing` | every view |
| `themes/ableton/tcp.frame` | `tcp.row`, the track header | arrangement |
| `themes/ableton/strip.frame` | `mcp.strip`, `mcp.rackback`, `mcp.step` | mixer |

The rails, the FX rack and the MIDI editor have no page here yet, so the views draw them natively in this pack's colours.

## Use

In Studio, with its actions:

```text
THEME_INSTALL {"link": "FastTrackStudios/studio-ableton-theme"}
THEME_SET     {"pack": "ableton", "theme": "ableton-light"}
THEME_CHECK   {}
```

Or for one run, by its files:

```sh
FTS_THEME=themes/ableton FTS_THEME_MODE=light Studio
```

Check it against the spec with daw's `daw-theme` command:

```sh
daw-theme check .
```

## Licence

GPL-3.0-or-later.

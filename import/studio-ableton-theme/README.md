# studio-ableton-theme

A flat, low-contrast look for Studio's views, in the style of Ableton Live: the [arrangement](https://github.com/FastTrackStudios/arrangement-view) and its track headers, and the [mixer](https://github.com/FastTrackStudios/mixer-view). It is [Frame](https://github.com/FastTrackStudios/frame) documents that implement daw's theme spec ([daw#52](https://github.com/FastTrackStudios/daw/issues/52), `daw-theme`), with a dark and a light theme.

It's a look-alike in style only. Nothing in it is taken from Ableton: no logos, icons, fonts or artwork. The colours are its own, and its type is Inter, which every Frame renderer bundles. Ableton and Ableton Live are trademarks of Ableton AG; this pack is not affiliated with or endorsed by them.

## Themes

| Theme | Mode |
|---|---|
| `ableton-light` | the mid-grey look: arrange `#a8a8a8`, rows `#b6b6b6` / `#adadad`, strips `#8d8d8d`, black type |
| `ableton-dark` | the same hues and controls on dark greys |

Both are the same documents in two modes (`pack.styx`).

## How it was measured

The visual reference is MyDaw, a third-party REAPER theme styled after Live. Its palette and layout were read for colours and proportions only; every part here is drawn again as vector Frame parts, and none of its images or files are in this repository.

- **Colours** from its palette: the rows, the arrange ground, black text and waveforms, the red edit cursor and black play cursor, the teal selection, the meter (green, yellow at the top, red over).
- **Proportions** from its layout: 15 px tall buttons and fields, 25 px wide buttons, an 80 px mixer strip, a 12 px meter down the track header's right edge, the fader's pointer at the meter, the name at the strip's foot.
- **Controls** from its art: the amber activator bar (grey when muted), heavy black **S**, record dot and **Fx** glyphs on flat grey faces with a one-pixel dark outline, blue solo `#3873ff`, red-orange arm `#ff563f`, cyan input monitoring, cyan-filled volume and pan fields, and the pan knob's cyan arc from the top over a thick dark track, with a pointer and a mark above it.

## What it looks like

- **Flat greys, one step apart**, every edge a one-pixel outline. No gradients, glows or shadows.
- **A fixed clip palette** of sixteen hues (`clip.0`…`clip.15`). A track's own colour snaps to the nearest, on its clips, its name box and its strip's name.
- **Compact, bold type**: Inter at 10 px for names, 8–11 px for glyphs and readouts.
- **The track header**: row one is the name in a box of the track's colour, then the activator (carrying the track's number), solo and arm. Row two, on a tall enough track, is the output field, **Fx**, and the volume and pan fields. A meter runs down the right edge.
- **The mixer strip**, top to foot: **Fx**, the output field, input monitoring, the gain, the fader pointer beside a two-bar meter in a dark well, the pan knob, the activator with solo and arm stacked beside it, and the name on the track's colour.
- **The activator is lit while the track plays** and goes grey when it's muted, so `$mute` is the grey. The views' own painters use the same token, so every mute in Studio reads the same way.

## Files

| File | What it draws | Used by |
|---|---|---|
| `themes/ableton/tokens.frame` | every colour, size and font, in `dark` and `light` | every view |
| `themes/ableton/controls.frame` | the shared controls: `mute` (the activator), `solo`, `arm`, `monitor`, `phase`, `pan` (the knob), `pan.field`, `volume`, `fx`, `routing` | every view |
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

# studio-reaper-theme

REAPER's look for Studio's views (the arrangement, the mixer and the MIDI editor): [Frame](https://github.com/FastTrackStudios/frame) documents that implement daw's theme spec ([daw#52](https://github.com/FastTrackStudios/daw/issues/52), `daw-theme`). It's one of the `studio-*-theme` packs, beside [studio-fts-theme](https://github.com/FastTrackStudios/studio-fts-theme).

## What's in it

| File | What it draws |
|---|---|
| `themes/fts-dark.frame` | the mixer strip in REAPER's layout, in FastTrackStudio's dark colours |

`fts-dark.frame` is generated from the native painters in the studio repo (`cargo run -r -p session-ui --bin theme_export`). It's the reference those painters are held to, pixel for pixel. The track control panel, the arrangement and the MIDI editor in REAPER's look are still to come.

## Use

In Studio, while it runs:

```text
THEME_SET {"pack": "reaper", "theme": "fts-dark"}
```

or for one run, `FTS_THEME=themes/fts-dark.frame Studio`. `pack.styx` names the theme; it passes spec 1 (`daw-theme check .`).

## Licence

GPL-3.0-or-later.

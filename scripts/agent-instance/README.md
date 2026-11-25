# A second Studio for an agent

`STUDIO_INSTANCE=<name>` runs a Studio beside the person's own without either
seeing the other:

- the window's app id is `Studio-<name>`, so no `^Studio$` window rule moves it;
- its doors are `~/.config/fts/studio-<name>-mcp.json` and
  `$XDG_RUNTIME_DIR/app-shell/studio-<name>.sock`;
- its engine lives in `studio-app-engine-<name>`.

`STUDIO_SINGLE_WINDOW=1` opens no Stage windows: one main window, and the
Charts screenset (`SESSION_SCREENSET {"screenset":"6"}`) fills it with the chart.

**Give it its own `XDG_RUNTIME_DIR`** (Linux). With a shared one, when the
agent's engine exits its window finds the person's engine and shows their song.
`run.sh` does this, and points display, bus and audio back at the desktop's.

Building a `dev/live` stack needs the local stack overrides
(`apps/studio/.cargo/config.toml`, made by `scripts/stack-overrides.py`, never
committed). Build inside the dev shell (`direnv exec .`): the system cargo is
older than the pins, and the dev shell carries the Wayland libraries.

- `run.sh`: launch it (Linux).
- `mcp.py`: call its MCP door (`list`, `run <ACTION> '<json>'`, `<tool> '<json>'`).
- `shot.py <file.png>`: screenshot its window.

`STUDIO_EXT_DEV` overrides are written to a shared `overrides.json`. End them
(`STUDIO_EXT_DEV_END`) on the person's Studio too; prefer
`STUDIO_DEV_EXTENSIONS` for one run.

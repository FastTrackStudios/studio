# A second Studio for an agent

`STUDIO_INSTANCE=<name>` runs a Studio beside the person's own without either
seeing the other:

- the window stays "Studio" (app id and title), so the desktop's window rules
  apply to it as to the person's;
- its doors are `~/.config/fts/studio-<name>-mcp.json` and
  `$XDG_RUNTIME_DIR/app-shell/studio-<name>.sock`;
- its engine lives in `studio-app-engine-<name>`, and it attaches only to an
  engine whose socket is under its own `XDG_RUNTIME_DIR` (every engine's
  discovery file is in the one shared `~/.config/fts/engines/`);
- its extensions folder is `~/.config/fts/studio-extensions-<name>` (the
  daw extension's `dev/daw` link to its engine program, its
  `STUDIO_EXT_DEV` overrides), never the person's `studio-extensions`.

`STUDIO_SINGLE_WINDOW=1` opens no Stage windows: one main window, and the
Charts screenset (`SESSION_SCREENSET {"screenset":"6"}`) fills it with the chart.

**Give it its own `XDG_RUNTIME_DIR`** (Linux): its engine's socket, control
socket and engine folder go there, and that is what tells its engine from the
person's. `run.sh` does this, and points display, bus and audio back at the
desktop's.

**Still shared** between instances: the engine's log
(`~/.cache/fts/studio-engine.log`, when Studio starts it itself), the
daw extension's session-lead port (`ws://127.0.0.1:4142`), and the systemd
unit name `studio-engine` (a second engine falls back to a session of its
own when that unit runs, or when its runtime dir has no user bus).

Build inside the dev shell: `just engine` (or `just studio`, which builds the
engine first) and the window under `just studio`. Open a **copy** of a song,
never the person's file: two engines autosaving one `.session` can damage it.

- `run.sh`: launch it (Linux).
- `mcp.py`: call its MCP door (`list`, `run <ACTION> '<json>'`, `<tool> '<json>'`).
- `shot.py <file.png>`: screenshot its window.

`STUDIO_EXT_DEV` overrides are kept in the instance's own extensions folder
(`dev/overrides.json`); `STUDIO_DEV_EXTENSIONS` overrides for one run.

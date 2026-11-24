# Live DAW quantization

The client reads a Rhythm hybrid plan, then uses the DAW Vox item, take,
stretch-marker and undo services. The REAPER extension and Studio's engine
route those services through their own backends.

A request names the current project GUID, one audio item GUID per mic, and
the plan. The items must share the same start and duration and have unit
play rate, with empty stretch maps. The client checks these conditions before
editing. It copies and places items, sets take offsets and fades, assigns a
group ID across the mics per slice, and writes stretch markers for the
stretched sections. When a protected slip leads into a tail stretch, the
transition marker is inside the item. The client batches duplication and placement through the DAW service, then
sets the required stretch maps before reading the edits back. It wraps the
operation in one undo block and asks the DAW to undo that block if a write or
readback fails and the edit is still the last undo entry.

## REAPER adapter

Use `git apply --unidiff-zero reaper-adapter.patch` in the isolated adapter
checkout. The patch applies to Studio adapters at
`6ce1b1a912e969071cd2fe55caaea0a5cd486252`. It registers the existing
stretch-marker service and returns stable item GUIDs from duplication.
DAW itself is pinned to the revision already used by that adapter.

The live demo was built in an isolated adapter checkout using its devenv
toolchain. The client source was added as a `daw-bridge` binary alongside the
plugin, with `rhythm-editing`, `serde`, and `serde_json` dependencies.
This reused the adapter workspace's dependency patches and Vox versions.
Build both with `cargo build -p daw-bridge --lib --bin rhythm-daw-live` in
that workspace. For a separate Cargo invocation using this directory's
manifest, supply the same adapter workspace Cargo patch tables with
`--config` and use the adapter dev shell.

Install `libreaper_daw_bridge.so` as `reaper_daw_bridge.so` in the REAPER
instance's UserPlugins directory. Start it with `FTS_SOCKET` set to a private
Unix socket path. Then inspect or apply:

```sh
rhythm-daw-live /absolute/path/to/reaper.sock
rhythm-daw-live /absolute/path/to/reaper.sock edit.json
```

The first form prints the current project GUID and its item GUIDs. Use those
in `project_guid` and `item_guids` in the request. Put the
`RHYTHM_QUANTIZE_PLAN` response's `plan` object in `plan`. Plan times are in
project seconds and must cover the selected item span.

The Unbreakable live excerpt used three original acoustic mic excerpts and
verified 30 items in ten phase groups, with three stretch markers on each
of the three stretched takes. The batched writes took 164 ms before readback. A pass over the original overlapping comp
items and verification against a current Studio engine remain future work.

## Studio and Session MCP

`studio_mcp.py` reads the running app's MCP discovery file under
`$XDG_CONFIG_HOME/fts` (or `~/.config/fts`) and uses the app's Action List.
Action calls default to Session's engine. Script calls default to the Studio
desktop host. Use `--host` to choose which action list receives the call, or
`--discovery` to use an isolated instance.

```sh
python3 integrations/daw-live/studio_mcp.py run SESSION_TRACKS
python3 integrations/daw-live/studio_mcp.py run HITS_LIST --args '{"pieces":["Kick"],"limit":10}'
python3 integrations/daw-live/studio_mcp.py run QUANTIZE --args '{"dry_run":true,"brief":true}'
python3 integrations/daw-live/studio_mcp.py describe HITS_PLACE
python3 integrations/daw-live/studio_mcp.py --host studio describe SCRIPT_RUN
python3 integrations/daw-live/studio_mcp.py script edit.luau --args '{"amount":4}'
```

`SCRIPT_RUN` runs Luau against the receiving host's actions using
`actions.run(id, args)`. Studio and its Session engine publish different
action lists. The current Session engine exposes `HITS_PLACE`, `HITS_STRETCH`
and `QUANTIZE`. Script access to those actions requires the host to register
the same engine actions with its scripting runtime.

Studio's existing engine actions use its current drum-edit implementation.
Connecting them to Rhythm's planner and applying a plan to a full multitrack
comp requires host integration. Rhythm's current stdio process is also
separate from Architect's Vox process-extension protocol.

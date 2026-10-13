"""Proposed crate split for session-daw. Run: python3 proposal.py
ASSIGN: module -> crate. EXTRACT: (module, symbol) -> crate the symbol moves to.
INVERT: (from_module, to_module, symbol) -> remedy (the edge is cut by a hook/trait/param).
Checks the resulting crate graph for cycles and prints every remaining edge."""
import json, sys
from collections import defaultdict
D = json.load(open("module_deps_detailed.json")); deps, L = D["deps"], D["lines"]

C = {
 "core": "num text jobs profile settings loading icons audio_mode window_cursor song_bars art popup which_key touch plan engine",
 "timeline": "lanes row layout cursor play_return bar_step grid_in_force hit pointer gesture mousemap tool zoom options toolbar midi wave_detail wave_pool ruler tcp track_icon folds track_regions marks_strip",
 "skin": "skin",
 "automation": "automation automation_actions automation_draw automation_edit automation_menu leveler",
 "drum": "hits drum_edit drum_lanes drum_level drum_reference drum_settings drum_group fundamental groove hit_overlay kit_gate quantize_panel written poi",
 "vocals": "vocal_level take_rating",
 "fx_rack": "tone tone_bind tone_read fx_actions plugin_faces faces macros beats live closeup simulate rig",
 "mixer": "mcp mixer_panel strip balance routes gain_stage mix_chains mix_export track_icon_unused overlay rails fx_view",
 "session": "open open_core project_file project_new projects versions session_file folder prepare setlist song_modes instrument_tracks organize grow guide_instrument section_colors",
 "collab": "collab collab_bar collab_pointers ghosts",
 "stream": "song_stream stream_in stream_set reference_play task_account task_set reference",
 "chart": "chart_panel chart_editor lyrics_panel song_chart",
 "arrangement": "arrangement arrange_edit widget comp swipe take_window rename folder_item notice fps",
 "removed": "bass_rig drum_samples rig_actions",
 "elsewhere": "patch_list editor_view expression",
 "app": "shell compact rails_unused settings_unused setup navigator mix_guide keys closeup_unused jobs_unused hook_rules mcp_server actions web_engine web_host web_audio device_audio headless animate studio panel progress transport_bar record timeline_actions browser record_view",
}
ASSIGN = {m: c for c, ms in C.items() for m in ms.split() if not m.endswith("_unused")}
ORDER = ["core", "timeline", "skin", "fx_rack", "automation", "vocals", "drum", "mixer", "session",
         "collab", "chart", "stream", "arrangement", "elsewhere", "removed", "app"]

def X(mod, syms, crate):
    return {(mod, s): crate for s in syms.split()}
EXTRACT = {}
# -- into core
EXTRACT |= X("arrangement", "Palette Viewport TCP_WIDTH submit_command", "core")       # core::paint
EXTRACT |= X("tcp", "glyphs", "core")                                                    # core::paint
EXTRACT |= X("art", "scaled path_of place squashed", "core")
EXTRACT |= X("shell", "DIM RULE TEXT ACCENT SCRIM BAR_BG BAR_H BOTTOM_H RAISED SAFE_AREA Back WindowSize Density use_density MixerToggle Pins EditorPane FxToggle EditorComing", "core")  # core::chrome
EXTRACT |= X("keys", "set_editing set_typing window_has_transport", "core")  # core::focus flags
EXTRACT |= X("keys", "Action Keys Visibility key_code", "timeline")  # Action carries zoom::Command
EXTRACT |= X("studio", "request_resync take_resync resync_pending Edits PPS css_delta css_point", "core")    # core::resync
EXTRACT |= X("actions", "list meta resolve song sections queue request_mode samples_follow KitPiece take_window window_pending open_song_unsaved open_song_versions", "core")  # core::action_kit (+ ActionList registry)
EXTRACT |= X("open", "runtime engine_runtime current_song set_current_song local_engine with_local_engine LocalOnly switch_song is_session request_mode mode switch_to", "core")  # core::open facade (= today's web stub)
EXTRACT |= X("gain_stage", "path_of", "core")
EXTRACT |= X("mcp", "f64_to_f32 STRIP_GAP STRIP_W", "core")
EXTRACT |= X("rails", "SIDE TOP", "core")
EXTRACT |= X("web_host", "Hosted HostedRef WidgetCanvas ElementSlot", "core")            # core::host
EXTRACT |= X("panel", "Button PanelEvent BAR", "core")
EXTRACT |= X("progress", "use_reading Song", "core")
EXTRACT |= X("ghosts", "Anchor register_anchor region_mounted ARRANGEMENT", "core")       # core::anchors
EXTRACT |= X("fx_actions", "moment", "core")
EXTRACT |= X("track_regions", "moment", "core")
EXTRACT |= X("compact", "use_sides use_form Form", "core")
EXTRACT |= X("touch", "", "core")
# -- into timeline
EXTRACT |= X("mcp", "Control", "timeline")                  # the strip-control vocabulary, beside row::Control
EXTRACT |= X("organize", "engine_edit", "timeline")         # returns cursor::Edit
EXTRACT |= X("automation", "lanes_height", "timeline")      # lane-height store -> layout
EXTRACT |= X("arrangement", "ChartMarks Lettering ItemZone", "timeline")
EXTRACT |= X("studio", "StudioSession Planner fetch Edits", "timeline")   # timeline::session_view
EXTRACT |= X("widget", "edit_song", "timeline")
# -- into skin
EXTRACT |= X("mcp", "FOLD_W INDENT_STEP strip_edge strip_ground", "skin")
EXTRACT |= X("strip", "shape", "skin")
EXTRACT |= X("overlay", "route_ink volume_label", "skin")
EXTRACT |= X("expression", "base_look look_fields", "skin")
# -- automation owns the leveler lane state
EXTRACT |= X("vocal_level", "has_lanes lane_shown leveled playing show_lane", "automation")
EXTRACT |= X("drum_level", "write_curve", "automation")
# -- session gets
# -- arrangement gets the arrangement-surface hit test
EXTRACT |= X("hit", "arrangement", "arrangement")

INVERT = {}
def I(frm, to, syms, why):
    for s in syms.split(): INVERT[(frm, to, s)] = why
SCENE = "take &impl timeline::SceneRows (trait the arrangement scene implements) instead of &arrangement::Arrangement"
for m in ["automation_draw", "ghosts", "notice", "overlay", "swipe", "hit"]:
    I(m, "arrangement", "Arrangement", SCENE)
HOOK = "engine change hook: engine::on_change(fn) registered by the owning crate at startup"
I("engine", "automation", "target_key", "move AutomationTarget key type into core::engine (Edit carries it)")
I("engine", "collab", "transport_pressed", "engine::hooks::on_transport_pressed, set by collab")
I("engine", "hit_overlay", "refresh", HOOK); I("engine", "midi", "mark_stale", HOOK)
I("engine", "wave_detail", "engine_changed", HOOK); I("engine", "web_audio", "latency_seconds", "engine::hooks::latency, set by web host")
I("engine", "mcp", "Control", "mcp::Control moves to timeline; engine::Edit uses a core ControlKind (or move Control to core)")
I("engine", "play_return", "Step get with_locate", "play_return is 265 lines, no deps: move it into core")
I("engine", "record", "toggle_through_the_list", "engine::hooks, set by app")
I("engine", "routes", "At", "routes::At type moves into core::engine")
I("track_regions", "drum_reference", "find_track notes pieces", "kit-region detection fns move from track_regions into drum crate")
I("track_regions", "hits", "kit", "kit-region detection fns move from track_regions into drum crate")
I("track_regions", "fx_actions", "moment", "moment -> core::action_kit")
I("open", "mcp_server", "start", "app starts the MCP server after open; not open's job")
I("open", "reference_play", "renderer", "session::open::register_reference_renderer, set by stream")
I("open", "stream_in", "start", "session::hooks::stream_start, set by stream")
I("setlist", "reference_play", "hear_local", "SongSource trait in session; stream implements it")
I("setlist", "song_stream", "StreamedSong", "SongSource trait in session; stream implements it")
I("setlist", "stream_set", "stream", "SongSource trait in session; stream implements it")
I("setlist", "collab", "song_opened", "session::hooks::on_song_opened, collab subscribes")
I("collab", "chart_editor", "local_caret", "collab::register_caret_source, set by chart")
I("automation_edit", "vocal_level", "relevel set_stages", "automation::on_leveler_edit hook, set by vocals")
I("drum_reference", "open_core", "Media load_into_with", "drum_reference takes a reference loader (fn(&Path)->Standalone) registered by session; or the reference-load step moves into session::prepare")
I("song_stream", "web_audio", "adopt", "stream::hooks::on_feeder (web), set by the web host")
EXTRACT |= X("editor_view", "picked", "timeline")   # the arrangement's picked-item slot
for s_ in "Arrangement WebArrangement EngineRef".split(): pass
I("widget", "actions", "through_list", "core::action_kit::dispatch hook: app installs the key-Action -> ActionList router")
I("mixer_panel", "studio", "Arrangement", "DawPanels/WebDawPanels (arrangement+mixer docked) move out of mixer_panel into app")
I("mixer_panel", "web_host", "WebArrangement", "DawPanels/WebDawPanels move into app")
I("mixer_panel", "web_engine", "EngineRef", "DawPanels/WebDawPanels move into app")

def home(mod, sym):
    return EXTRACT.get((mod, sym), ASSIGN[mod])
assert set(ASSIGN) == set(deps), (set(deps) - set(ASSIGN), set(ASSIGN) - set(deps))
edges = defaultdict(list)   # (crateA, crateB) -> [(m, t, sym)]
inverted = []
for m, ts in deps.items():
    a = ASSIGN[m]
    for t, k in ts.items():
        for s in k.get("prod", []):
            if (m, t, s) in INVERT: inverted.append((m, t, s, INVERT[(m, t, s)])); continue
            b = home(t, s)
            if a != b: edges[(a, b)].append((m, t, s))
rank = {c: i for i, c in enumerate(ORDER)}
viol = {k: v for k, v in edges.items() if rank[k[0]] < rank[k[1]]}
if __name__ == "__main__":
    lines = defaultdict(int)
    for m, n in L.items(): lines[ASSIGN[m]] += n
    print("crate lines:", dict(sorted(lines.items(), key=lambda x: rank[x[0]])))
    print(f"{len(EXTRACT)} extracted symbols, {len(inverted)} inverted refs")
    print("UPWARD (cycle-causing) edges remaining:")
    for (a, b), refs in sorted(viol.items(), key=lambda x: (rank[x[0][0]], rank[x[0][1]])):
        print(f" {a} -> {b}:")
        for m, t, s in refs: print(f"    {m} -> {t}::{s}")
    json.dump({"order": ORDER, "assign": ASSIGN,
               "extract": {f"{m}::{s}": c for (m, s), c in EXTRACT.items()},
               "invert": [{"from": m, "to": t, "symbol": s, "remedy": r} for m, t, s, r in inverted],
               "crate_edges": {f"{a} -> {b}": [f"{m} -> {t}::{s}" for m, t, s in v] for (a, b), v in sorted(edges.items())},
               "violations": {f"{a} -> {b}": [f"{m} -> {t}::{s}" for m, t, s in v] for (a, b), v in viol.items()}},
              open("proposal_result.json", "w"), indent=1)

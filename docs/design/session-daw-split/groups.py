G = {
 "arrangement": "arrangement arrange_edit widget tcp folder_item folds ruler track_regions marks_strip comp swipe take_window rename",
 "timeline": "lanes row layout cursor play_return bar_step grid_in_force hit pointer gesture mousemap tool zoom options toolbar midi wave_detail wave_pool",
 "mixer": "mcp mixer_panel strip skin balance routes gain_stage mix_chains mix_export mix_guide track_icon",
 "automation": "automation automation_actions automation_draw automation_edit automation_menu",
 "drum": "hits drum_edit drum_lanes drum_level drum_reference drum_settings drum_group fundamental groove hit_overlay kit_gate quantize_panel written poi",
 "vocals": "vocal_level leveler take_rating record_view",
 "fx_rack": "tone tone_bind tone_read fx_view fx_actions plugin_faces faces macros beats live",
 "chart": "chart_panel chart_editor lyrics_panel song_chart section_colors song_bars",
 "collab": "collab collab_bar collab_pointers ghosts",
 "stream": "song_stream stream_in stream_set reference_play task_account task_set reference",
 "session": "open open_core project_file project_new projects versions session_file folder prepare setlist song_modes instrument_tracks organize grow guide_instrument",
 "removed": "bass_rig drum_samples rig rig_actions",
 "elsewhere": "patch_list editor_view expression",
 "app": "shell compact touch rails settings setup navigator loading notice popup which_key keys window_cursor closeup jobs hook_rules mcp_server actions web_engine web_host web_audio device_audio audio_mode engine text art headless fps profile animate simulate overlay icons studio panel plan progress transport_bar record timeline_actions num browser",
}
GROUP = {m: g for g, ms in G.items() for m in ms.split()}

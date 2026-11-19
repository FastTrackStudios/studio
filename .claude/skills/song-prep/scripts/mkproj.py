"""Write a REAPER Lua build script for one reference song.

usage: python mkproj.py <beats.json> <config.json> <out.lua>
config: {rpp, folder, base, mix, bar1: beat index of song bar 1, drop_before: beat index (earlier beats are noise),
         sections: [[song_bar, name, "#rrggbb"], ...]}
Song bar 1 = project bar 2 (bar 1 is a count-in). One tempo point per beat from bar 1 on.
If the RPP is already open in a tab, that tab is rebuilt in place; otherwise a new tab is opened.
"""
import json, sys

beats = json.load(open(sys.argv[1]))['beats']
cfg = json.load(open(sys.argv[2]))
import numpy as np
k = cfg['bar1']
Bb = np.array(beats[k:])
srcs = json.load(open(sys.argv[1])).get('src', [])[k:]
w_all = np.array([0.3 if (i < len(srcs) and srcs[i] in ('between', 'dp', 'interp')) else 1.0 for i in range(len(Bb))])
idx = np.arange(len(Bb))
# each downbeat = local line fit through beats -4..+4 around it (fills / single hits can't drag a bar line)
D = []
raw_from = cfg.get('raw_from_bar')  # optional: from this song bar on, downbeats = the beats as given (ritards)
for i in range(0, len(Bb), 4):
    if raw_from and i // 4 + 1 >= raw_from:
        D.append(float(Bb[i])); continue
    lo, hi = max(0, i - 4), min(len(Bb), i + 5)
    D.append(float(np.polyval(np.polyfit(idx[lo:hi], Bb[lo:hi], 1, w=w_all[lo:hi]), i)))
if (len(Bb) - 1) % 4 != 0:  # close the last bar from the local beat period
    per = float(np.median(np.diff(Bb[-8:])))
    D.append(D[-1] + 4 * per)
B = D                                   # tempo points: one per bar
bpm = [240 / (B[i + 1] - B[i]) for i in range(len(B) - 1)]
bpm.append(bpm[-1])
# how far the actual beats sit from the steady in-bar grid (info)
dev = []
for j in range(len(B) - 1):
    for q in range(4):
        i = 4 * j + q
        if i < len(Bb) and w_all[i] == 1.0:
            dev.append(abs(Bb[i] - (B[j] + q * (B[j + 1] - B[j]) / 4)) * 1000)
print('beats vs per-bar grid: median %.1f ms, 95%% %.1f ms, max %.1f ms' % (np.median(dev), np.percentile(dev, 95), max(dev)))
n_count = 1
while 240 * n_count / bpm[0] < B[0] + 0.05:
    n_count += 1
off = 240 * n_count / bpm[0] - B[0]
countin_bpm = bpm[0]
if cfg.get('one_countin') and n_count > 1:  # one long count-in bar holding all the pre-song audio
    n_count, off = 1, 0.0
    countin_bpm = 240 / B[0]


def q(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"') + '"'


ORDER = [('CLICK', None, 'ffc800', 0), ('MIX', 'Mix', 'c8c8c8', 0), ('VOCALS', 'Vocals', 'e65a5a', 0),
         ('DRUMS', None, '5a8ce6', 1), ('Kick', 'Kick', '5a8ce6', 0), ('Snare', 'Snare', '5a8ce6', 0),
         ('Hi-Hat', 'Hi-Hat', '5a8ce6', 0), ('Toms', 'Toms', '5a8ce6', 0), ('Cymbals', 'Cymbals', '5a8ce6', 0),
         ('Drums Other', 'Drums Other', '5a8ce6', -1), ('BASS', 'Bass', '5ac878', 0), ('GUITAR', 'Guitar', 'e6963c', 0),
         ('PIANO', 'Piano', 'aa6edc', 0), ('OTHER', 'Other', '969696', 0)]
tracks = []
for name, stem, col, fold in ORDER:
    path = None
    if stem == 'Mix':
        path = cfg['mix']
    elif stem:
        path = f"{cfg['folder']}/{cfg['base']} ({stem}).wav"
    tracks.append('{name=%s, file=%s, color=0x%s, folder=%d}' % (q(name), q(path) if path else 'nil', col, fold))
marks = ', '.join('{%d, %s}' % (b, q(n)) for b, n in cfg.get('markers', []))
secs = ',\n'.join('{%d, %s, 0x%s}' % (b, q(n), c.lstrip('#')) for b, n, c in cfg['sections'])

lua = f"""-- Build / rebuild reference project: {cfg['base']}
local RPP = {q(cfg['rpp'])}
local OFF = {off!r}
local NCOUNT = {n_count}
local COUNTIN_BPM = {countin_bpm!r}
local BEATS = {{{', '.join(f'{x:.6f}' for x in B)}}}
local BPM = {{{', '.join(f'{x:.4f}' for x in bpm)}}}
local TRACKS = {{{', '.join(tracks)}}}
local SECS = {{{secs}}}
local MARKS = {{{marks}}}
local function rgb(c) return reaper.ColorToNative((c >> 16) & 255, (c >> 8) & 255, c & 255) | 0x1000000 end

-- find an open tab with this project, else open a new tab
local found
local i = 0
while true do
  local p, fn = reaper.EnumProjects(i)
  if not p then break end
  if fn == RPP then found = p end
  i = i + 1
end
if found then reaper.SelectProjectInstance(found) else reaper.Main_OnCommand(40859, 0) end
local proj = 0
reaper.Undo_BeginBlock()
reaper.PreventUIRefresh(1)
-- clear (only this project's tab)
for t = reaper.CountTracks(proj) - 1, 0, -1 do reaper.DeleteTrack(reaper.GetTrack(proj, t)) end
for m = reaper.CountTempoTimeSigMarkers(proj) - 1, 0, -1 do reaper.DeleteTempoTimeSigMarker(proj, m) end
local _, nm, nr = reaper.CountProjectMarkers(proj)
for m = nm + nr - 1, 0, -1 do reaper.DeleteProjectMarkerByIndex(proj, m) end

-- tempo map: count-in bar, then one point per beat
reaper.SetTempoTimeSigMarker(proj, -1, 0, -1, -1, COUNTIN_BPM, 4, 4, false)
for b = 1, #BEATS do reaper.SetTempoTimeSigMarker(proj, -1, BEATS[b] + OFF, -1, -1, BPM[b], 0, 0, false) end

local songlen = 0
for t, d in ipairs(TRACKS) do
  reaper.InsertTrackAtIndex(t - 1, false)
  local tr = reaper.GetTrack(proj, t - 1)
  reaper.GetSetMediaTrackInfo_String(tr, "P_NAME", d.name, true)
  reaper.SetMediaTrackInfo_Value(tr, "I_CUSTOMCOLOR", rgb(d.color))
  reaper.SetMediaTrackInfo_Value(tr, "I_FOLDERDEPTH", d.folder)
  if d.file and reaper.file_exists(d.file) then
    local src = reaper.PCM_Source_CreateFromFile(d.file)
    local len = reaper.GetMediaSourceLength(src)
    if d.name == "MIX" then songlen = len end
    local it = reaper.AddMediaItemToTrack(tr)
    local tk = reaper.AddTakeToMediaItem(it)
    reaper.SetMediaItemTake_Source(tk, src)
    reaper.SetMediaItemInfo_Value(it, "D_POSITION", OFF)
    reaper.SetMediaItemInfo_Value(it, "D_LENGTH", len)
    reaper.GetSetMediaItemTakeInfo_String(tk, "P_NAME", d.name, true)
  end
  if d.name ~= "MIX" and d.name ~= "CLICK" and d.name ~= "DRUMS" then reaper.SetMediaTrackInfo_Value(tr, "B_MUTE", 1) end
end
local click = reaper.GetTrack(proj, 0)
reaper.SetMediaTrackInfo_Value(click, "D_VOL", 0.5)
reaper.SetOnlyTrackSelected(click)
reaper.GetSet_LoopTimeRange(true, false, 0, OFF + songlen, false)
reaper.Main_OnCommand(40013, 0) -- Insert click source (follows the tempo map)
reaper.GetSet_LoopTimeRange(true, false, 0, 0, false)

-- sections as regions (song bar N = project measure index N, i.e. bar N+1)
for s = 1, #SECS do
  local a = reaper.TimeMap2_beatsToTime(proj, 0, SECS[s][1] + NCOUNT - 1)
  local e = s < #SECS and reaper.TimeMap2_beatsToTime(proj, 0, SECS[s + 1][1] + NCOUNT - 1) or (OFF + songlen)
  if SECS[s][2] == "End" then
    reaper.AddProjectMarker2(proj, false, a, 0, "End", s, rgb(SECS[s][3]))
  else
    reaper.AddProjectMarker2(proj, true, a, e, SECS[s][2], s, rgb(SECS[s][3]))
  end
end
for m = 1, #MARKS do
  reaper.AddProjectMarker2(proj, false, reaper.TimeMap2_beatsToTime(proj, 0, MARKS[m][1] + NCOUNT - 1), 0, MARKS[m][2], 100 + m, rgb(0xffc800))
end
reaper.SetEditCurPos(0, true, false)
reaper.PreventUIRefresh(-1)
reaper.UpdateArrange(); reaper.UpdateTimeline()
reaper.Undo_EndBlock("Build reference project", -1)

-- verify every beat is on the grid and bar 1 lands on project bar 2
local worst, dbw = 0, 0
for b = 1, #BEATS do
  local t = BEATS[b] + OFF
  local qn = reaper.TimeMap2_timeToQN(proj, t)
  worst = math.max(worst, math.abs(qn - math.floor(qn + 0.5)) * 60 / BPM[b] * 1000)
  local inbar = reaper.TimeMap2_timeToBeats(proj, t); dbw = math.max(dbw, math.min(inbar, 4 - inbar))
end
local _, m1 = reaper.TimeMap2_timeToBeats(proj, BEATS[1] + OFF)
reaper.Main_SaveProjectEx(proj, RPP, 0)
return {{rebuilt_existing_tab = found ~= nil, bars = #BEATS, tracks = reaper.CountTracks(proj), worst_ms = worst, worst_downbeat = dbw, bar1_project_bar = m1 + 1}}
"""
open(sys.argv[3], 'w').write(lua)
print('count-in bars', n_count, 'count-in bpm', round(countin_bpm, 2), 'offset', round(off, 4), 'bars', len(B) - 1, 'bpm range', round(min(bpm), 1), round(max(bpm), 1))

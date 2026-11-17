"""Chord/bass guesses from a show set's audio stems, per bar, half bar, beat or 8th.

usage: ./py/bin/python3 -I stem_chords.py SET.als SAMPLES_DIR LO HI "bassTrack|..." "harmTrack|..." [--half|--beats|--eighths] [--chroma] [--all]

LO/HI are arrangement beats (bar 1 = LO). Each cell prints `Chord[bassroot]`;
--chroma adds each bar's top pitch classes; --all includes disabled clips.
Clips map to file time through warp markers (warped) or LoopStart seconds at
the project tempo (unwarped). The bass CQT starts at E1: notes below it show
as their 5th. Machine analysis: confirm against tabs, chord clips or ears.
"""
import gzip, sys, os
import xml.etree.ElementTree as ET
import numpy as np
import soundfile as sf
import librosa

als, sdir, lo, hi, btracks, htracks = sys.argv[1], sys.argv[2], float(sys.argv[3]), float(sys.argv[4]), sys.argv[5].split('|'), sys.argv[6].split('|')
keyroot = None
if '--key' in sys.argv:
    keyroot = sys.argv[sys.argv.index('--key') + 1]
root = ET.fromstring(gzip.open(als).read())
ls = root.find('LiveSet')
NN = ['C', 'Db', 'D', 'Eb', 'E', 'F', 'F#', 'G', 'Ab', 'A', 'Bb', 'B']

# meter envelope
ts = []
for env in ls.find('MainTrack').iter('AutomationEnvelope'):
    if env.find('EnvelopeTarget/PointeeId').get('Value') == ls.find('MainTrack').find('.//TimeSignature/AutomationTarget').get('Id'):
        for e in env.find('Automation/Events'):
            v = int(float(e.get('Value')))
            num = v % 99 + 1
            den = 2 ** (v // 99)
            ts.append((float(e.get('Time')), num, den))
ts.sort(key=lambda x: x[0])

def meter_at(b):
    m = (4, 4)
    for t, n, d in ts:
        if t <= b + 1e-6:
            m = (n, d)
    return m

bars = []
b = lo
while b < hi - 1e-6:
    n, d = meter_at(b)
    ln = n * 4 / d
    bars.append((b, ln, n))
    b += ln

def clips_for(names):
    out = []
    for tr in ls.find('Tracks'):
        nm = tr.find('Name/EffectiveName').get('Value')
        if nm not in names:
            continue
        for c in tr.iter('AudioClip'):
            s = float(c.find('CurrentStart').get('Value'))
            e = float(c.find('CurrentEnd').get('Value'))
            if e <= lo or s >= hi:
                continue
            if '--all' not in sys.argv and c.find('Disabled') is not None and c.find('Disabled').get('Value') == 'true':
                continue
            rel = c.find('.//SampleRef/FileRef/RelativePath').get('Value')
            fname = os.path.join(sdir, os.path.basename(rel))
            wm = sorted((float(w.get('BeatTime')), float(w.get('SecTime'))) for w in c.find('WarpMarkers'))
            lstart = float(c.find('Loop/LoopStart').get('Value'))
            srel = float(c.find('Loop/StartRelative').get('Value'))
            warped = c.find('IsWarped').get('Value') == 'true'
            out.append(dict(s=s, e=e, f=fname, wm=wm, off=lstart + srel, warped=warped, track=nm))
    return out

TEMPO = []
_mt = ls.find('MainTrack'); _tid = _mt.find('.//Tempo/AutomationTarget').get('Id')
for _env in _mt.iter('AutomationEnvelope'):
    if _env.find('EnvelopeTarget/PointeeId').get('Value') == _tid:
        TEMPO = sorted(((float(e.get('Time')), float(e.get('Value'))) for e in _env.find('Automation/Events')), key=lambda x: x[0])
def _bpm(b):
    v = TEMPO[0][1]
    for t, x in TEMPO:
        if t <= b + 1e-9: v = x
    return v
def beat_to_sec(c, b):
    if not c['warped']:
        # unwarped: LoopStart is seconds into the file; time runs at the project tempo
        secs = 0.0; x = c['s']
        while x < b - 1e-9:
            d = min(0.25, b - x); secs += d * 60 / _bpm(x); x += d
        return c['off'] + secs
    cb = b - c['s'] + c['off']
    wm = c['wm']
    if len(wm) == 1:
        return wm[0][1] + (cb - wm[0][0]) * 0.5
    for i in range(len(wm) - 1):
        if cb <= wm[i + 1][0] or i == len(wm) - 2:
            (b0, s0), (b1, s1) = wm[i], wm[i + 1]
            return s0 + (cb - b0) * (s1 - s0) / (b1 - b0)

cache = {}
def load(f):
    if f not in cache:
        y, sr = sf.read(f, always_2d=True)
        y = y.mean(axis=1).astype(np.float32)
        if sr != 22050:
            y = librosa.resample(y, orig_sr=sr, target_sr=22050)
        cache[f] = y
    return cache[f]

def chroma_span(clips, b0, b1, bass=False):
    acc = np.zeros(12)
    energy = 0.0
    for c in clips:
        if b1 <= c['s'] or b0 >= c['e']:
            continue
        x0, x1 = max(b0, c['s']), min(b1, c['e'])
        t0, t1 = beat_to_sec(c, x0), beat_to_sec(c, x1)
        y = load(c['f'])
        i0, i1 = int(t0 * 22050), int(t1 * 22050)
        seg = y[max(0, i0):max(0, i1)]
        if len(seg) < 2048:
            continue
        if bass:
            C = np.abs(librosa.cqt(seg, sr=22050, fmin=librosa.note_to_hz('E1'), n_bins=36, bins_per_octave=12, hop_length=512))
            v = C.mean(axis=1)
            ch = np.zeros(12)
            for k in range(36):
                ch[(k + 4) % 12] += v[k]
        else:
            ch = librosa.feature.chroma_cqt(y=seg, sr=22050, hop_length=512).mean(axis=1)
        e = float(np.sqrt(np.mean(seg ** 2)))
        acc += ch * e
        energy += e
    return acc, energy

TPL = {'': [0, 4, 7], 'm': [0, 3, 7], '7': [0, 4, 7, 10], 'maj7': [0, 4, 7, 11], 'm7': [0, 3, 7, 10], 'sus4': [0, 5, 7], 'sus2': [0, 2, 7]}
def best_chord(ch, bassch):
    if ch.sum() <= 0:
        return '-'
    ch = ch / (ch.max() + 1e-9)
    broot = int(np.argmax(bassch)) if bassch.sum() > 0 else None
    best = None
    for r in range(12):
        for q, iv in TPL.items():
            t = np.zeros(12)
            t[[(r + i) % 12 for i in iv]] = 1
            sc = (ch * t).sum() / len(iv) - 0.35 * (ch * (1 - t)).sum() / (12 - len(iv))
            if q in ('7', 'maj7', 'm7', 'sus4', 'sus2'):
                sc -= 0.04
            if broot is not None and broot == r:
                sc += 0.08
            if best is None or sc > best[0]:
                best = (sc, NN[r] + q)
    name = best[1]
    if broot is not None and not name.startswith(NN[broot]):
        name += '/' + NN[broot]
    return name

bclips = clips_for(btracks)
hclips = clips_for(htracks)
print('bass clips', [(c['track'], c['s'], c['warped']) for c in bclips])
print('harm clips', [(c['track'], c['s'], c['warped']) for c in hclips])
tot = np.zeros(12)
step = 2.0 if '--half' in sys.argv else None
for i, (b0, ln, n) in enumerate(bars):
    parts = []
    seg = step or ln
    if '--beats' in sys.argv:
        seg = 1.0
    if '--eighths' in sys.argv:
        seg = 0.5
    x = b0
    while x < b0 + ln - 1e-6:
        x1 = min(x + seg, b0 + ln)
        bc, be = chroma_span(bclips, x, x1, bass=True)
        hc, he = chroma_span(hclips, x, x1)
        tot += hc
        broot = NN[int(np.argmax(bc))] if be > 1e-4 else '.'
        parts.append(f'{best_chord(hc + 0.3 * (bc / (bc.max() + 1e-9)) * hc.max() if he > 0 else hc, bc):>9}[{broot:>2}]')
        x = x1
    print(f'bar {i+1:3d} @{b0:8.2f} {n}/4 ' + ' '.join(parts))
    if '--chroma' in sys.argv:
        hc,he=chroma_span(hclips,b0,b0+ln)
        if he>0:
            v=hc/hc.max(); print('      ', ' '.join(f'{NN[k]}:{v[k]:.2f}' for k in np.argsort(-v)[:6]))
print('KEY PROFILE', ' '.join(f'{NN[k]}:{tot[k]/tot.max():.2f}' for k in range(12)))

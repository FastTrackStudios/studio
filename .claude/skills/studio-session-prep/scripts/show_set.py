"""Read an Ableton/AbleSet show set (.als): its songs, and per song the
sections, tempo/meter changes, the helper tracks' clips and an LRC.

usage (always `python3 -I`, from outside the downloaded set's folder):
  show_set.py SET.als songs
  show_set.py SET.als song "<Song locator name>"     # sections, tempo, meter, cue/chord/patch clips, lyric lines
  show_set.py SET.als lrc  "<Song locator name>"     # LRC from the Lyrics clips, 0:00 = the song locator

Stdlib only. Bars in the output are counted from the first SECTIONS clip
after the count-in (bar 1), in 4-beat steps unless the meter map says otherwise.
"""
import gzip
import sys
import xml.etree.ElementTree as ET

END_NAMES = {'SONG END', 'END', 'STOP'}


def load(path):
    return ET.fromstring(gzip.open(path).read()).find('LiveSet')


def envelope(ls, name):
    """Events of the MainTrack (Live 12) / MasterTrack envelope for Tempo or TimeSignature."""
    mt = ls.find('MainTrack')
    if mt is None:
        mt = ls.find('MasterTrack')
    tid = mt.find('.//%s/AutomationTarget' % name).get('Id')
    for env in mt.iter('AutomationEnvelope'):
        if env.find('EnvelopeTarget/PointeeId').get('Value') == tid:
            # stable sort on time only: a step change is two events at one time, in document order
            return sorted(((float(e.get('Time')), float(e.get('Value'))) for e in env.find('Automation/Events')), key=lambda x: x[0])
    return [(-1e9, float(mt.find('.//%s/Manual' % name).get('Value')))]


def meter(value):
    """TimeSignature enum: (num-1) + 99*log2(den). 201 = 4/4, 200 = 3/4, 202 = 5/4."""
    v = int(value)
    return v % 99 + 1, 2 ** (v // 99)


def value_at(events, beat):
    v = events[0][1]
    for t, x in events:
        if t <= beat + 1e-9:
            v = x
    return v


def seconds(tempo, b0, b1):
    """Seconds from beat b0 to b1 through the tempo map (ramps are stored as steps)."""
    s, x = 0.0, b0
    while x < b1 - 1e-9:
        d = min(0.125, b1 - x)
        s += d * 60 / value_at(tempo, x)
        x += d
    return s


def locators(ls):
    out = []
    for loc in ls.find('Locators/Locators'):
        out.append((float(loc.find('Time').get('Value')), loc.find('Name').get('Value')))
    return sorted(out)


def songs(ls):
    locs = locators(ls)
    starts = [(t, n) for t, n in locs if not n.startswith('>') and n.strip().upper() not in END_NAMES]
    res = []
    for i, (t, n) in enumerate(starts):
        nxt = starts[i + 1][0] if i + 1 < len(starts) else 1e12
        ends = [x for x, m in locs if m.strip().upper() in END_NAMES and t < x <= nxt]
        res.append((n, t, ends[0] if ends else nxt))
    return res


def track_clips(ls, prefix, lo, hi):
    """Clips (start, end, name) on tracks whose name starts with prefix, overlapping [lo, hi)."""
    out = []
    for tr in ls.find('Tracks'):
        nm = tr.find('Name/EffectiveName').get('Value')
        if not nm.startswith(prefix):
            continue
        for c in tr.iter():
            if c.tag not in ('MidiClip', 'AudioClip'):
                continue
            s = float(c.find('CurrentStart').get('Value'))
            e = float(c.find('CurrentEnd').get('Value'))
            dis = c.find('Disabled')
            if e <= lo or s >= hi or (dis is not None and dis.get('Value') == 'true'):
                continue
            out.append((s, e, c.find('Name').get('Value').replace('’', "'")))
    return sorted(set(out))


def find_song(ls, name):
    for n, t, e in songs(ls):
        if n == name:
            return t, e
    sys.exit('no song locator named %r' % name)


def bar_of(beat, bar1, ts):
    """Bar.beat of an arrangement beat, counting bars through the meter map from bar1."""
    b, bar = bar1, 1
    if beat < bar1:
        return '%.2f(pre)' % ((beat - bar1) / 4)
    while True:
        n, d = meter(value_at(ts, b))
        ln = n * 4 / d
        if beat < b + ln - 1e-9:
            return '%d:%g' % (bar, (beat - b) * d / 4 + 1)
        b += ln
        bar += 1


def cmd_song(ls, name):
    lo, hi = find_song(ls, name)
    tempo, ts = envelope(ls, 'Tempo'), envelope(ls, 'TimeSignature')
    secs = track_clips(ls, 'SECTIONS', lo, hi)
    body = [c for c in secs if not c[2].lower().startswith('count')]
    bar1 = body[0][0] if body else lo
    print('song %r: beats %g-%g (%.1f s); bar 1 = beat %g' % (name, lo, hi, seconds(tempo, lo, hi), bar1))
    print('tempo changes:', [(t, v) for t, v in tempo if lo <= t < hi])
    print('meter changes:', [(t, '%d/%d' % meter(v)) for t, v in ts if lo <= t < hi] or 'none (%d/%d)' % meter(value_at(ts, lo)))
    print('\nSECTIONS:')
    for s, e, n in secs:
        n_, d_ = meter(value_at(ts, s))
        print('  %-24s beat %8g  bar %-8s %g bars' % (n, s, bar_of(s, bar1, ts), (e - s) * d_ / 4 / n_))
    for prefix in ('Chords', 'Count Ins', 'FoH Notes', 'GP', 'Lyrics'):
        clips = track_clips(ls, prefix, lo, hi)
        clips = [c for c in clips if not c[2].strip().isdigit()]  # drop the 1-2-3-4 count clips
        if clips:
            print('\n%s:' % prefix)
            for s, e, n in clips:
                print('  bar %-8s %r' % (bar_of(s, bar1, ts), n))


def cmd_lrc(ls, name):
    lo, hi = find_song(ls, name)
    tempo = envelope(ls, 'Tempo')
    lines = [c for c in track_clips(ls, 'Lyrics', lo, hi) if c[0] >= lo]
    print('[ti:%s]' % name)
    prev = None
    for s, e, n in lines:
        if prev is not None and s - prev > 2:  # a gap: blank stamp = singing stops
            t = seconds(tempo, lo, prev)
            print('[%02d:%05.2f]' % (t // 60, t % 60))
        t = seconds(tempo, lo, s)
        print('[%02d:%05.2f]%s' % (t // 60, t % 60, n.replace('*', '')))
        prev = e
    if prev is not None:
        t = seconds(tempo, lo, prev)
        print('[%02d:%05.2f]' % (t // 60, t % 60))


def main():
    ls = load(sys.argv[1])
    cmd = sys.argv[2]
    if cmd == 'songs':
        tempo = envelope(ls, 'Tempo')
        for n, t, e in songs(ls):
            print('%-40s beats %8g-%-8g %6.1f s  %g bpm' % (n, t, e, seconds(tempo, t, e), value_at(tempo, t + 0.01)))
    elif cmd == 'song':
        cmd_song(ls, sys.argv[3])
    elif cmd == 'lrc':
        cmd_lrc(ls, sys.argv[3])
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main()

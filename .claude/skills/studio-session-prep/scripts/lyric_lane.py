"""Write `[lyrics lrc]` cue lanes into a keyflow chart from a show set's Lyrics clips.

usage: python3 -I lyric_lane.py SET.als "<Song locator name>" chart.kf out.kf

Each lyric clip becomes one lane line, `@bar:beat first words…`, at the clip's
start (8th grid) in the section it starts in. The cue keeps about 0.6 words
per beat until the next line, so dense passages stay readable. Section starts
come from the SECTIONS clips (count-in skipped) and must pair 1:1, in order,
with the chart's section headers; the script stops if the counts differ.
Bars are section-relative in 4/4 (the lane's convention); rewrite for other meters.
Existing `[lyrics lrc]` lanes in the chart are replaced.
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import show_set  # noqa: E402

HEADER = re.compile(r'^(IN|VS|PRE|CH|BR|INST|SOLO|INT|OUT)\b|^\[(?!lyrics)[^\]]+\]\s*\d')


def main():
    als, song, chart, out = sys.argv[1:5]
    ls = show_set.load(als)
    lo, hi = show_set.find_song(ls, song)
    starts = [s for s, e, n in show_set.track_clips(ls, 'SECTIONS', lo, hi) if not n.lower().startswith('count')]
    lines = [(s, n) for s, e, n in show_set.track_clips(ls, 'Lyrics', lo - 8, hi)]
    lanes = {i: [] for i in range(len(starts))}
    for j, (s, txt) in enumerate(lines):
        gap = lines[j + 1][0] - s if j + 1 < len(lines) else 99
        nw = max(2, min(10, int(gap * 0.6)))
        q = round(s * 2) / 2
        i = max([k for k, st in enumerate(starts) if st <= q + 1e-6] or [0])
        rel = q - starts[i]
        bar = int(rel // 4) + 1
        beat = rel - (bar - 1) * 4 + 1
        words = txt.replace('\\', ' ').replace('*', '').replace('♪', '').split()
        if not words:
            continue
        cue = ' '.join(words[:nw]).rstrip(',.;:!?')
        if len(words) > nw:
            cue += '…'
        lanes[i].append(('@%d' % bar if beat == 1 else '@%d:%g' % (bar, beat)) + ' ' + cue)

    src = open(chart).read().split('\n')
    # drop old lanes: a `[lyrics lrc]` line and the lane lines after it, up to a blank line
    kept, in_lane = [], False
    for line in src:
        if line.strip().lower() == '[lyrics lrc]':
            in_lane = True
            continue
        if in_lane and line.strip() and not line.startswith('['):
            continue
        in_lane = False
        kept.append(line)
    src = kept
    idx = [n for n, line in enumerate(src) if HEADER.match(line)]
    if len(idx) != len(starts):
        sys.exit('chart has %d sections, SECTIONS clips %d: pair them first' % (len(idx), len(starts)))
    res = []
    for k, n in enumerate(idx):
        stop = idx[k + 1] if k + 1 < len(idx) else len(src)
        if k == 0:
            res += src[:n]
        body = src[n:stop]
        trailing = []
        while body and (body[-1].strip() == '' or body[-1].lstrip().startswith(';')):
            trailing.insert(0, body.pop())  # keep comments that belong to the next section
        res += body
        if lanes[k]:
            res += ['[lyrics lrc]'] + lanes[k]
        comments = [t for t in trailing if t.strip()]
        if comments:
            res += [''] + comments
        res.append('')
    open(out, 'w').write('\n'.join(res).rstrip('\n') + '\n')
    print('%d lyric lines into %d sections' % (sum(len(v) for v in lanes.values()), sum(1 for v in lanes.values() if v)))


if __name__ == '__main__':
    main()

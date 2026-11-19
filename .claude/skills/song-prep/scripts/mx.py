"""Dump a MusicXML score as per-measure events: python3 -I mx.py score.xml > out.json"""
import json, sys
import xml.etree.ElementTree as ET

STEPS = {'C': 0, 'D': 2, 'E': 4, 'F': 5, 'G': 7, 'A': 9, 'B': 11}
KIND = {'major': '', 'minor': 'm', 'dominant': '7', 'major-seventh': 'maj7', 'minor-seventh': 'm7',
        'diminished': 'dim', 'diminished-seventh': 'dim7', 'half-diminished': 'm7b5', 'augmented': 'aug',
        'suspended-fourth': 'sus4', 'suspended-second': 'sus2', 'major-sixth': '6', 'minor-sixth': 'm6',
        'dominant-ninth': '9', 'major-ninth': 'maj9', 'minor-ninth': 'm9', 'dominant-11th': '11',
        'dominant-13th': '13', 'minor-11th': 'm11', 'major-minor': 'm(maj7)', 'power': '5', 'none': '', 'other': '?'}


def alter_s(a):
    return {-2: 'bb', -1: 'b', 0: '', 1: '#', 2: '##'}.get(int(float(a or 0)), '?')


def harmony_name(h):
    r = h.find('root')
    name = r.findtext('root-step') + alter_s(r.findtext('root-alter'))
    k = h.find('kind')
    txt = k.get('text') if k is not None else None
    kind = k.text if k is not None else ''
    name += txt if txt is not None else KIND.get(kind, kind)
    for d in h.findall('degree'):
        name += '(' + (d.findtext('degree-type') or '')[:3] + alter_s(d.findtext('degree-alter')) + (d.findtext('degree-value') or '') + ')'
    b = h.find('bass')
    if b is not None:
        name += '/' + b.findtext('bass-step') + alter_s(b.findtext('bass-alter'))
    return name


def main(path):
    root = ET.parse(path).getroot()
    names = {p.get('id'): p.findtext('part-name') for p in root.iter('score-part')}
    out = {}
    for part in root.findall('part'):
        div = 1
        ms = []
        for m in part.findall('measure'):
            t = 0
            last_start = 0
            ev = {'n': m.get('number'), 'notes': [], 'harm': [], 'words': [], 'key': None, 'time': None, 'tempo': None,
                  'bar': [], 'len': 0}
            for el in m:
                tag = el.tag
                if tag == 'attributes':
                    if el.findtext('divisions'):
                        div = int(el.findtext('divisions'))
                    if el.find('key') is not None:
                        ev['key'] = int(el.find('key').findtext('fifths'))
                    if el.find('time') is not None:
                        ev['time'] = el.find('time').findtext('beats') + '/' + el.find('time').findtext('beat-type')
                elif tag == 'harmony':
                    off = int(el.findtext('offset') or 0)
                    ev['harm'].append([round((t + off) / div, 3), harmony_name(el)])
                elif tag == 'direction':
                    for w in el.iter('words'):
                        if (w.text or '').strip():
                            ev['words'].append(w.text.strip())
                    for w in el.iter('rehearsal'):
                        ev['words'].append('[' + (w.text or '') + ']')
                    s = el.find('sound')
                    if s is not None and s.get('tempo'):
                        ev['tempo'] = float(s.get('tempo'))
                elif tag == 'sound' and el.get('tempo'):
                    ev['tempo'] = float(el.get('tempo'))
                elif tag == 'backup':
                    t -= int(el.findtext('duration'))
                elif tag == 'forward':
                    t += int(el.findtext('duration'))
                elif tag == 'barline':
                    rep = el.find('repeat')
                    end = el.find('ending')
                    if rep is not None:
                        ev['bar'].append('repeat-' + rep.get('direction'))
                    if end is not None:
                        ev['bar'].append('ending-' + (end.get('number') or '') + '-' + (end.get('type') or ''))
                elif tag == 'note':
                    dur = int(el.findtext('duration') or 0)
                    chord = el.find('chord') is not None
                    start = last_start if chord else t
                    p = el.find('pitch')
                    u = el.find('unpitched')
                    if p is not None:
                        midi = 12 * (int(p.findtext('octave')) + 1) + STEPS[p.findtext('step')] + int(float(p.findtext('alter') or 0))
                        name = p.findtext('step') + alter_s(p.findtext('alter')) + p.findtext('octave')
                    elif u is not None:
                        midi = None
                        name = 'x' + (u.findtext('display-step') or '') + (u.findtext('display-octave') or '')
                    else:
                        midi = None
                        name = 'r'
                    tie = [x.get('type') for x in el.findall('tie')]
                    lyr = el.findtext('lyric/text')
                    ev['notes'].append({'on': round(start / div, 3), 'd': round(dur / div, 3), 'p': name, 'midi': midi,
                                        'v': el.findtext('voice'), 'tie': tie, 'grace': el.find('grace') is not None,
                                        'lyr': lyr, 'inst': (el.find('instrument').get('id') if el.find('instrument') is not None else None)})
                    if not chord:
                        last_start = t
                        if el.find('grace') is None:
                            t += dur
                ev['len'] = max(ev['len'], round(t / div, 3))
            ms.append(ev)
        out[names[part.get('id')]] = ms
    json.dump(out, sys.stdout)


main(sys.argv[1])

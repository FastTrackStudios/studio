"""Prepare an isolated REAPER render copy while retaining session routing."""
import argparse
import re
from pathlib import Path


def render_copy(project, destination, render_dir, args):
    def absolute_media(match):
        source = (project.parent / match[2]).resolve(strict=True)
        if not source.is_file() or '"' in str(source):
            raise ValueError(f"invalid media source: {source}")
        return f'{match[1]}"{source}"'

    text = re.sub(r'(?m)^( *FILE )"([^"]+)"$', absolute_media, project.read_text())
    bounds, master_gain = args.bounds, args.master_gain
    settings = {
        "RENDER_FILE": f'"{render_dir}"',
        "RENDER_PATTERN": f'"{destination.stem}"',
        "RENDER_FMT": "44100 2 0",
        "RENDER_RANGE": f"0 {bounds[0]} {bounds[1]} 0 0" if bounds else "1 0 0 0 1000",
        "RENDER_STEMS": "0",
    }
    for key, value in settings.items():
        text, count = re.subn(rf"(?m)^  {key} .*$", lambda _: f"  {key} {value}", text)
        if count != 1:
            raise ValueError(f"expected exactly one {key} value in the project")
    if master_gain != 1:
        text, count = re.subn(r"(?m)^  MASTER_VOLUME (\S+)( .*)$",
                              lambda m: f"  MASTER_VOLUME {float(m[1]) * master_gain}{m[2]}", text)
        if count != 1:
            raise ValueError("expected one MASTER_VOLUME value")
    if args.solo_track or args.mute_trigger_tracks:
        text = set_track_states(text, args.solo_track, args.mute_trigger_tracks)
    with destination.open("x", encoding="utf-8") as output:
        output.write(text)


def set_track_states(text, solo_name, mute_triggers):
    tracks = re.split(r"(?m)^  <TRACK ", text)
    solo_count = mute_count = 0
    for index, track in enumerate(tracks[1:], 1):
        match = re.search(r"(?m)^    NAME (.*)$", track)
        name = match[1].strip('"') if match else ""
        if solo_name and name == solo_name:
            tracks[index], count = re.subn(r"(?m)^    MUTESOLO 0 0 0$",
                                          "    MUTESOLO 0 2 0", track)
            solo_count += count
        elif mute_triggers and re.fullmatch(r"(?:T[1-4] )?Trig", name, re.IGNORECASE):
            tracks[index], count = re.subn(r"(?m)^    MUTESOLO [01] 0 0$",
                                          "    MUTESOLO 1 0 0", track)
            if count != 1:
                raise ValueError(f"unsupported mute/solo state for trigger track {name}")
            mute_count += count
    if solo_name and solo_count != 1:
        raise ValueError(f"expected one unmuted, unsoloed track named {solo_name}")
    if mute_triggers and not mute_count:
        raise ValueError("no trigger tracks could be muted")
    return "  <TRACK ".join(tracks)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("project", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("render_dir", type=Path)
    parser.add_argument("--solo-track")
    parser.add_argument("--mute-trigger-tracks", action="store_true")
    parser.add_argument("--bounds", nargs=2, type=float, metavar=("START", "END"))
    parser.add_argument("--master-gain", type=float, default=1)
    args = parser.parse_args()
    if args.bounds and not 0 <= args.bounds[0] < args.bounds[1]:
        parser.error("expected nonnegative start and later end")
    if not 0 < args.master_gain <= 1:
        parser.error("master gain must be greater than zero and at most one")
    render_copy(args.project.resolve(strict=True), args.destination,
                args.render_dir.resolve(strict=True), args)

#!/usr/bin/env python3
"""Seam 3 on iOS (studio#689, Testing Decisions): Studio, built for the
simulator with the engine linked into the app, opens a session and plays
it with the click.

Checked only from outside, the way Seam 1 checks a desktop: the app is
launched with its MCP door open (`FTS_MCP=1`; a phone build serves none
otherwise), and the engine linked into it is driven through the app's
Action List (`DAW_RUN`, what an agent runs), its answers read back. The
simulator shares the host's loopback, so the door the app publishes in its
container is reached from here.

    python3 -I apps/studio/ios/sim-test.py --app target/dx/Studio/debug/ios/studio-app.app

Crashes and the app's own spans reach the telemetry: the app is pointed at
a collector here (a small OTLP receiver on loopback); its `studio.boot`
span must arrive, and then, launched again with `--crash-test`, its
`crash` span (studio-otel's crash hook).

The song is made here (a short drum loop at 120 bpm, written into the
app's container), so no media is kept in git. The simulator is one of its
own, in a device set under --set (default: a fresh folder in $TMPDIR),
deleted afterwards unless --keep. On failure the app's output and a
screenshot are left in --out.
"""

import argparse
import http.server
import json
import math
import os
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import wave

RATE = 48_000
SECONDS = 8.0
# How long the app has to come up (a debug build, a cold simulator).
UP_WAIT = 240
OPEN_WAIT = 120


def log(msg):
    print(f"sim-test: {msg}", file=sys.stderr, flush=True)


class Failed(Exception):
    pass


def simctl(dev_set, *args, check=True, capture=True):
    cmd = ["xcrun", "simctl", "--set", dev_set, *args]
    out = subprocess.run(cmd, check=False, text=True, capture_output=capture)
    if check and out.returncode != 0:
        raise Failed(f"{' '.join(cmd)}: {out.stderr.strip() or out.stdout.strip()}")
    return out.stdout if capture else ""


def newest(items, key):
    return sorted(items, key=key)[-1] if items else None


def make_device(dev_set):
    """A fresh iPhone on the newest iOS runtime this Xcode has."""
    listing = json.loads(simctl(dev_set, "list", "-j", "runtimes", "devicetypes"))
    runtimes = [r for r in listing["runtimes"] if r.get("isAvailable") and r["platform"] == "iOS"]
    runtime = newest(runtimes, lambda r: [int(p) for p in r["version"].split(".")])
    if runtime is None:
        raise Failed("no iOS simulator runtime is installed")
    phones = [t for t in listing["devicetypes"] if t["name"].startswith("iPhone")]
    names = {t["identifier"] for t in runtime.get("supportedDeviceTypes", [])}
    phones = [t for t in phones if not names or t["identifier"] in names]
    phone = newest(phones, lambda t: t.get("minRuntimeVersion", 0))
    if phone is None:
        raise Failed(f"no iPhone runs on {runtime['name']}")
    udid = simctl(dev_set, "create", "studio-seam3", phone["identifier"], runtime["identifier"]).strip()
    log(f"simulator {udid}: {phone['name']}, {runtime['name']}")
    return udid


class Collector:
    """An OTLP/HTTP traces receiver on loopback: every body posted to it
    kept, protobuf or JSON as the exporter sends it."""

    def __init__(self):
        bodies = self.bodies = []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                n = int(self.headers.get("Content-Length") or 0)
                bodies.append(self.rfile.read(n))
                self.send_response(200)
                self.send_header("Content-Type", "application/x-protobuf")
                self.send_header("Content-Length", "0")
                self.end_headers()

            def log_message(self, *_):
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}/v1/traces"

    def seen(self, *needles):
        return any(all(n in b for n in needles) for b in self.bodies)

    def close(self):
        self.server.shutdown()


# What `Studio --crash-test` panics with (src/telemetry.rs).
CRASH_MESSAGE = "studio --crash-test: a crash on purpose"


def kick(t):
    f = 50.0 + 60.0 * math.exp(-t * 30.0)
    return 0.8 * math.exp(-t * 12.0) * math.sin(math.tau * f * t)


def write_wav(path, first, every):
    n = int(SECONDS * RATE)
    frames = bytearray()
    for i in range(n):
        t = i / RATE
        s = 0.0 if t < first else kick((t - first) % every)
        frames += struct.pack("<h", int(max(-1.0, min(1.0, s)) * 32767))
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(bytes(frames))


def stage_song(folder):
    """A song: a REAPER project at 120 bpm in 4/4 with sections and one
    drum track (the engine organizes it and generates its click on open)."""
    media = os.path.join(folder, "Media")
    os.makedirs(media, exist_ok=True)
    write_wav(os.path.join(media, "kick.wav"), 0.0, 0.5)
    sections = [("IN", 0.0, 2.0), ("CH 1", 2.0, 6.0), ("OUT", 6.0, SECONDS)]
    rpp = ['<REAPER_PROJECT 0.1 "7.0/test" 1700000000', "  SAMPLERATE 48000 0 0", "  TEMPO 120 4 4 0"]
    for n, (name, start, end) in enumerate(sections, 1):
        rpp.append(f'  MARKER {n} {start} "{name}" 1 0 1 B {{00000000-0000-0000-0000-0000000000{n:02}}} 0')
        rpp.append(f'  MARKER {n} {end} "" 1')
    rpp += [
        "  <TRACK",
        '    NAME "Kick"',
        "    NCHAN 2",
        "    <ITEM",
        "      POSITION 0",
        f"      LENGTH {SECONDS}",
        "      NAME kick.wav",
        "      <SOURCE WAVE",
        '        FILE "Media/kick.wav"',
        "      >",
        "    >",
        "  >",
        ">",
    ]
    path = os.path.join(folder, "seam3.RPP")
    with open(path, "w") as f:
        f.write("\n".join(rpp) + "\n")
    return path


def find_door(data):
    """The app's MCP discovery file (`<config>/fts/<name>-mcp.json`),
    wherever the app's config folder is in its container."""
    for root, _dirs, files in os.walk(data):
        for name in files:
            if name.endswith("-mcp.json") and os.path.basename(root) == "fts":
                with open(os.path.join(root, name)) as f:
                    door = json.load(f)
                if door.get("url"):
                    return door
    return None


class Mcp:
    def __init__(self, url, token):
        self.url = url
        self.token = token

    def run(self, action, args=None, timeout=60):
        body = json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "run", "arguments": {"id": action, "args": args if args is not None else {}}},
        }).encode()
        req = urllib.request.Request(self.url, data=body, method="POST")
        req.add_header("Content-Type", "application/json")
        req.add_header("Accept", "application/json, text/event-stream")
        if self.token:
            req.add_header("Authorization", f"Bearer {self.token}")
        with urllib.request.urlopen(req, timeout=timeout) as reply:
            text = reply.read().decode()
        # An event stream's answer is its last `data:` line.
        lines = [line[5:].strip() for line in text.splitlines() if line.startswith("data:")]
        answer = json.loads(lines[-1] if lines else text)
        if "error" in answer:
            raise Failed(f"{action}: {answer['error']}")
        result = answer["result"]
        said = (result.get("content") or [{}])[0].get("text", "")
        if result.get("isError"):
            raise Failed(f"{action}: {said}")
        if "structuredContent" in result:
            return result["structuredContent"]
        try:
            return json.loads(said)
        except ValueError:
            return said

    def engine(self, action, args=None, timeout=60):
        """An action of the engine linked into the app, through its daw
        extension (`DAW_RUN`)."""
        out = self.run("DAW_RUN", {"id": action, "args": args if args is not None else {}}, timeout)
        if isinstance(out, str):
            try:
                return json.loads(out)
            except ValueError:
                return out
        return out


def wait_for(what, seconds, probe):
    began = time.monotonic()
    last = None
    while time.monotonic() - began < seconds:
        try:
            got = probe()
            if got:
                return got
        except (Failed, OSError, urllib.error.URLError, ValueError) as e:
            last = e
        time.sleep(1)
    raise Failed(f"{what} not within {seconds} s" + (f" (last: {last})" if last else ""))


def opened(mcp):
    state = mcp.engine("SESSION_OPENING")
    if isinstance(state, dict) and state.get("state") == "failed":
        raise Failed(f"the song did not open: {state.get('why')}")
    if isinstance(state, dict) and state.get("state") == "opening":
        return None
    transport = mcp.engine("SESSION_TRANSPORT")
    return transport if isinstance(transport, dict) and transport.get("song") else None


def click_track(tracks):
    rows = tracks if isinstance(tracks, list) else (tracks or {}).get("tracks", [])
    for t in rows:
        name = str(t.get("name", "")).strip().lower()
        if name == "click":
            return t
    return None


def check(mcp, song):
    log("opening the song in the engine")
    mcp.engine("SESSION_OPEN", {"path": song}, timeout=OPEN_WAIT)
    wait_for("the song open", OPEN_WAIT, lambda: opened(mcp))
    tracks = wait_for("the click track", 30, lambda: click_track(mcp.engine("SESSION_TRACKS")))
    if tracks.get("mute"):
        raise Failed(f"the click is muted: {tracks}")
    if not tracks.get("items"):
        raise Failed(f"the click has no clicks: {tracks}")
    log(f"click: {tracks.get('path') or tracks.get('name')}, {tracks.get('items')} items")
    mcp.engine("SESSION_PLAY_FROM", {"at": 0.0})
    first = wait_for(
        "the transport playing",
        20,
        lambda: (lambda t: t if t.get("playing") else None)(mcp.engine("SESSION_TRANSPORT")),
    )
    time.sleep(1.5)
    later = mcp.engine("SESSION_TRANSPORT")
    a, b = first.get("position") or 0.0, later.get("position") or 0.0
    if not later.get("playing") or b <= a:
        raise Failed(f"the transport is not moving: {first} then {later}")
    log(f"playing: {a:.2f} s -> {b:.2f} s")
    mcp.engine("SESSION_PLAY_PAUSE")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--app", required=True, help="the Studio .app built for the simulator")
    p.add_argument("--set", help="the simulator device set (default: a fresh folder in $TMPDIR)")
    p.add_argument("--out", default=None, help="where the app's output and a screenshot go")
    p.add_argument("--keep", action="store_true", help="leave the simulator and its set")
    a = p.parse_args()

    app = os.path.abspath(a.app)
    if not os.path.isdir(app):
        log(f"no app at {app}: build it first (apps/studio/ios/build.sh sim)")
        return 1
    with open(os.path.join(app, "Info.plist"), "rb") as f:
        bundle = plistlib.load(f)["CFBundleIdentifier"]
    dev_set = a.set or tempfile.mkdtemp(prefix="studio-sim-")
    out = a.out or tempfile.mkdtemp(prefix="studio-sim-out-")
    os.makedirs(dev_set, exist_ok=True)
    os.makedirs(out, exist_ok=True)
    app_log = os.path.join(out, "studio.log")
    udid = None
    ok = False
    collector = Collector()
    try:
        udid = make_device(dev_set)
        simctl(dev_set, "boot", udid)
        simctl(dev_set, "bootstatus", udid, "-b")
        simctl(dev_set, "install", udid, app)
        data = simctl(dev_set, "get_app_container", udid, bundle, "data").strip()
        song = stage_song(os.path.join(data, "Documents", "seam3"))
        env = dict(
            os.environ,
            SIMCTL_CHILD_FTS_MCP="1",
            SIMCTL_CHILD_OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=collector.url,
        )
        subprocess.run(
            ["xcrun", "simctl", "--set", dev_set, "launch", "--terminate-running-process",
             f"--stdout={app_log}", f"--stderr={app_log}", udid, bundle],
            check=True, env=env, capture_output=True, text=True,
        )
        log(f"launched {bundle}; waiting for its MCP door")
        door = wait_for("the app's MCP door", UP_WAIT, lambda: find_door(data))
        mcp = Mcp(door["url"], door.get("token"))
        wait_for("the engine in the app", UP_WAIT, lambda: mcp.engine("SESSION_TRANSPORT") is not None)
        check(mcp, song)
        wait_for("the app's studio.boot span at the collector", 60, lambda: collector.seen(b"studio.boot"))
        log("crashing it on purpose (--crash-test)")
        subprocess.run(
            ["xcrun", "simctl", "--set", dev_set, "launch", "--terminate-running-process",
             f"--stdout={app_log}", f"--stderr={app_log}", udid, bundle, "--crash-test"],
            check=False, env=env, capture_output=True, text=True,
        )
        wait_for(
            "the crash span at the collector",
            60,
            lambda: collector.seen(b"crash", CRASH_MESSAGE.encode()),
        )
        ok = True
        log("PASS: the session opened and played with the click; the app's spans and its crash reached the telemetry")
    except (Failed, subprocess.CalledProcessError, urllib.error.URLError, OSError) as e:
        log(f"FAIL: {e}")
    finally:
        collector.close()
        if udid:
            if not ok:
                simctl(dev_set, "io", udid, "screenshot", os.path.join(out, "screen.png"), check=False)
                log(f"the app's output and a screenshot: {out}")
            if not a.keep:
                simctl(dev_set, "shutdown", udid, check=False)
                simctl(dev_set, "delete", udid, check=False)
        if not a.keep and not a.set:
            shutil.rmtree(dev_set, ignore_errors=True)
        if ok and not a.out:
            shutil.rmtree(out, ignore_errors=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Seam 2 with an iPhone (studio#689, studio#712): Studio in the iOS
simulator joins a room from its link and follows a desktop leader.

The leader is a desktop engine (`daw-extension --engine`, sandboxed as
the room harness sandboxes one, `tests/room`), the member is the app in
the simulator with the engine linked into it. Both are read only through
MCP: the leader's own door, and the app's (`FTS_MCP=1`), whose engine is
reached through `DAW_RUN` as an agent reaches it.

    python3 -I apps/studio/ios/sim-room-test.py \\
        --app target/dx/Studio/debug/ios/studio-app.app \\
        --engine target/aarch64-apple-darwin/desktop-dev/daw-extension

What it does, in order:
1. Stages a Studio folder on the host: three short drum songs and two
   setlists of them. The leader's Rooms folder is in it.
2. The leader makes a room (`ROOM_CREATE`), leads it (`ROOM_LEAD`) and
   loads the first setlist (`ROOM_LOAD`).
3. The Studio folder is copied into the app's own Studio folder in its
   container, as Drive carries an organization's folder to each device:
   the room's file (where its leader is) and the songs it loads.
4. The room's link is opened on the simulator, in the app's scheme
   (`simctl openurl ... fasttrackstudio://join/<room>`): the link opens
   Studio and Studio joins. Nothing on the phone is told to join any
   other way. iOS 27 asks first ("Open in “Studio”?"), as for any link
   from outside an app; the test taps Open through idb, so it needs
   `idb_companion` (Homebrew) and the `idb` client (pip's fb-idb).
5. Checked from outside: the phone is a member and opens the leader's
   setlist; the leader sees it in the room; it follows the leader's song,
   playhead and stop; and when the leader loads the second setlist the
   phone moves to it on the same connection (`attaches` stays 1).
6. Studio is launched again with the link (`--join <link>`), cold: it
   joins before its engine is up, and opens what the leader has loaded.

How long the phone took from the link to the leader's setlist open is
in `room.json` (`join_s`).

On failure the app's and the leader's output and a screenshot are left
in --out; the checks go to `<out>/room.json` either way.
"""

import argparse
import importlib.util
import json
import os
import plistlib
import shutil
import socket
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("sim_test", os.path.join(HERE, "sim-test.py"))
sim = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sim)

Failed = sim.Failed
log = sim.log

# The two setlists the leader loads, one after the other, by their songs.
FIRST = ["One", "Two"]
SECOND = ["Three", "One"]
TEMPO = {"One": 120, "Two": 100, "Three": 90}
# How long the phone has to follow what the leader did.
FOLLOW_WAIT = 30
# How far apart the two playheads may read: the follower's tolerance, two
# MCP reads (one through the app's DAW_RUN), and the transport's poll.
PLAYHEAD_TOLERANCE = 0.5
ENGINE_WAIT = 90
# How long iOS has to show its "Open in “Studio”?" alert for the link.
LINK_ALERT_WAIT = 20


def stage_song(folder, name, bpm):
    """A song: a REAPER project with one drum track, its kick on the beat."""
    media = os.path.join(folder, "Media")
    os.makedirs(media, exist_ok=True)
    beat = 60.0 / bpm
    sim.write_wav(os.path.join(media, "kick.wav"), 0.0, beat)
    seconds = sim.SECONDS
    rpp = [
        '<REAPER_PROJECT 0.1 "7.0/test" 1700000000',
        "  SAMPLERATE 48000 0 0",
        f"  TEMPO {bpm} 4 4 0",
        '  MARKER 1 0 "IN" 1 0 1 B {00000000-0000-0000-0000-000000000001} 0',
        f'  MARKER 1 {seconds / 2} "" 1',
        f'  MARKER 2 {seconds / 2} "CH 1" 1 0 1 B {{00000000-0000-0000-0000-000000000002}} 0',
        f'  MARKER 2 {seconds} "" 1',
        "  <TRACK",
        '    NAME "Kick"',
        "    NCHAN 2",
        "    <ITEM",
        "      POSITION 0",
        f"      LENGTH {seconds}",
        "      NAME kick.wav",
        "      <SOURCE WAVE",
        '        FILE "Media/kick.wav"',
        "      >",
        "    >",
        "  >",
        ">",
    ]
    with open(os.path.join(folder, f"{name}.RPP"), "w") as f:
        f.write("\n".join(rpp) + "\n")


def stage(studio):
    """The Studio folder: the songs in `sessions/`, the two setlists in
    `Setlists/`. Returns the setlists' paths."""
    for name in sorted(set(FIRST + SECOND)):
        stage_song(os.path.join(studio, "sessions", name), name, TEMPO[name])
    setlists = os.path.join(studio, "Setlists")
    os.makedirs(setlists, exist_ok=True)
    paths = []
    for file, names in (("first.setlist", FIRST), ("second.setlist", SECOND)):
        path = os.path.join(setlists, file)
        with open(path, "w") as f:
            f.write("".join(f"../sessions/{n}\n" for n in names))
        paths.append(path)
    return paths


class Leader:
    """A private desktop engine, sandboxed as the room harness's
    (`tests/unbreakable/engine.rs`), its door found in its registry file.
    Stopped on its control socket, and killed by its PID if it lingers."""

    def __init__(self, binary, rooms, out):
        self.sandbox = tempfile.mkdtemp(prefix="ube-", dir="/tmp")
        dirs = {k: os.path.join(self.sandbox, k) for k in ("run", "data", "engine", "engines")}
        for d in dirs.values():
            os.makedirs(d, exist_ok=True)
        self.engine_dir = dirs["engine"]
        self.log = os.path.join(out, "leader.log")
        env = {k: v for k, v in os.environ.items() if not k.startswith("OTEL_EXPORTER_OTLP")}
        for var in ("FTS_MCP_TOKEN", "FTS_MCP", "FTS_DAW_OPEN", "FTS_ENGINE_SILENT", "STUDIO_ENGINE", "STUDIO_EXT_DEV"):
            env.pop(var, None)
        env.update({
            "FTS_DAW_ENGINE_DIR": dirs["engine"],
            "XDG_RUNTIME_DIR": dirs["run"],
            "XDG_DATA_HOME": dirs["data"],
            "FTS_ENGINES_DIR": dirs["engines"],
            "STUDIO_INSTANCE": f"room-ios-leader-{os.getpid()}",
            "FTS_MCP_PORT": "0",
            "FTS_MCP_PRIVATE": "1",
            "FTS_AUDIO_DEVICE": "null",
            "FTS_RENDER_WORKER": "process",
            "FTS_ROOMS_DIR": rooms,
        })
        out_file = open(self.log, "w")
        self.child = subprocess.Popen(
            [binary, "--engine"], env=env, stdin=subprocess.DEVNULL, stdout=out_file, stderr=out_file
        )
        self.registry = os.path.join(dirs["engines"], f"studio-{self.child.pid}.json")
        self.mcp = sim.wait_for("the leader's MCP door", ENGINE_WAIT, self._door)

    def _door(self):
        if self.child.poll() is not None:
            raise Failed(f"the leader's engine exited ({self.child.returncode}): see {self.log}")
        try:
            with open(self.registry) as f:
                url = json.load(f).get("mcp")
        except (OSError, ValueError):
            return None
        if not url or ":0/" in url:
            return None
        mcp = sim.Mcp(url, None)
        mcp.run("SESSION_TRANSPORT")
        return mcp

    def stop(self):
        if self.child.poll() is None:
            try:
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
                    s.settimeout(5)
                    s.connect(os.path.join(self.engine_dir, "control.sock"))
                    s.sendall(b'{"stop":true}\n')
                    s.recv(4096)
            except OSError:
                pass
            try:
                self.child.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.child.kill()
                self.child.wait()
        shutil.rmtree(self.sandbox, ignore_errors=True)


class Checks:
    def __init__(self):
        self.rows = []
        self.join_s = None

    def add(self, name, ok, said):
        self.rows.append({"name": name, "ok": bool(ok), "said": str(said)})
        log(f"{'ok  ' if ok else 'FAIL'} {name}: {said}")
        return ok

    def ok(self):
        return bool(self.rows) and all(r["ok"] for r in self.rows)


def until(seconds, read, ok):
    """`read()` until `ok` holds of it, up to `seconds`: (held, last)."""
    began = time.monotonic()
    while True:
        try:
            got = read()
        except (Failed, OSError, ValueError) as e:
            got = {"error": str(e)}
        if ok(got):
            return True, got
        if time.monotonic() - began > seconds:
            return False, got
        time.sleep(0.25)


def open_titles(run):
    """The songs an engine has open, by title, once its open has settled."""
    opening = run("SESSION_OPENING")
    if not isinstance(opening, dict) or opening.get("state") != "idle":
        return {"opening": opening}
    rows = run("SESSION_SONGS")
    rows = rows if isinstance(rows, list) else []
    if not rows or any(r.get("state") != "open" for r in rows):
        return {"songs": rows}
    return [r.get("title") for r in rows]


def current(run):
    rows = run("SESSION_SONGS")
    for r in rows if isinstance(rows, list) else []:
        if r.get("current"):
            return r.get("number")
    return None


def play_from(run, at):
    if run("SESSION_TRANSPORT").get("playing"):
        run("SESSION_STOP")
    run("SESSION_PLAY_FROM", {"at": at})
    ok, got = until(10, lambda: run("SESSION_TRANSPORT"),
                    lambda t: t.get("playing") is True and (t.get("position") or 0) >= at - 0.01)
    if not ok:
        raise Failed(f"asked to play from {at} s, it did not: {got}")


def room(leader, phone, setlists, studio, home, checks, made_room):
    lead = leader.run
    member = phone.engine
    first, second = setlists

    made = lead("ROOM_CREATE", {"name": "Adult Jam", "org": "rockstars"})
    link = made.get("link", "")
    made_room.append(made.get("id", ""))
    if not checks.add("made", link.startswith("https://fasttrackstudio.app/join/adult-jam-"), made):
        return
    lead("ROOM_LEAD", {"room": link, "name": "Cody"})
    lead("ROOM_LOAD", {"path": first})
    ok, leader_first = until(FOLLOW_WAIT, lambda: open_titles(lead),
                             lambda v: isinstance(v, list) and len(v) == len(FIRST))
    if not checks.add("leader opens the first setlist", ok, leader_first):
        return

    # Drive's part: the organization's folder on the phone, with the room's
    # file saying where its leader is now.
    shutil.copytree(studio, home, dirs_exist_ok=True)

    # The link, opened on the phone in the app's scheme: nothing else.
    in_app = "fasttrackstudio://join/" + link.rsplit("/", 1)[-1]
    began = time.monotonic()
    sim.simctl(phone.dev_set, "openurl", phone.udid, in_app)
    # iOS 27 asks first ("Open in “Studio”?") when a link comes from no
    # app, as simctl's does; a person taps Open, and so does this.
    phone.alert.tap("Open", LINK_ALERT_WAIT)
    ok, got = until(FOLLOW_WAIT, lambda: member("ROOM"),
                    lambda v: isinstance(v, dict) and v.get("role") == "member")
    if not checks.add("the link joins the phone to the room", ok, got):
        return
    ok, got = until(FOLLOW_WAIT, lambda: open_titles(member), lambda v: v == leader_first)
    checks.join_s = round(time.monotonic() - began, 2)
    checks.add("the phone opens the leader's setlist", ok,
               f"leader {leader_first}, phone {got}, {checks.join_s} s from the link")
    # The room shows the phone by the device's own word (room_link::member_name).
    people = sorted(["Cody", "iPhone"])
    ok, got = until(FOLLOW_WAIT, lambda: lead("ROOM"),
                    lambda v: isinstance(v, dict) and sorted(v.get("people") or []) == people)
    checks.add("the leader sees the phone in the room", ok, (got or {}).get("people", got))

    lead("SESSION_SONG", {"to": "2"})
    ok, got = until(FOLLOW_WAIT, lambda: current(member), lambda v: v == 2)
    checks.add("the phone follows the song", ok, f"phone on song {got}")

    try:
        play_from(lead, 1.0)
    except Failed as e:
        checks.add("the phone follows the playhead", False, e)
        return
    playing, _ = until(FOLLOW_WAIT, lambda: member("SESSION_TRANSPORT"), lambda v: v.get("playing") is True)
    time.sleep(1.0)
    lt, mt = lead("SESSION_TRANSPORT"), member("SESSION_TRANSPORT")
    apart = abs((lt.get("position") or float("nan")) - (mt.get("position") or float("nan")))
    checks.add("the phone follows the playhead",
               playing and mt.get("playing") is True and apart < PLAYHEAD_TOLERANCE,
               f"leader {lt}, phone {mt}: {apart:.3f} s apart")
    lead("SESSION_STOP")
    ok, got = until(FOLLOW_WAIT, lambda: member("SESSION_TRANSPORT"), lambda v: v.get("playing") is False)
    checks.add("the phone stops with the leader", ok, got)

    before = member("ROOM")
    lead("ROOM_LOAD", {"path": second})
    ok, leader_second = until(FOLLOW_WAIT, lambda: open_titles(lead),
                              lambda v: isinstance(v, list) and len(v) == len(SECOND) and v != leader_first)
    if not checks.add("leader opens the second setlist", ok, leader_second):
        return
    ok, got = until(FOLLOW_WAIT, lambda: open_titles(member), lambda v: v == leader_second)
    checks.add("the phone moves to the second setlist", ok, f"leader {leader_second}, phone {got}")
    after = member("ROOM")
    checks.add("without reconnecting",
               before.get("attaches") == 1 and after.get("attaches") == 1 and after.get("load") == 2,
               f"attaches {before.get('attaches')} then {after.get('attaches')}, load {after.get('load')}")

    # Launched by the link, cold: the join is asked for before the engine
    # in the app is up.
    phone.relaunch(["--join", link])
    ok, got = until(sim.UP_WAIT, lambda: phone.engine("ROOM"),
                    lambda v: isinstance(v, dict) and v.get("role") == "member")
    if not checks.add("launched with the link, the phone joins", ok, got):
        return
    ok, got = until(FOLLOW_WAIT, lambda: open_titles(phone.engine), lambda v: v == leader_second)
    checks.add("and opens what the leader has loaded", ok, f"leader {leader_second}, phone {got}")


class Alert:
    """The system's alerts on the simulator, answered through idb (simctl
    cannot tap): `idb_companion` serves the device, the `idb` client
    (pip's fb-idb) reads its screen and taps."""

    def __init__(self, dev_set, udid, idb, companion, out):
        self.idb = idb
        # Its gRPC and debug ports, free ones: other runners on the Mac
        # may run a companion at the same time.
        with socket.socket() as a, socket.socket() as b:
            a.bind(("127.0.0.1", 0))
            b.bind(("127.0.0.1", 0))
            port, debug = a.getsockname()[1], b.getsockname()[1]
        self.at = f"localhost:{port}"
        self.log = open(os.path.join(out, "idb_companion.log"), "w")
        self.child = subprocess.Popen(
            [companion, "--udid", udid, "--device-set-path", dev_set,
             "--grpc-port", str(port), "--debug-port", str(debug)],
            stdout=self.log, stderr=subprocess.STDOUT)

    def _idb(self, *args):
        return subprocess.run([self.idb, "--companion", self.at, *args],
                              capture_output=True, text=True, timeout=30)

    def tap(self, label, wait):
        """Tap the button called `label` once it is on screen."""
        deadline = time.monotonic() + wait
        while time.monotonic() < deadline:
            try:
                items = json.loads(self._idb("ui", "describe-all", "--json").stdout or "[]")
            except (json.JSONDecodeError, subprocess.TimeoutExpired):
                items = []
            for item in items:
                if item.get("type") == "Button" and item.get("AXLabel") == label:
                    f = item["frame"]
                    x, y = int(f["x"] + f["width"] / 2), int(f["y"] + f["height"] / 2)
                    self._idb("ui", "tap", str(x), str(y)).check_returncode()
                    log(f"tapped the system's {label!r} at {x},{y}")
                    return
            time.sleep(0.5)
        raise Failed(f"no {label!r} button on screen in {wait} s")

    def stop(self):
        self.child.terminate()
        try:
            self.child.wait(10)
        except subprocess.TimeoutExpired:
            self.child.kill()
        self.log.close()


def idb_tools(idb, companion):
    """The idb client and companion, or why not."""
    idb = idb or shutil.which("idb")
    companion = companion or shutil.which("idb_companion") or "/opt/homebrew/bin/idb_companion"
    if not idb or not os.access(idb, os.X_OK):
        raise Failed("no idb client (pip install fb-idb; --idb): it taps iOS's \"Open in\" alert for the link")
    if not os.access(companion, os.X_OK):
        raise Failed(f"no idb_companion at {companion} (brew install idb-companion; --idb-companion)")
    return idb, companion


class Phone:
    """The app in the simulator, read through its MCP door."""

    def __init__(self, dev_set, udid, bundle, data, app_log, alert):
        self.dev_set = dev_set
        self.udid = udid
        self.bundle = bundle
        self.data = data
        self.app_log = app_log
        self.alert = alert
        self.mcp = None

    def launch(self, args=()):
        # The door a run before left is not this run's.
        for root, _dirs, files in os.walk(self.data):
            if os.path.basename(root) == "fts":
                for name in files:
                    if name.endswith("-mcp.json"):
                        os.remove(os.path.join(root, name))
        env = dict(os.environ, SIMCTL_CHILD_FTS_MCP="1")
        subprocess.run(
            ["xcrun", "simctl", "--set", self.dev_set, "launch", "--terminate-running-process",
             f"--stdout={self.app_log}", f"--stderr={self.app_log}", self.udid, self.bundle, *args],
            check=True, env=env, capture_output=True, text=True,
        )
        log(f"launched {self.bundle} {' '.join(args)}; waiting for its MCP door")
        door = sim.wait_for("the app's MCP door", sim.UP_WAIT, lambda: sim.find_door(self.data))
        self.mcp = sim.Mcp(door["url"], door.get("token"))

    relaunch = launch

    def engine(self, action, args=None):
        return self.mcp.engine(action, args)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--app", required=True, help="the Studio .app built for the simulator")
    p.add_argument("--engine", required=True, help="the desktop engine (daw-extension) that leads")
    p.add_argument("--set", help="the simulator device set (default: a fresh folder in $TMPDIR)")
    p.add_argument("--out", default=None, help="where the outputs, a screenshot and room.json go")
    p.add_argument("--keep", action="store_true", help="leave the simulator and its set")
    p.add_argument("--idb", help="the idb client (pip's fb-idb; default: `idb` on PATH)")
    p.add_argument("--idb-companion", help="idb_companion (default: on PATH, or Homebrew's)")
    a = p.parse_args()

    app = os.path.abspath(a.app)
    engine = os.path.abspath(a.engine)
    for path, what in ((app, "app"), (engine, "engine")):
        if not os.path.exists(path):
            log(f"no {what} at {path}: build it first")
            return 1
    with open(os.path.join(app, "Info.plist"), "rb") as f:
        bundle = plistlib.load(f)["CFBundleIdentifier"]
    dev_set = a.set or tempfile.mkdtemp(prefix="studio-sim-")
    out = a.out or tempfile.mkdtemp(prefix="studio-sim-room-")
    os.makedirs(dev_set, exist_ok=True)
    os.makedirs(out, exist_ok=True)
    work = tempfile.mkdtemp(prefix="studio-room-")
    studio = os.path.join(work, "Studio")
    app_log = os.path.join(out, "studio.log")
    udid = None
    leader = None
    alert = None
    checks = Checks()
    made_room = []
    try:
        idb, companion = idb_tools(a.idb, a.idb_companion)
        setlists = stage(studio)
        leader = Leader(engine, os.path.join(studio, "Rooms"), out)
        udid = sim.make_device(dev_set)
        sim.simctl(dev_set, "boot", udid)
        sim.simctl(dev_set, "bootstatus", udid, "-b")
        sim.simctl(dev_set, "install", udid, app)
        data = sim.simctl(dev_set, "get_app_container", udid, bundle, "data").strip()
        # The app's own Studio folder (src/storage.rs: no Drive mount on a
        # phone), where its engine keeps and finds rooms.
        home = os.path.join(data, "Library", "Application Support", "fasttrackstudio", "Studio")
        alert = Alert(dev_set, udid, idb, companion, out)
        phone = Phone(dev_set, udid, bundle, data, app_log, alert)
        phone.launch()
        sim.wait_for("the engine in the app", sim.UP_WAIT, lambda: phone.engine("SESSION_TRANSPORT") is not None)
        room(leader.mcp, phone, setlists, studio, home, checks, made_room)
    except (Failed, subprocess.CalledProcessError, OSError) as e:
        checks.add("ran to the end", False, e)
    finally:
        ok = checks.ok()
        with open(os.path.join(out, "room.json"), "w") as f:
            json.dump({"test": "room-ios", "ok": ok, "join_s": checks.join_s, "checks": checks.rows}, f, indent=2)
        if alert:
            alert.stop()
        if leader:
            leader.stop()
        # The endpoint key the leader kept for the room (engine-collab's
        # `key_file`, in this machine's data folder): the room was the test's.
        for room_id in filter(None, made_room):
            key = os.path.join(os.path.expanduser("~"), "Library", "Application Support",
                               "fasttrackstudio", "rooms", f"{room_id}.key")
            if os.path.isfile(key):
                os.remove(key)
        if udid:
            if not ok:
                sim.simctl(dev_set, "io", udid, "screenshot", os.path.join(out, "screen.png"), check=False)
            if not a.keep:
                sim.simctl(dev_set, "shutdown", udid, check=False)
                sim.simctl(dev_set, "delete", udid, check=False)
        if not a.keep and not a.set:
            shutil.rmtree(dev_set, ignore_errors=True)
        shutil.rmtree(work, ignore_errors=True)
    log(("PASS: the phone joined the room from its link and followed the leader"
         if ok else f"FAIL: the outputs, a screenshot and room.json are in {out}"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

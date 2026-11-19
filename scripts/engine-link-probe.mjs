// The engine through a share link, measured in a real browser
// (docs/design/engine-through-a-link.md): a headless Firefox over
// WebDriver BiDi opens the link as the web app's guest, waits for the
// engine to open the link's set, plays one song from the playlist and
// reports what the engine's own worklet put out: the time from the press
// to the first frame that was not silence, how many frames were not
// silence and how many were underruns over the seconds it played, where
// the transport got to, and what the browser's processes weigh. Then it
// opens the Arrangement and the Mixer and keeps a screenshot of each.
//
//   SHARE_URL    the setlist page of the link (scripts/local-stack.sh url)
//   SONG         the title of the song to play (default: the first that plays)
//   WIDTH/HEIGHT the viewport (default 390 x 844)
//   SECONDS      how long to let it play (default 20)
//   SCREENSHOTS  a folder for the shots (optional)
//   PRESS        "x,y[;x,y…]" presses in the Mixer view after it is shown,
//                each followed by a shot (to try a mute or a solo by hand)
//
//   AUDIO        pipewire (default): a PipeWire of the probe's own with one
//                null sink, recorded; none: no device (an AudioContext
//                then does not run, and nothing is measured)
//
// One line of JSON on standard output. "Played" here means measured: at
// the worklet's output (the last point in the page before the browser's
// device), and at the device, which is a null sink of a PipeWire the probe
// starts for itself and records (`at_the_sink`: sox's length, peak and
// RMS). The browser runs in a user and mount namespace of its own where
// this machine's audio sockets and its sound cards are hidden under empty
// tmpfs, so nothing can reach the machine's audio; where that cannot be
// done it refuses to run.
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const share = process.env.SHARE_URL;
const shots = process.env.SCREENSHOTS;
const width = Number(process.env.WIDTH || 390);
const height = Number(process.env.HEIGHT || 844);
const seconds = Number(process.env.SECONDS || 20);
const wanted = process.env.SONG || '';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (m) => console.error(`probe: ${m}`);
if (!share) { console.error('probe: SHARE_URL is not set'); process.exit(2); }

function firefox() {
  if (process.env.FIREFOX) return process.env.FIREFOX;
  return execFileSync('sh', ['-c', 'command -v firefox']).toString().trim();
}

// What the browser's processes hold, in MB: all of them, and the largest
// (the tab's content process once the page is up).
function rss(pgid) {
  try {
    const out = execFileSync('ps', ['-o', 'rss=', '-g', String(pgid)]).toString();
    const each = out.split('\n').map((l) => Number(l.trim())).filter((n) => n > 0);
    return { all: Math.round(each.reduce((a, b) => a + b, 0) / 1024), largest: Math.round(Math.max(0, ...each) / 1024), processes: each.length };
  } catch {
    return { all: 0, largest: 0, processes: 0 };
  }
}

// The browser's launcher, run inside a user and mount namespace of its
// own (`unshare -r -m`). Empty tmpfs goes over every place this machine's
// audio lives: the runtime dir (PipeWire's and Pulse's sockets),
// /run/pipewire and /dev/snd (the sound cards themselves). Then, unless
// AUDIO=none, a PipeWire of its own is started there with one null sink,
// which is the browser's only device, and what reaches that sink is
// recorded: the measurement "at the output". Its config and state are in
// the scratch folder, never the person's.
const LAUNCH = `
set -e
mount -t tmpfs none "$XDG_RUNTIME_DIR"
[ ! -d /run/pipewire ] || mount -t tmpfs none /run/pipewire
[ ! -d /dev/snd ] || mount -t tmpfs none /dev/snd
[ -z "$(ls -A "$XDG_RUNTIME_DIR")" ] || { echo "the runtime dir is not empty" >&2; exit 70; }
[ ! -d /dev/snd ] || [ -z "$(ls -A /dev/snd)" ] || { echo "/dev/snd is not hidden" >&2; exit 70; }
[ "$1" = check ] && exit 0
shift
if [ -n "$PROBE_XVFB" ]; then
  # A display of its own, drawn in software: a headless browser offers no
  # WebGL2 here, and the engine's views are WebGL2 canvases.
  [ ! -d /tmp/.X11-unix ] || mount -t tmpfs none /tmp/.X11-unix
  "$PROBE_XVFB" :"$PROBE_DISPLAY" -screen 0 "$PROBE_SCREEN" -nolisten tcp >"$PROBE_SCRATCH/xvfb.log" 2>&1 &
  for i in $(seq 1 50); do [ -S "/tmp/.X11-unix/X$PROBE_DISPLAY" ] && break; sleep 0.1; done
  export DISPLAY=":$PROBE_DISPLAY" LIBGL_ALWAYS_SOFTWARE=1
  unset WAYLAND_DISPLAY MOZ_HEADLESS
fi
if [ "$PROBE_AUDIO" = pipewire ]; then
  export XDG_CONFIG_HOME="$PROBE_SCRATCH/config" XDG_STATE_HOME="$PROBE_SCRATCH/state" XDG_CACHE_HOME="$PROBE_SCRATCH/cache"
  mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
  pipewire >"$PROBE_SCRATCH/pipewire.log" 2>&1 &
  for i in $(seq 1 50); do [ -S "$XDG_RUNTIME_DIR/pipewire-0" ] && break; sleep 0.1; done
  wireplumber >"$PROBE_SCRATCH/wireplumber.log" 2>&1 &
  pipewire-pulse >"$PROBE_SCRATCH/pulse.log" 2>&1 &
  for i in $(seq 1 50); do [ -S "$XDG_RUNTIME_DIR/pulse/native" ] && break; sleep 0.1; done
  pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=probe-sink node.description="Probe" media.class=Audio/Sink object.linger=true audio.position=[FL FR] audio.rate=48000 }' >"$PROBE_SCRATCH/sink.log" 2>&1
  sleep 1
  # The browser's stream goes to the default sink: this one, not a virtual
  # sink the machine's own PipeWire config defines.
  pw-metadata -n default 0 default.configured.audio.sink '{ "name": "probe-sink" }' >"$PROBE_SCRATCH/default.log" 2>&1 || true
  pw-metadata -n default 0 default.audio.sink '{ "name": "probe-sink" }' >>"$PROBE_SCRATCH/default.log" 2>&1 || true
  # The recorder, linked by hand to the sink's monitor ports (no policy
  # is asked to find them).
  pw-record -P '{ node.name=probe-rec node.autoconnect=false }' --rate 48000 --channels 2 --format s16 "$PROBE_SCRATCH/out.wav" >"$PROBE_SCRATCH/record.log" 2>&1 &
  for i in $(seq 1 50); do
    pw-link probe-sink:monitor_FL probe-rec:input_FL 2>>"$PROBE_SCRATCH/link.log" && pw-link probe-sink:monitor_FR probe-rec:input_FR 2>>"$PROBE_SCRATCH/link.log" && break
    sleep 0.2
  done
  pw-link -l >"$PROBE_SCRATCH/links.log" 2>&1 || true
fi
exec "$@"
`;

async function open() {
  const profile = mkdtempSync(join(tmpdir(), 'engine-link-probe-'));
  const audio = process.env.AUDIO || 'pipewire';
  writeFileSync(join(profile, 'user.js'), [
    'user_pref("media.autoplay.default", 0);',
    'user_pref("media.autoplay.blocking_policy", 0);',
    // No device at all (AUDIO=none): cubeb never opens one, and an
    // AudioContext does not run. Otherwise the namespace's own null sink.
    ...(audio === 'none' ? ['user_pref("media.cubeb.force_null_context", true);'] : ['user_pref("media.cubeb.backend", "pulse-rust");']),
    'user_pref("browser.shell.checkDefaultBrowser", false);',
    // The engine's views are WebGL2 canvases; a headless browser on a
    // machine's software renderer offers it only when told to.
    'user_pref("webgl.force-enabled", true);',
    'user_pref("webgl.disabled", false);',
    'user_pref("webgl.enable-webgl2", true);',
    'user_pref("layers.acceleration.force-enabled", true);',
    ...(process.env.PREFS || '').split(';').filter(Boolean).map((p) => `user_pref(${p});`),
  ].join('\n') + '\n');
  const port = 19000 + Math.floor(Math.random() * 900);
  const runtime = process.env.XDG_RUNTIME_DIR || `/run/user/${process.getuid()}`;
  const scratch = join(profile, 'audio');
  mkdirSync(scratch, { recursive: true });
  // The display: Xvfb when there is one (XVFB names it, else the PATH's),
  // else headless, where the engine's views cannot draw.
  let xvfb = process.env.XVFB ?? '';
  if (!xvfb && process.env.DISPLAY_MODE !== 'headless') {
    try { xvfb = execFileSync('sh', ['-c', 'command -v Xvfb']).toString().trim(); } catch { xvfb = ''; }
  }
  const env = { ...process.env, XDG_RUNTIME_DIR: runtime, MOZ_HEADLESS: '1', PROBE_AUDIO: audio, PROBE_SCRATCH: scratch,
    PROBE_XVFB: xvfb, PROBE_DISPLAY: String(200 + Math.floor(Math.random() * 700)), PROBE_SCREEN: `${Math.max(width, 1280)}x${Math.max(height, 900)}x24` };
  delete env.LD_LIBRARY_PATH;
  delete env.PIPEWIRE_REMOTE;
  delete env.PULSE_SERVER;
  delete env.DBUS_SESSION_BUS_ADDRESS;
  const launcher = join(profile, 'launch.sh');
  writeFileSync(launcher, LAUNCH);
  try {
    execFileSync('unshare', ['-r', '-m', 'sh', launcher, 'check'], { env, stdio: 'ignore' });
  } catch {
    throw new Error('cannot run the browser away from this machine\'s audio (unshare -r -m, or hiding its sockets and cards, failed): refusing to play anything');
  }
  const proc = spawn('unshare', ['-r', '-m', 'sh', launcher, 'run', firefox(), ...(xvfb ? [] : ['--headless']), '--no-remote', '--profile', profile, '--remote-debugging-port', String(port)], { stdio: 'ignore', env, detached: true });
  let ws;
  for (let i = 0; i < 160 && !ws; i++) {
    await sleep(250);
    try {
      const s = new WebSocket(`ws://127.0.0.1:${port}/session`);
      await new Promise((ok, no) => { s.onopen = ok; s.onerror = no; });
      ws = s;
    } catch {}
  }
  if (!ws) { try { process.kill(-proc.pid, 'SIGTERM'); } catch {} throw new Error('firefox did not start'); }
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && waiting.has(m.id)) {
      const w = waiting.get(m.id);
      waiting.delete(m.id);
      m.type === 'error' ? w.no(new Error(`${m.error}: ${m.message}`)) : w.ok(m.result);
    }
  };
  const send = (method, params = {}) => new Promise((ok, no) => {
    const i = ++id;
    // A command the browser never answers fails the probe; it does not hang it.
    const timer = setTimeout(() => { waiting.delete(i); no(new Error(`${method} was not answered in 120 s`)); }, 120000);
    waiting.set(i, { ok: (v) => { clearTimeout(timer); ok(v); }, no: (e) => { clearTimeout(timer); no(e); } });
    ws.send(JSON.stringify({ id: i, method, params }));
  });
  await send('session.new', { capabilities: {} });
  const context = (await send('browsingContext.create', { type: 'tab' })).context;
  await send('browsingContext.setViewport', { context, viewport: { width, height } });
  const tab = {
    pgid: proc.pid,
    audio,
    display: xvfb ? 'xvfb' : 'headless',
    go: (url) => send('browsingContext.navigate', { context, url, wait: 'complete' }),
    async frame() {
      const tree = await send('browsingContext.getTree', { root: context });
      return tree.contexts[0]?.children?.find((c) => c.url.includes('/_ext/webview.'))?.context ?? null;
    },
    async evalIn(frame, expression) {
      const r = await send('script.evaluate', { expression, target: { context: frame }, awaitPromise: true });
      if (r.type === 'exception') throw new Error(r.exceptionDetails.text);
      return r.result.value;
    },
    press: (x, y) => send('input.performActions', { context, actions: [{ type: 'pointer', id: 'mouse', parameters: { pointerType: 'mouse' }, actions: [
      { type: 'pointerMove', x: Math.round(x), y: Math.round(y) }, { type: 'pointerDown', button: 0 }, { type: 'pause', duration: 60 }, { type: 'pointerUp', button: 0 }] }] }),
    eval: async (expression) => {
      const r = await send('script.evaluate', { expression, target: { context }, awaitPromise: true });
      if (r.type === 'exception') throw new Error(r.exceptionDetails.text);
      return r.result.value;
    },
    async until(expression, what, ms = 15000) {
      const started = Date.now();
      for (;;) {
        const v = await tab.eval(expression).catch(() => null);
        if (v) return v;
        if (Date.now() - started > ms) throw new Error(`waited ${ms} ms for ${what}`);
        await sleep(100);
      }
    },
    async shot(name) {
      if (!shots) return;
      mkdirSync(shots, { recursive: true });
      const r = await send('browsingContext.captureScreenshot', { context, origin: 'viewport' });
      const path = join(shots, `${name}-${width}.png`);
      writeFileSync(path, Buffer.from(r.data, 'base64'));
      log(`screenshot ${path}`);
    },
    // What reached the namespace's null sink, by sox: its length, its
    // loudest sample and its RMS, whole and over the last five seconds.
    recorded() {
      if (audio !== 'pipewire') return null;
      try { return this.measured ??= this.measure(); } catch (e) { return { error: String(e.message).slice(0, 200) }; }
    },
    measure() {
      const wav = join(scratch, 'out.wav');
      const stat = (args) => {
        try {
          const r = execFileSync('sh', ['-c', `sox "${wav}" -n ${args} stat 2>&1`]).toString();
          const pick = (k) => { const m = r.match(new RegExp(k + '\\s*:?\\s*(-?[0-9.]+)')); return m ? Number(m[1]) : null; };
          return { seconds: pick('Length \\(seconds\\)'), peak: pick('Maximum amplitude'), rms: pick('RMS\\s+amplitude') };
        } catch (e) {
          return { error: String(e.message).slice(0, 200) };
        }
      };
      return { whole: stat(''), last_5s: stat('reverse trim 0 5') };
    },
  };
  // Stop everything (the recorder writes its header as it ends), then
  // read what it recorded, then forget the scratch folder.
  const close = async () => {
    try { ws.close(); } catch {}
    try { process.kill(-proc.pid, 'SIGTERM'); } catch {}
    await sleep(1500);
    const heard = tab.recorded();
    if (process.env.KEEP) log(`kept ${profile}`);
    try { if (!process.env.KEEP) rmSync(profile, { recursive: true, force: true }); } catch {}
    return heard;
  };
  return { tab, close };
}

const { tab, close } = await open();
const out = { url: share, viewport: [width, height], song: wanted, audio: tab.audio, display: tab.display };
let failed = false;
// What the engine says of itself, on the page's root element
// (apps/architect/web/src/clock.rs `report`).
const engine = `JSON.stringify(Object.fromEntries(Object.entries(document.documentElement.dataset).filter(([k]) => k.startsWith('engine'))))`;
const read = async () => JSON.parse(await tab.eval(engine));
const inFrame = async (frame, pick) => {
  const at = await tab.eval(`(() => { const r = document.querySelector('iframe[src*="/_ext/webview."]').getBoundingClientRect(); return JSON.stringify([r.x, r.y]); })()`);
  const el = await tab.evalIn(frame, `(() => { const e = ${pick}; if (!e) return ''; e.scrollIntoView({ block: 'center' }); const r = e.getBoundingClientRect(); return JSON.stringify([r.x + r.width / 2, r.y + r.height / 2]); })()`);
  if (!el) return null;
  const [fx, fy] = JSON.parse(at);
  const [x, y] = JSON.parse(el);
  return [fx + x, fy + y];
};
const frameWith = async (testid, what, ms = 60000) => {
  const started = Date.now();
  for (;;) {
    const f = await tab.frame();
    if (f && await tab.evalIn(f, `!!document.querySelector('[data-testid="${testid}"]')`).catch(() => false)) return f;
    if (Date.now() - started > ms) throw new Error(`waited ${ms} ms for ${what}`);
    await sleep(200);
  }
};
const pressTab = async (name) => {
  const at = await tab.eval(`(() => { const b = [...document.querySelectorAll('button')].find(b => (b.getAttribute('aria-label') || b.title || b.innerText.trim()) === ${JSON.stringify(name)}); if (!b) return ''; const r = b.getBoundingClientRect(); return JSON.stringify([r.x + r.width / 2, r.y + r.height / 2]); })()`);
  if (!at) throw new Error(`no ${name} tab`);
  await tab.press(...JSON.parse(at));
};
const more = async (entry) => {
  await pressTab('More');
  const at = await tab.until(`(() => { const b = [...document.querySelectorAll('[data-testid="tab-menu-item"]')].find(b => b.innerText.trim() === ${JSON.stringify(entry)}); if (!b) return ''; const r = b.getBoundingClientRect(); return JSON.stringify([r.x + r.width / 2, r.y + r.height / 2]); })()`, `${entry} in the More menu`, 8000);
  await tab.press(...JSON.parse(at));
};
// A hosted panel on show: its root, and the canvases the engine draws into.
const hosted = (which) => `(() => { const h = document.querySelector('[data-hosted="${which}"]'); if (!h) return ''; const c = [...h.querySelectorAll('canvas')].map(c => [c.width, c.height]); return c.length ? JSON.stringify({ listening: h.dataset.hostedListening, canvases: c }) : ''; })()`;

try {
  out.rss_blank_mb = rss(tab.pgid);
  out.webgl2 = await tab.eval(`(() => { try { const c = document.createElement('canvas'); const g = c.getContext('webgl2'); if (!g) return 'none'; const e = g.getExtension('WEBGL_debug_renderer_info'); return e ? String(g.getParameter(e.UNMASKED_RENDERER_WEBGL)) : 'webgl2'; } catch (e) { return 'none: ' + e; } })()`);
  if (process.env.ONLY === 'webgl') throw new Error('checked WebGL2 only');
  const t0 = Date.now();
  await tab.go(share);
  const set = await frameWith('studio-setlist', 'Studio\'s playlist on the setlist note');
  out.playlist_ms = Date.now() - t0;
  const rows = JSON.parse(await tab.evalIn(set, `JSON.stringify([...document.querySelectorAll('.sl-row')].map(r => ({ label: r.getAttribute('aria-label') || '', off: r.getAttribute('aria-disabled') === 'true' })))`));
  out.songs_listed = rows.length;
  const index = wanted ? rows.findIndex((r) => r.label.includes(wanted)) : rows.findIndex((r) => !r.off);
  if (index < 0) throw new Error(`the playlist does not list ${wanted || 'a song that plays'} (${rows.map((r) => r.label).join(' | ')})`);
  // The engine, with the link's set: the first song up, then the one asked for.
  await tab.until(`document.documentElement.dataset.engine === 'open' || document.documentElement.dataset.engine === 'failed'`, 'the engine to open the link\'s set', 180000);
  let e = await read();
  if (e.engine !== 'open') throw new Error('the engine did not open the link\'s set');
  out.engine_open_ms = Math.round(Number(e.engineOpenMs) - Number(e.engineOpenBeganMs));
  log(`the engine opened the first song ${out.engine_open_ms} ms after it began`);
  await tab.until(`Number(document.documentElement.dataset.engineSongs || 0) > ${index}`, `song ${index + 1} of the set to open in the engine`, 600000);
  out.song_open_ms = Date.now() - t0;
  e = await read();
  out.engine_songs_when_pressed = Number(e.engineSongs);
  out.rss_open_mb = rss(tab.pgid);
  await tab.shot('setlist');
  // Play it, from the playlist: the press is the page's gesture.
  const at = await inFrame(set, `document.querySelectorAll('.sl-row')[${index}]`);
  const pressed = Number(await tab.eval('performance.now()'));
  await tab.press(...at);
  await tab.until(`document.documentElement.dataset.engineClock === 'engine'`, 'the engine to be the playhead\'s clock for the song', 30000);
  await tab.until(`!!document.documentElement.dataset.engineFirstAudioMs`, 'a frame that is not silence at the worklet\'s output', 90000);
  e = await read();
  out.clock = e.engineClock;
  out.listening_at_first_audio = e.engineListening;
  out.first_audio_after_press_ms = Math.round(Number(e.engineFirstAudioMs) - pressed);
  out.first_audio_after_engine_asked_ms = Math.round(Number(e.engineFirstAudioMs) - Number(e.enginePlayAskedMs));
  log(`first audio ${out.first_audio_after_press_ms} ms after the press, heard as ${e.engineListening}`);
  // Let it play, watching what comes out and what it weighs.
  const before = e;
  const samples = [];
  const began = Date.now();
  while (Date.now() - began < seconds * 1000) {
    await sleep(1000);
    const now = await read();
    samples.push({ t: Number(now.engineTime), loud: Number(now.engineLoudFrames), dry: Number(now.engineDryFrames), listening: now.engineListening, rss: rss(tab.pgid) });
  }
  const last = samples[samples.length - 1];
  const wall = (Date.now() - began) / 1000;
  out.played = {
    wall_seconds: Number(wall.toFixed(1)),
    transport_from: Number(before.engineTime),
    transport_to: last.t,
    loud_frames: last.loud - Number(before.engineLoudFrames),
    loud_seconds: Number(((last.loud - Number(before.engineLoudFrames)) / 48000).toFixed(2)),
    underrun_frames: last.dry - Number(before.engineDryFrames),
    peak: Number((await read()).enginePeak),
    listening: [...new Set(samples.map((s) => s.listening))],
    stems_after_s: (() => { const i = samples.findIndex((s) => s.listening === 'Stems'); return i < 0 ? null : i + 1; })(),
  };
  out.rss_playing_mb = { all: Math.max(...samples.map((s) => s.rss.all)), largest: Math.max(...samples.map((s) => s.rss.largest)), at_end: last.rss };
  await tab.shot('song-playing');
  // The views behind the shell's tabs.
  // Not fatal: what was played is measured either way (a browser with no
  // WebGL2 shows the views' own notice instead).
  try {
    await more('Arrangement');
    out.arrangement = JSON.parse(await tab.until(hosted('arrangement'), 'the engine\'s Arrangement behind its tab', 30000));
    await sleep(2500);
    await tab.shot('arrangement');
    await more('Mixer');
    out.mixer = JSON.parse(await tab.until(hosted('mixer'), 'the engine\'s Mixer behind its tab', 30000));
    await sleep(2500);
    await tab.shot('mixer');
    let n = 0;
    for (const point of (process.env.PRESS || '').split(';').filter(Boolean)) {
      const [x, y] = point.split(',').map(Number);
      const was = await read();
      await tab.press(x, y);
      await sleep(2500);
      const now = await read();
      await tab.shot(`mixer-press-${++n}`);
      (out.presses ||= []).push({ at: [x, y], loud_before: Number(was.engineLoudFrames), loud_after: Number(now.engineLoudFrames), time: Number(now.engineTime) });
    }
  } catch (err) {
    out.views_error = err.message;
    out.views_notice = await tab.eval(`(() => { const h = document.querySelector('[data-provided-view]'); return h ? h.innerText.trim().slice(0, 200) : ''; })()`).catch(() => '');
    await tab.shot('views-failed').catch(() => {});
  }
  out.end = await read();
  out.rss_end_mb = rss(tab.pgid);
} catch (err) {
  failed = true;
  out.error = err.message;
  out.engine = await read().catch(() => null);
  out.audio_context = await tab.eval(`(async () => { try { const c = new AudioContext(); await Promise.race([c.resume(), new Promise(r => setTimeout(r, 1500))]); await new Promise(r => setTimeout(r, 500)); return c.state + ' at ' + c.currentTime.toFixed(2); } catch (e) { return String(e); } })()`).catch((e) => String(e));
  await tab.shot('failed').catch(() => {});
  console.error(`probe: FAILED: ${err.message}`);
} finally {
  out.at_the_sink = await close();
}
console.log(JSON.stringify(out));
process.exit(failed ? 1 : 0);

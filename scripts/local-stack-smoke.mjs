// The local stack in a real browser (`just dev-smoke`, scripts/local-stack-smoke.sh):
// a headless Firefox over WebDriver BiDi (FIREFOX=… names one, else
// `firefox` on the PATH; Chromium hangs on http loads on THEBATTLESHIP)
// opens the share link's setlist page, then its first song with a chart,
// plays it and sees the playhead move and the chart's bar follow.
//
// SHARE_URL   the setlist page (scripts/local-stack.sh url)
// SCREENSHOTS a folder: the setlist and the playing song at 390 px wide
//
// The browser can never reach this machine's audio graph: it runs in a
// mount namespace of its own (`unshare -r -m`) where the runtime dir and
// /run/pipewire are empty, so there is no PipeWire or Pulse socket to
// connect to, and with cubeb's null context. Media still plays, on the
// system clock. Without the namespace the check refuses to run.
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const share = process.env.SHARE_URL;
const shots = process.env.SCREENSHOTS;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (m) => console.error(`smoke: ${m}`);

function firefox() {
  if (process.env.FIREFOX) return process.env.FIREFOX;
  return execFileSync('sh', ['-c', 'command -v firefox']).toString().trim();
}

async function open() {
  const profile = mkdtempSync(join(tmpdir(), 'local-stack-smoke-'));
  writeFileSync(join(profile, 'user.js'), [
    'user_pref("media.autoplay.default", 0);',
    'user_pref("media.autoplay.blocking_policy", 0);',
    // No audio device, ever: cubeb never opens one.
    'user_pref("media.cubeb.force_null_context", true);',
    'user_pref("browser.shell.checkDefaultBrowser", false);',
  ].join('\n') + '\n');
  const port = 19000 + Math.floor(Math.random() * 900);
  const runtime = process.env.XDG_RUNTIME_DIR || `/run/user/${process.getuid()}`;
  const env = { ...process.env, XDG_RUNTIME_DIR: runtime, MOZ_HEADLESS: '1' };
  delete env.LD_LIBRARY_PATH;
  delete env.PIPEWIRE_REMOTE;
  delete env.PULSE_SERVER;
  delete env.DBUS_SESSION_BUS_ADDRESS;
  // Empty tmpfs over every place an audio socket lives, then Firefox.
  const hide = 'mount -t tmpfs none "$XDG_RUNTIME_DIR" && { [ ! -d /run/pipewire ] || mount -t tmpfs none /run/pipewire; } && exec "$0" "$@"';
  try {
    execFileSync('unshare', ['-r', '-m', 'sh', '-c', `${hide.replace(' && exec "$0" "$@"', '')} && [ -z "$(ls -A "$XDG_RUNTIME_DIR")" ]`], { env, stdio: 'ignore' });
  } catch {
    throw new Error('cannot run the browser away from the audio graph (unshare -r -m failed): refusing to play anything');
  }
  const proc = spawn('unshare', ['-r', '-m', 'sh', '-c', hide, firefox(), '--headless', '--no-remote', '--profile', profile, '--remote-debugging-port', String(port)], { stdio: 'ignore', env, detached: true });
  let ws;
  for (let i = 0; i < 120 && !ws; i++) {
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
    waiting.set(i, { ok, no });
    ws.send(JSON.stringify({ id: i, method, params }));
  });
  await send('session.new', { capabilities: {} });
  // A tab of its own (getTree can list a context only system access may drive).
  const context = (await send('browsingContext.create', { type: 'tab' })).context;
  await send('browsingContext.setViewport', { context, viewport: { width: 390, height: 844 } });
  const tab = {
    go: (url) => send('browsingContext.navigate', { context, url, wait: 'complete' }),
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
      const path = join(shots, `${name}.png`);
      writeFileSync(path, Buffer.from(r.data, 'base64'));
      log(`screenshot ${path}`);
    },
  };
  // The whole group: unshare's shell, Firefox and its content processes.
  const close = () => { try { ws.close(); } catch {} try { process.kill(-proc.pid, 'SIGTERM'); } catch {} try { rmSync(profile, { recursive: true, force: true }); } catch {} };
  return { tab, close };
}

const { tab, close } = await open();
let failed = false;
try {
  await tab.go(share);
  const rows = await tab.until(`document.querySelectorAll('[data-track]').length`, 'the setlist\'s playable rows');
  log(`the setlist page has ${rows} playable rows (songs with a recording)`);
  await tab.shot('setlist-390');
  // Each row's song page; the first whose chart follows the playhead plays.
  const links = await tab.eval(`JSON.stringify([...document.querySelectorAll('[data-track][data-page]')].map(e => new URL(e.dataset.page, location.href).href))`);
  let song = null;
  for (const href of [...new Set(JSON.parse(links))]) {
    if (href === share) continue;
    await tab.go(href);
    if (await tab.eval(`!!document.querySelector('[data-follow-playhead] [data-at]')`)) { song = href; break; }
  }
  if (!song) throw new Error('no song page with a chart that follows the playhead');
  log(`playing ${song}`);
  const before = await tab.eval('architect.playhead.time');
  await tab.eval('architect.playhead.play()');
  await tab.until('architect.playhead.playing', 'the player to start', 20000);
  const t = await tab.until(`architect.playhead.time > ${before} + 1.5 && architect.playhead.time`, 'the playhead to move 1.5 s', 20000);
  log(`the playhead moved: ${before.toFixed(2)} s -> ${Number(t).toFixed(2)} s`);
  const bar = await tab.until(`document.querySelector('[data-follow-playhead] [aria-current=time]')?.getAttribute('data-at')`, 'a bar of the chart marked at the playhead', 10000);
  log(`the chart follows: the bar at ${bar} s is current`);
  await tab.shot('song-playing-390');
  await tab.eval('architect.playhead.pause()');
} catch (e) {
  failed = true;
  console.error(`smoke: FAILED: ${e.message}`);
} finally {
  close();
}
process.exit(failed ? 1 : 0);

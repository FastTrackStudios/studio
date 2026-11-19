// The local stack in a real browser (`just dev-smoke`, scripts/local-stack-smoke.sh):
// a headless Firefox over WebDriver BiDi (FIREFOX=… names one, else
// `firefox` on the PATH; Chromium hangs on http loads on THEBATTLESHIP)
// opens the share link as the web app's guest (architect#646), sees
// Studio's playlist drawn on the setlist note with the set's songs, plays
// a song from it and sees the recording load, the playhead move and the
// row marked; then finds the link's own tabs (and no member control),
// presses Charts and sees the playing song's chart engraved with the bar
// being played marked, and Arrangement and sees its playhead.
//
// SHARE_URL   the setlist page (scripts/local-stack.sh url)
// SONGS       the titles the playlist must list, one a line (optional)
// SCREENSHOTS a folder: the setlist, the song playing and its chart at 390 px wide
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
    // The view's frame (the app's sandboxed web view document), once it is there.
    async frame() {
      const tree = await send('browsingContext.getTree', { root: context });
      return tree.contexts[0]?.children?.find((c) => c.url.includes('/_ext/webview.'))?.context ?? null;
    },
    async evalIn(frame, expression) {
      const r = await send('script.evaluate', { expression, target: { context: frame }, awaitPromise: true });
      if (r.type === 'exception') throw new Error(r.exceptionDetails.text);
      return r.result.value;
    },
    // A real pointer press at a point of the page.
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
// Where an element of the frame is on the page: the frame's box plus the element's own.
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
// What the app's player holds: its marks on the app's audio element (`data-media`
// the recording, `data-clock` whether the app decodes it or the browser
// plays it, `data-time` where it is).
const audio = `(() => { const a = document.querySelector('audio'); return a && a.dataset.media ? JSON.stringify({ src: decodeURIComponent(a.dataset.media), clock: a.dataset.clock, t: Number(a.dataset.time || 0) }) : ''; })()`;
try {
  await tab.go(share);
  // The app, as the link's guest, with Studio's playlist on the setlist note.
  const set = await frameWith('studio-setlist', 'Studio\'s playlist on the setlist note (the node lists the island\'s claims: /_ext/islands.json)');
  const rows = JSON.parse(await tab.evalIn(set, `JSON.stringify([...document.querySelectorAll('.sl-row')].map(r => ({ label: r.getAttribute('aria-label') || '', off: r.getAttribute('aria-disabled') === 'true' })))`));
  if (!rows.length) throw new Error('the playlist lists no songs');
  for (const title of (process.env.SONGS || '').split('\n').filter(Boolean)) {
    if (!rows.some((r) => r.label.includes(title))) throw new Error(`the playlist does not list ${title}`);
  }
  const first = rows.findIndex((r) => !r.off);
  if (first < 0) throw new Error(`none of the playlist's ${rows.length} songs can play (no recording the link serves)`);
  log(`the playlist lists ${rows.length} songs, ${rows.filter((r) => !r.off).length} with a recording`);
  await tab.shot('setlist-390');
  const at = await inFrame(set, `document.querySelectorAll('.sl-row')[${first}]`);
  await tab.press(...at);
  const loaded = JSON.parse(await tab.until(audio, 'the recording in the app\'s player', 30000));
  log(`playing ${loaded.src.split('/').pop()}`);
  // It moves: decoded by the app (Ogg, on every browser), or, where no
  // audio device lets a context start (this check's sandbox), on the
  // browser's own player.
  const moved = JSON.parse(await tab.until(`(() => { const s = ${audio}; return s && JSON.parse(s).t > 1 ? s : ''; })()`, 'the playhead to move a second', 30000));
  log(`the playhead moved to ${moved.t.toFixed(1)} s, played by ${moved.clock === 'decoded' ? 'the app\'s decoder' : 'the browser\'s player'}`);
  await tab.until(`navigator.mediaSession && navigator.mediaSession.playbackState === 'playing'`, 'the player to say it is playing', 20000);
  const title = await tab.eval(`navigator.mediaSession.metadata ? navigator.mediaSession.metadata.title : ''`);
  // The row is marked, and its time moves (the deck's clock, heard or not).
  const started = Date.now();
  for (;;) {
    if (await tab.evalIn(set, `!!document.querySelector('.sl-row[data-current][data-playing]')`)) break;
    if (Date.now() - started > 15000) throw new Error('the playing song\'s row is not marked');
    await sleep(200);
  }
  log(`the Now Playing bar and the lock screen's title say ${title}; its row is marked`);
  await tab.shot('song-playing-390');
  // The link's own tabs, as its extension declares them, and no control a
  // guest cannot use.
  const controls = JSON.parse(await tab.eval(`JSON.stringify([...document.querySelectorAll('button')].map(b => (b.getAttribute('aria-label') || b.title || b.innerText.trim())))`));
  for (const want of ['Setlist', 'Chart', 'Lyrics', 'More']) {
    if (!controls.includes(want)) throw new Error(`no ${want} tab at the bottom of the link (${controls.filter((c) => c.length < 20).join(', ')})`);
  }
  for (const not of ['New note', 'Vault', 'Files', 'Open…', 'Search']) {
    if (controls.includes(not)) throw new Error(`a guest is shown a member's control: ${not}`);
  }
  log('the link has its Setlist, Chart, Lyrics and More tabs, and no member control');
  const pressTab = async (name) => {
    const at = await tab.eval(`(() => { const b = [...document.querySelectorAll('button')].find(b => (b.getAttribute('aria-label') || b.title || b.innerText.trim()) === ${JSON.stringify(name)}); const r = b.getBoundingClientRect(); return JSON.stringify([r.x + r.width / 2, r.y + r.height / 2]); })()`);
    await tab.press(...JSON.parse(at));
  };
  // Charts: the chart of the song playing, engraved, and following.
  await pressTab('Chart');
  const song = await frameWith('studio-song', `${title}'s song view`);
  const chart = await (async () => {
    const t0 = Date.now();
    for (;;) {
      const v = await tab.evalIn(song, `(() => { const s = document.querySelector('[data-testid="studio-song"]'); const bars = document.querySelectorAll('.sv-bar').length; const cur = document.querySelector('[data-testid="chart-current-bar"]'); const none = [...document.querySelectorAll('.sv-empty')].map(e => e.textContent).join(' '); return JSON.stringify({ following: s.dataset.following, bars, bar: cur ? cur.dataset.bar : '', none }); })()`);
      const c = JSON.parse(v);
      if (c.bar || /No chart/.test(c.none) || Date.now() - t0 > 60000) return c;
      await sleep(300);
    }
  })();
  if (/No chart/.test(chart.none)) {
    log(`${title} has no chart: its song view drew without one`);
  } else {
    if (!chart.bars) throw new Error(`the chart did not engrave (${chart.none || 'no bars'})`);
    if (!chart.bar) throw new Error(`the chart has ${chart.bars} bars and none is marked at the playhead (following: ${chart.following})`);
    log(`the chart is engraved (${chart.bars} bars) and follows: bar ${chart.bar} is the one being played`);
  }
  await tab.eval(`(() => { [...document.querySelectorAll('*')].forEach(e => { if (e.scrollTop > 0) e.scrollTop = 0; }); return 1; })()`);
  await tab.shot('song-chart-390');
  // Arrangement: the song playing on its timeline, the playhead in it.
  await pressTab('More');
  const entries = JSON.parse(await tab.until(`(() => { const e = [...document.querySelectorAll('[data-testid="tab-menu-item"]')].map(b => b.innerText.trim()); return e.length ? JSON.stringify(e) : ''; })()`, 'the More menu above its tab', 5000));
  if (entries.join(', ') !== 'Arrangement, Mixer, Lighting Visualizer') throw new Error(`the More menu holds ${entries.join(', ')}`);
  const entryAt = await tab.eval(`(() => { const r = document.querySelector('[data-testid="tab-menu-item"]').getBoundingClientRect(); return JSON.stringify([r.x + r.width / 2, r.y + r.height / 2]); })()`);
  await tab.press(...JSON.parse(entryAt));
  const arr = await frameWith('studio-arrangement', 'the arrangement view');
  const head = await tab.evalIn(arr, `(() => { const p = document.querySelector('[data-testid="arrangement-playhead"]'); const s = document.querySelectorAll('[data-testid="arrangement-section"]').length; return p ? JSON.stringify({ left: parseFloat(p.style.left), sections: s }) : ''; })()`);
  if (!head) throw new Error('the arrangement draws no playhead for the song playing');
  log(`the arrangement shows the song (${JSON.parse(head).sections} section${JSON.parse(head).sections === 1 ? '' : 's'}) with its playhead ${JSON.parse(head).left} points in`);
  await tab.shot('song-arrangement-390');
} catch (e) {
  failed = true;
  console.error(`smoke: FAILED: ${e.message}`);
} finally {
  close();
}
process.exit(failed ? 1 : 0);

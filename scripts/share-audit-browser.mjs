// A browser to walk a share link by hand or by script (the audit in
// docs/design/share-link-setlist.md): a headless Firefox over WebDriver
// BiDi, kept open, driven by one JSON command per HTTP POST.
//
//   node scripts/share-audit-browser.mjs [port]        (default 19777)
//   curl -s 127.0.0.1:19777 -d '{"go":"http://127.0.0.1:7523/s/<code>/"}'
//   curl -s 127.0.0.1:19777 -d '{"viewport":[390,844]}'
//   curl -s 127.0.0.1:19777 -d '{"eval":"document.title"}'
//   curl -s 127.0.0.1:19777 -d '{"frames":true}'               the page's frames
//   curl -s 127.0.0.1:19777 -d '{"eval":"…","frame":"<context>"}'
//   curl -s 127.0.0.1:19777 -d '{"click":[195,400]}'           a real pointer press
//   curl -s 127.0.0.1:19777 -d '{"shot":"/abs/path.png"}'      (+ "full":true the whole page)
//   curl -s 127.0.0.1:19777 -d '{"console":true}'              the page's console and network errors so far
//   curl -s 127.0.0.1:19777 -d '{"quit":true}'
//
// As the smoke test's browser (scripts/local-stack-smoke.mjs): Firefox,
// because Chromium hangs on http loads on THEBATTLESHIP; in a mount
// namespace of its own where the runtime dir and /run/pipewire are empty
// and with cubeb's null context, so it can never open an output on this
// machine's audio graph. Media still plays, on the system clock. Without
// the namespace it refuses to run.
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

const listen = Number(process.argv[2] || 19777);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function firefox() {
  if (process.env.FIREFOX) return process.env.FIREFOX;
  return execFileSync('sh', ['-c', 'command -v firefox']).toString().trim();
}

const profile = mkdtempSync(join(tmpdir(), 'share-audit-'));
writeFileSync(join(profile, 'user.js'), [
  'user_pref("media.autoplay.default", 0);',
  'user_pref("media.autoplay.blocking_policy", 0);',
  'user_pref("media.cubeb.force_null_context", true);',
  'user_pref("browser.shell.checkDefaultBrowser", false);',
].join('\n') + '\n');
const port = 19000 + Math.floor(Math.random() * 700);
const runtime = process.env.XDG_RUNTIME_DIR || `/run/user/${process.getuid()}`;
const env = { ...process.env, XDG_RUNTIME_DIR: runtime, MOZ_HEADLESS: '1' };
for (const k of ['LD_LIBRARY_PATH', 'PIPEWIRE_REMOTE', 'PULSE_SERVER', 'DBUS_SESSION_BUS_ADDRESS']) delete env[k];
const hide = 'mount -t tmpfs none "$XDG_RUNTIME_DIR" && { [ ! -d /run/pipewire ] || mount -t tmpfs none /run/pipewire; }';
try {
  execFileSync('unshare', ['-r', '-m', 'sh', '-c', `${hide} && [ -z "$(ls -A "$XDG_RUNTIME_DIR")" ]`], { env, stdio: 'ignore' });
} catch {
  console.error('share-audit: cannot run the browser away from the audio graph (unshare -r -m failed): refusing');
  process.exit(1);
}
const proc = spawn('unshare', ['-r', '-m', 'sh', '-c', `${hide} && exec "$0" "$@"`, firefox(), '--headless', '--no-remote', '--profile', profile, '--remote-debugging-port', String(port)], { stdio: 'ignore', env, detached: true });

let ws;
for (let i = 0; i < 160 && !ws; i++) {
  await sleep(250);
  try {
    const s = new WebSocket(`ws://127.0.0.1:${port}/session`);
    await new Promise((ok, no) => { s.onopen = ok; s.onerror = no; });
    ws = s;
  } catch {}
}
const close = () => {
  try { ws?.close(); } catch {}
  try { process.kill(-proc.pid, 'SIGTERM'); } catch {}
  try { rmSync(profile, { recursive: true, force: true }); } catch {}
};
if (!ws) { close(); console.error('share-audit: firefox did not start'); process.exit(1); }

let id = 0;
const waiting = new Map();
const heard = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) {
    const w = waiting.get(m.id);
    waiting.delete(m.id);
    m.type === 'error' ? w.no(new Error(`${m.error}: ${m.message}`)) : w.ok(m.result);
  } else if (m.method === 'log.entryAdded') {
    heard.push(`${m.params.level}: ${String(m.params.text).slice(0, 400)}`);
  } else if (m.method === 'network.responseCompleted') {
    const r = m.params.response;
    if (r.status >= 400) heard.push(`http ${r.status}: ${m.params.request.url}`);
  } else if (m.method === 'network.fetchError') {
    heard.push(`fetch failed: ${m.params.request.url} (${m.params.errorText})`);
  }
  if (heard.length > 400) heard.splice(0, heard.length - 400);
};
const send = (method, params = {}) => new Promise((ok, no) => {
  const i = ++id;
  waiting.set(i, { ok, no });
  ws.send(JSON.stringify({ id: i, method, params }));
});
await send('session.new', { capabilities: {} });
const context = (await send('browsingContext.create', { type: 'tab' })).context;
await send('browsingContext.setViewport', { context, viewport: { width: 390, height: 844 } });
await send('session.subscribe', { events: ['log.entryAdded', 'network.responseCompleted', 'network.fetchError'] });

async function run(c) {
  if (c.viewport) {
    await send('browsingContext.setViewport', { context, viewport: { width: c.viewport[0], height: c.viewport[1] } });
    return 'ok';
  }
  if (c.go) {
    await send('browsingContext.navigate', { context, url: c.go, wait: c.wait || 'complete' });
    return 'ok';
  }
  if (c.frames) {
    const flat = (n, depth = 0) => [{ context: n.context, url: n.url, depth }, ...(n.children || []).flatMap((k) => flat(k, depth + 1))];
    return (await send('browsingContext.getTree', { root: context })).contexts.flatMap((n) => flat(n));
  }
  if (c.eval !== undefined) {
    const r = await send('script.evaluate', { expression: c.eval, target: { context: c.frame || context }, awaitPromise: true });
    if (r.type === 'exception') return { exception: r.exceptionDetails.text };
    return r.result.value ?? r.result.type;
  }
  if (c.click) {
    const [x, y] = c.click;
    await send('input.performActions', {
      context,
      actions: [{
        type: 'pointer', id: 'finger', parameters: { pointerType: c.touch ? 'touch' : 'mouse' },
        actions: [
          { type: 'pointerMove', x: Math.round(x), y: Math.round(y) },
          { type: 'pointerDown', button: 0 },
          { type: 'pause', duration: 60 },
          { type: 'pointerUp', button: 0 },
        ],
      }],
    });
    return 'ok';
  }
  if (c.shot) {
    mkdirSync(dirname(c.shot), { recursive: true });
    const r = await send('browsingContext.captureScreenshot', { context, origin: c.full ? 'document' : 'viewport' });
    writeFileSync(c.shot, Buffer.from(r.data, 'base64'));
    return c.shot;
  }
  if (c.console) return heard.splice(0);
  if (c.quit) { setTimeout(() => { close(); process.exit(0); }, 50); return 'bye'; }
  return { error: 'go, viewport, eval, frames, click, shot, console or quit' };
}

createServer((req, res) => {
  let body = '';
  req.on('data', (d) => { body += d; });
  req.on('end', async () => {
    let out;
    try { out = await run(JSON.parse(body || '{}')); } catch (e) { out = { error: e.message }; }
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify(out) + '\n');
  });
}).listen(listen, '127.0.0.1', () => console.error(`share-audit: browser ready on http://127.0.0.1:${listen}`));
process.on('SIGTERM', () => { close(); process.exit(0); });
process.on('SIGINT', () => { close(); process.exit(0); });

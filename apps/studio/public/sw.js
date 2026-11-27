// Studio's service worker (studio#716): the web app opens again with no
// network.
//
// - The page's own files (its HTML, script, wasm, styles, fonts) are kept
//   in a cache of their own: the page names them once it has loaded
//   (`keep`, from `register_service_worker` in src/main.rs), and every
//   same-origin GET it makes is answered from the network first and kept.
// - With no network, a request is answered from what was kept: the page
//   itself (whatever its query), its files, and what it last read through
//   the node (a setlist file, a folder's listing, a link's `files.json`).
// - A ranged request (`Range:`) is never touched: a song's proxies and its
//   small files are the page's own to keep (Cache Storage, `song_stream`'s
//   `CachedFetch` and its docs), downloaded for offline from the setlist.

const SHELL = "studio-shell-v1";

// What a song is made of, never kept here: the engine's caches keep it.
const SONG_FILES = /\.(ogg|oga|wav|wave|flac|mp3|aif|aiff|idx|session|sessionpeaks|reapeaks|kf|rpp)$/i;

self.addEventListener("install", () => self.skipWaiting());

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names
          .filter((n) => n.startsWith("studio-shell-") && n !== SHELL)
          .map((n) => caches.delete(n)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("message", (event) => {
  const urls = event.data && event.data.keep;
  if (Array.isArray(urls)) {
    event.waitUntil(keepAll(urls));
  }
});

function sameOrigin(url) {
  return new URL(url, self.location.href).origin === self.location.origin;
}

async function keepAll(urls) {
  const cache = await caches.open(SHELL);
  const wanted = [...new Set(urls)].filter(
    (u) => sameOrigin(u) && !SONG_FILES.test(new URL(u).pathname),
  );
  await Promise.all(
    wanted.map(async (url) => {
      if (await cache.match(url)) {
        return;
      }
      try {
        const response = await fetch(url, { cache: "no-cache" });
        if (response.status === 200) {
          await cache.put(url, response);
        }
      } catch (_) {
        // Offline already: kept next time.
      }
    }),
  );
}

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (
    request.method !== "GET" ||
    request.headers.has("range") ||
    !sameOrigin(request.url)
  ) {
    return;
  }
  event.respondWith(networkFirst(request));
});

async function networkFirst(request) {
  const cache = await caches.open(SHELL);
  try {
    const response = await fetch(request);
    if (response.status === 200 && !response.headers.has("content-range")) {
      cache.put(request, response.clone()).catch(() => {});
    }
    return response;
  } catch (error) {
    const kept =
      (await cache.match(request)) ||
      (request.mode === "navigate"
        ? (await cache.match(request, { ignoreSearch: true })) ||
          (await cache.match(self.registration.scope))
        : undefined);
    if (kept) {
      return kept;
    }
    throw error;
  }
}

// Studio's service worker (studio#716): the web app opens again with no
// network.
//
// What it keeps, and only that:
// - the page itself (a navigation, and any route of its router: whatever
//   its query, answered offline by the page it kept);
// - the page's own files: its script, wasm, styles, fonts and pictures, by
//   their extension;
// - a session folder's listing at a link (`files.json`), which a song kept
//   for offline opens by.
//
// Everything else passes through untouched: Drive's node (signed-in
// replies are never kept here), ranged requests, and a song's files — its
// small files and proxies are the engine's to keep (Cache Storage,
// `song_stream`'s `CachedFetch` and docs), downloaded for offline from the
// setlist.

const SHELL = "studio-shell-v1";

// The page's own files, by extension.
const PAGE_FILES = /\.(html|js|mjs|wasm|css|woff2?|ttf|otf|png|jpe?g|svg|ico|webp|json)$/i;

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

// Whether `url` is one of the files kept here: the page's own (under the
// worker's scope, not its node's API), or a link's folder listing.
function kept(url) {
  const u = new URL(url, self.location.href);
  if (u.origin !== self.location.origin || !u.href.startsWith(self.registration.scope)) {
    return false;
  }
  const path = u.pathname;
  if (path.includes("/studio/api/") || path.startsWith("/_dioxus")) {
    return false;
  }
  if (path.endsWith("/files.json")) {
    return true;
  }
  // A link's session folder (`/sessions/…`, `?session=`) holds a song's
  // files: never the page's.
  if (path.includes("/sessions/")) {
    return false;
  }
  // A route (`/`, `/sets/…`): the page itself, which its router draws.
  if (!/\.[a-z0-9]+$/i.test(path)) {
    return true;
  }
  return PAGE_FILES.test(path);
}

// A reply that may be kept: whole, and not one its server says not to.
function keepable(response) {
  const control = (response.headers.get("cache-control") || "").toLowerCase();
  return (
    response.status === 200 &&
    !response.headers.has("content-range") &&
    !control.includes("no-store") &&
    !control.includes("private")
  );
}

async function keepAll(urls) {
  const cache = await caches.open(SHELL);
  const wanted = [...new Set(urls)].filter(kept);
  await Promise.all(
    wanted.map(async (url) => {
      if (await cache.match(url)) {
        return;
      }
      try {
        const response = await fetch(url, { cache: "no-cache" });
        if (keepable(response)) {
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
  if (request.method !== "GET" || request.headers.has("range")) {
    return;
  }
  const navigation = request.mode === "navigate";
  if (!navigation && !kept(request.url)) {
    return;
  }
  event.respondWith(networkFirst(request, navigation));
});

async function networkFirst(request, navigation) {
  const cache = await caches.open(SHELL);
  try {
    const response = await fetch(request);
    if (keepable(response)) {
      cache.put(request, response.clone()).catch(() => {});
    }
    return response;
  } catch (error) {
    const hit =
      (await cache.match(request)) ||
      (navigation
        ? (await cache.match(request, { ignoreSearch: true })) ||
          (await cache.match(self.registration.scope))
        : undefined);
    if (hit) {
      return hit;
    }
    throw error;
  }
}

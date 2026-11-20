#!/usr/bin/env python3
"""The local stack's front (scripts/local-stack.sh `run web`): the web
bundle and the node on ONE origin, as the hosted proxy joins them.

A share link's page (`/s/<code>/`) is the node's answer, and it loads the
web app (`/app/start.js`, `/assets/app-<sha>/`) from the same origin
(Architect's scripts/web-routes.md). So the node's paths go to the node,
before the static files, and everything else is the bundle by its four
rules (P, P.html, P/index.html, else 404.html with status 404).

    local-stack-front.py <bundle | http://127.0.0.1:<dx port>> <port> <node port> [<bind address>]

Given a URL instead of a bundle, everything that is not the node's goes
there: `dx serve`'s dev server for studio-web, its hot-reload WebSocket
included. The bind address is 127.0.0.1 unless one is given (0.0.0.0:
the LAN, LOCAL_STACK_LAN=on).

`<origin>/<code>` (a link by its code alone) redirects to `/s/<code>/`.

Opened on another origin than loopback (`tailscale serve`, `just phone`),
the app signs in through this front too: with LOCAL_STACK_FRONT_AUTH=<the
issuer's port>, `/app/config.json` names this origin as the issuer and
`/oauth2/` goes to it, the app's way back (`/auth/callback`) and its
Origin rewritten to the loopback ones the issuer and the node register, so
a phone on the tailnet signs in as the demo account with nothing changed
in either of them.

Loopback by default. Streams both ways (a recording's ranges, the link's live
events), and relays an upgrade (the app's WebSocket to `/org/<slug>/vox`).
"""

import http.client
import mimetypes
import os
import re
import select
import socket
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# The node's prefixes (Architect's apps/web/Dioxus.toml proxies the same).
NODE = ("/s/", "/_ext/", "/node/", "/org/", "/vox", "/mcp", "/health")
HOP = {"connection", "keep-alive", "proxy-authenticate", "proxy-authorization", "te", "trailers", "transfer-encoding", "upgrade"}
# A share link's code: 16 of Architect's link alphabet (`architect_spaces::share::is_code`).
SHORT = re.compile(r"^/([abcdefghijkmnpqrstuvwxyz23456789]{16})(/.*)?$")
TYPES = {".wasm": "application/wasm", ".js": "text/javascript", ".mjs": "text/javascript", ".json": "application/json", ".woff2": "font/woff2", ".svg": "image/svg+xml"}


class Front(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    root = ""
    node = 0
    # The issuer's port, to sign in from another origin (0: loopback only).
    auth = 0
    # This front's own port: the loopback origin the issuer and node know.
    port = 0
    # dx's dev server's port, when it serves the app (else the bundle).
    app = 0

    def log_message(self, format, *args):
        pass

    def do_GET(self):
        self.route()

    do_HEAD = do_POST = do_PUT = do_DELETE = do_OPTIONS = do_PATCH = do_GET

    def remote(self):
        """The origin the browser is on, when it is not loopback."""
        host = (self.headers.get("X-Forwarded-Host") or self.headers.get("Host") or "").strip()
        name = host.rsplit(":", 1)[0] if not host.startswith("[") else host.split("]")[0] + "]"
        if not host or name in ("127.0.0.1", "localhost", "[::1]"):
            return None
        proto = self.headers.get("X-Forwarded-Proto") or "http"
        return f"{proto}://{host}"

    def local(self):
        return f"http://127.0.0.1:{self.port}"

    def route(self):
        path = self.path.split("?", 1)[0]
        origin = self.remote() if self.auth else None
        if origin:
            # Every request the node or the issuer sees comes from the
            # loopback origin they allow.
            if self.headers.get("Origin"):
                self.headers.replace_header("Origin", self.local())
            if path == "/app/config.json" and self.command in ("GET", "HEAD"):
                return self.config(origin)
            if path.startswith("/oauth2/"):
                return self.issuer(origin)
        # A link by its code alone (`<origin>/<code>`, the short form a
        # person is sent): to the link's page, `/s/<code>/`, where the node
        # answers it. The node's own paths and the app's relative URLs all
        # hang off `/s/`, so the short form is a redirect, never a mount.
        short = SHORT.match(path)
        if short and self.command in ("GET", "HEAD"):
            query = self.path[len(path):]
            self.send_response(302)
            self.send_header("Location", f"/s/{short.group(1)}{short.group(2) or '/'}{query}")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path == "/s" or any(path.startswith(p) or path == p.rstrip("/") for p in NODE):
            if self.headers.get("Upgrade"):
                self.relay(self.node)
            else:
                self.forward(self.node)
        elif self.app:
            if self.headers.get("Upgrade"):
                self.relay(self.app)
            else:
                self.forward(self.app)
        elif self.command in ("GET", "HEAD"):
            self.static(urllib.parse.unquote(path))
        else:
            self.plain(405, "the bundle is files")

    def config(self, origin):
        import json
        body = json.dumps({"issuer": origin, "client_id": "architect", "auto_sign_in": True}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def issuer(self, origin):
        """`/oauth2/` on another origin: the issuer's, with the app's way
        back swapped for the loopback one it registers, and back again."""
        quote = lambda u: urllib.parse.quote(u, safe="")
        there, here = quote(origin + "/auth/callback"), quote(self.local() + "/auth/callback")
        self.path = self.path.replace(there, here)
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        body = body.replace(there.encode(), here.encode())
        headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP and k.lower() != "content-length"}
        headers["Content-Length"] = str(len(body))
        conn = http.client.HTTPConnection("127.0.0.1", self.auth, timeout=60)
        try:
            conn.request(self.command, self.path, body=body or None, headers=headers)
            resp = conn.getresponse()
            data = resp.read()
        except OSError as e:
            return self.plain(502, f"the issuer does not answer on port {self.auth}: {e}")
        finally:
            conn.close()
        self.send_response(resp.status, resp.reason)
        for k, v in resp.getheaders():
            if k.lower() in HOP or k.lower() == "content-length":
                continue
            if k.lower() == "location":
                v = v.replace(self.local(), origin).replace(f"http://127.0.0.1:{self.auth}", origin)
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)

    def plain(self, status, text):
        body = text.encode()
        self.send_response(status)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def static(self, path):
        base = os.path.realpath(self.root)
        rel = path.lstrip("/")
        tries = [rel] if rel else []
        if not path.endswith("/"):
            tries.append(rel + ".html")
        tries.append(os.path.join(rel, "index.html"))
        # The sign-in's way back is the app's page.
        if path.rstrip("/") == "/auth/callback":
            tries = ["app/index.html"]
        status, found = 200, None
        for t in tries:
            full = os.path.realpath(os.path.join(base, t))
            if (full == base or full.startswith(base + os.sep)) and os.path.isfile(full):
                found = full
                break
        if not found:
            status, found = 404, os.path.join(base, "404.html")
            if not os.path.isfile(found):
                return self.plain(404, "not found")
        ext = os.path.splitext(found)[1]
        ctype = TYPES.get(ext) or mimetypes.guess_type(found)[0] or "application/octet-stream"
        size = os.path.getsize(found)
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(size))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Cache-Control", "public, max-age=31536000, immutable" if path.startswith("/assets/") else "no-store")
        self.end_headers()
        if self.command != "HEAD":
            with open(found, "rb") as f:
                while chunk := f.read(1 << 16):
                    self.wfile.write(chunk)

    def forward(self, port):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else None
        headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP}
        conn = http.client.HTTPConnection("127.0.0.1", port, timeout=3600)
        try:
            conn.request(self.command, self.path, body=body, headers=headers)
            resp = conn.getresponse()
        except OSError as e:
            return self.plain(502, f"nothing answers on port {port} yet (still building?): {e}")
        self.send_response(resp.status, resp.reason)
        sized = False
        for k, v in resp.getheaders():
            if k.lower() in HOP:
                continue
            sized = sized or k.lower() == "content-length"
            self.send_header(k, v)
        if not sized:
            # A stream (the link's live events): until the node closes it.
            self.send_header("Connection", "close")
            self.close_connection = True
        self.end_headers()
        try:
            if self.command != "HEAD":
                while chunk := resp.read1(1 << 16):
                    self.wfile.write(chunk)
                    self.wfile.flush()
        except (OSError, http.client.HTTPException):
            # The node went away mid-answer (a restart): the browser asks again.
            pass
        finally:
            conn.close()

    def relay(self, port):
        try:
            up = socket.create_connection(("127.0.0.1", port), timeout=10)
        except OSError as e:
            return self.plain(502, f"nothing answers on port {port}: {e}")
        up.settimeout(None)
        head = f"{self.command} {self.path} HTTP/1.1\r\n" + "".join(f"{k}: {v}\r\n" for k, v in self.headers.items()) + "\r\n"
        up.sendall(head.encode("latin-1"))
        down = self.connection
        self.close_connection = True
        try:
            while True:
                ready, _, _ = select.select([up, down], [], [], 3600)
                if not ready:
                    break
                for s in ready:
                    data = s.recv(1 << 16)
                    if not data:
                        return
                    (down if s is up else up).sendall(data)
        except OSError:
            pass
        finally:
            up.close()


def main(argv):
    if len(argv) not in (4, 5):
        sys.exit(__doc__)
    target, port, Front.node = argv[1], int(argv[2]), int(argv[3])
    bind = argv[4] if len(argv) == 5 else "127.0.0.1"
    Front.port = port
    Front.auth = int(os.environ.get("LOCAL_STACK_FRONT_AUTH") or 0)
    if target.startswith("http://"):
        Front.app = int(urllib.parse.urlsplit(target).port or 80)
        what = f"the app from dx at :{Front.app}"
    else:
        Front.root = target
        what = f"the bundle {target}"
    server = ThreadingHTTPServer((bind, port), Front)
    server.daemon_threads = True
    print(f"local-stack: front http://{bind}:{port} ({what}; the server's paths to :{Front.node})", file=sys.stderr, flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main(sys.argv)

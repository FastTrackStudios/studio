#!/usr/bin/env python3
"""Call a running Studio's MCP door from a shell.

    STUDIO_INSTANCE=agent scripts/agent-instance/mcp.py list
    scripts/agent-instance/mcp.py run SESSION_SCREENSET '{"screenset":"6"}'
    scripts/agent-instance/mcp.py session_tree '{}'

Reads the door's url and token from studio[-<instance>]-mcp.json in ~/.config/fts
(Linux) or ~/Library/Application Support/fts (macOS).
Never prints the token.
"""
import json, os, sys, urllib.request

inst = os.environ.get("STUDIO_INSTANCE", "")
name = f"studio-{inst}" if inst else "studio"
# Linux: ~/.config/fts; macOS: ~/Library/Application Support/fts.
dirs = [os.path.expanduser("~/.config/fts"), os.path.expanduser("~/Library/Application Support/fts")]
path = next(p for p in (os.path.join(d, f"{name}-mcp.json") for d in dirs) if os.path.exists(p))
cfg = json.load(open(path))

def call(method, params=None, sid=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}).encode()
    h = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream",
         "Authorization": "Bearer " + cfg["token"]}
    if sid:
        h["Mcp-Session-Id"] = sid
    r = urllib.request.urlopen(urllib.request.Request(cfg["url"], body, h), timeout=120)
    t = r.read().decode()
    s = r.headers.get("Mcp-Session-Id") or sid
    if t.startswith("event:") or "data:" in t[:20]:
        t = "\n".join(l[5:] for l in t.splitlines() if l.startswith("data:"))
    return (json.loads(t) if t.strip() else {}), s

_, sid = call("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                             "clientInfo": {"name": "agent", "version": "1"}})
try:
    call("notifications/initialized", sid=sid)
except Exception:
    pass

if sys.argv[1] == "list":
    r, _ = call("tools/list", sid=sid)
    for t in r["result"]["tools"]:
        print(t["name"], "-", t.get("description", "")[:110])
else:
    args = json.loads(sys.argv[3]) if len(sys.argv) > 3 else {}
    if sys.argv[1] == "run":
        args = {"id": sys.argv[2], "args": args}
    elif len(sys.argv) > 2:
        args = json.loads(sys.argv[2])
    r, _ = call("tools/call", {"name": sys.argv[1], "arguments": args}, sid=sid)
    print(json.dumps(r.get("result", r)))

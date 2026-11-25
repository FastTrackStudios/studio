#!/usr/bin/env python3
"""Save a screenshot of a running Studio window through app-shell's control socket.

    STUDIO_INSTANCE=agent XDG_RUNTIME_DIR=/run/user/1000/agent-rt scripts/agent-instance/shot.py /tmp/s.png
"""
import os, socket, sys

inst = os.environ.get("STUDIO_INSTANCE", "")
name = f"studio-{inst}" if inst else "studio"
rt = os.environ.get("XDG_RUNTIME_DIR") or os.environ.get("TMPDIR", "/tmp")
s = socket.socket(socket.AF_UNIX)
s.settimeout(30)
s.connect(os.path.join(rt, "app-shell", f"{name}.sock"))
s.sendall(f"shot {os.path.abspath(sys.argv[1])}\n".encode())
print(s.recv(4096).decode().strip())

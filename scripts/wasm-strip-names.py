#!/usr/bin/env python3
"""Strips the function names and DWARF from every .wasm file under a
folder, in place: a split studio-web build (`dx build --wasm-split`) keeps
the symbols the splitter needs (dx sets `debug = true`), and the names
alone are about a third of the page's module, which no page reads.

    wasm-strip-names.py <folder>
"""

import os
import sys


def leb(b, i):
    r = s = 0
    while True:
        x = b[i]
        i += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            return r, i


def strip(path):
    b = open(path, "rb").read()
    if b[:4] != b"\0asm":
        return 0
    out, i = bytearray(b[:8]), 8
    while i < len(b):
        start = i
        sid = b[i]
        i += 1
        n, i = leb(b, i)
        end = i + n
        if sid == 0:
            length, j = leb(b, i)
            name = b[j:j + length]
            if name == b"name" or name.startswith(b".debug"):
                i = end
                continue
        out += b[start:end]
        i = end
    if len(out) < len(b):
        open(path, "wb").write(bytes(out))
    return len(b) - len(out)


def main(argv):
    if len(argv) != 2:
        sys.exit(__doc__)
    saved = 0
    for folder, _, files in os.walk(argv[1]):
        for f in files:
            if f.endswith(".wasm"):
                saved += strip(os.path.join(folder, f))
    print(f"wasm-strip-names: {saved / 1e6:.1f} MB of names and DWARF taken out", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv)

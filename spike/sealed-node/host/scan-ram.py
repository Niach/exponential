#!/usr/bin/env python3
"""Count occurrences of secrets in a guest RAM file (QEMU memory-backend-file,
share=on). Prints COUNTS only, never the secret.

usage: scan-ram.py <ram-file> <label>=<secret-file-or-@literal> ...
"""
import mmap
import sys

ram = sys.argv[1]
with open(ram, "rb") as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as m:
    size = len(m)
    for spec in sys.argv[2:]:
        label, src = spec.split("=", 1)
        if src.startswith("@"):
            needle = src[1:].encode()
        else:
            with open(src, "rb") as s:
                needle = s.read().strip()
        # The full secret plus a 24-byte slice from its middle: catches
        # partial copies (split buffers, truncated logs) as well.
        mid = needle[len(needle) // 2 - 12 : len(needle) // 2 + 12]
        hits = []
        for n in (needle, mid):
            c, i = 0, m.find(n)
            while i != -1:
                c += 1
                i = m.find(n, i + 1)
            hits.append(c)
        print(f"[scan] {ram} ({size >> 20} MiB): {label} len={len(needle)} full={hits[0]} middle24={hits[1]}", flush=True)

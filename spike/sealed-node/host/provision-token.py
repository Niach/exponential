#!/usr/bin/env python3
"""One-shot token provisioning: push the token file into the gateway VM over
vsock (host -> cid:port), newline-terminated, then exit. The gateway accepts
exactly one connection on that port and closes it, so nothing (including
the agent VM, which cannot reach the gateway's CID at all) can repeat it.

It dials only after the gateway printed its ready line on the serial log:
a vsock request that reaches Hermit before its listener exists wedges
Hermit's vsock RX for good (see RESULTS.md).

usage: provision-token.py <token-file> <gateway-cid> <serial-log> [port=9000] [timeout_s=30]
"""
import socket
import sys
import time

path, cid, serial = sys.argv[1], int(sys.argv[2]), sys.argv[3]
port = int(sys.argv[4]) if len(sys.argv) > 4 else 9000
deadline = time.monotonic() + (float(sys.argv[5]) if len(sys.argv) > 5 else 30)
ready = f"waiting for the token on vsock port {port}".encode()
while True:
    try:
        with open(serial, "rb") as f:
            if ready in f.read():
                break
    except FileNotFoundError:
        pass
    if time.monotonic() > deadline:
        sys.exit("[provision] gateway never printed its ready line")
    time.sleep(0.005)
while True:
    s = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
    try:
        s.connect((cid, port))
        break
    except OSError:
        s.close()
        if time.monotonic() > deadline:
            sys.exit(f"[provision] gateway cid {cid} port {port} never came up")
        time.sleep(0.02)
with open(path, "rb") as f:
    s.sendall(f.read().strip() + b"\n")
s.close()
print(f"[provision] token pushed to cid {cid}:{port}", flush=True)

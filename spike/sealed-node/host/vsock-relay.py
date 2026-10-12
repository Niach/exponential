#!/usr/bin/env python3
"""Host vsock relay: vhost-vsock only routes guest<->host, so the agent VM
reaches the gateway VM through this splice. It accepts ONLY the agent CID,
only ever dials the gateway CID, and reads nothing it forwards.

The gateway (Hermit) serves a POOL of vsock ports, one connection per port
at a time (Hermit's vsock has no accept queue), so the relay hands each
agent connection a free pool port and returns it when the splice ends.

usage: vsock-relay.py <listen> <gateway-cid> <pool-base> <pool-size>
  listen = vsock:<port>:<agent-cid>   (the node: only that CID is accepted)
         | tcp:<host>:<port>          (host-side gateway tests, no agent VM)
"""
import queue
import socket
import sys
import threading
import time

listen = sys.argv[1]
gw_cid, base, n = map(int, sys.argv[2:5])
free = queue.Queue()
for p in range(base, base + n):
    free.put(p)


def pump(a, b):
    try:
        while True:
            d = a.recv(65536)
            if not d:
                break
            b.sendall(d)
    except OSError:
        pass
    finally:
        try:
            b.shutdown(socket.SHUT_WR)
        except OSError:
            pass


def serve(c):
    port = free.get()  # blocks while every pool port is busy
    try:
        g = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
        g.settimeout(5)
        try:
            g.connect((gw_cid, port))
        except OSError as e:
            print(f"[relay] gateway {gw_cid}:{port} connect failed: {e}", flush=True)
            c.close()
            return
        g.settimeout(None)
        t = threading.Thread(target=pump, args=(g, c), daemon=True)
        t.start()
        pump(c, g)
        t.join()
        c.close()
        g.close()
    finally:
        time.sleep(0.02)  # let the gateway re-bind the port
        free.put(port)


kind, a, b = listen.split(":")
if kind == "vsock":
    agent_cid = int(b)
    srv = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((socket.VMADDR_CID_ANY, int(a)))
else:
    agent_cid = None
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((a, int(b)))
srv.listen(64)
print(f"[relay] {listen} -> cid {gw_cid}:{base}..{base + n - 1}", flush=True)
while True:
    c, peer = srv.accept()
    if agent_cid is not None and peer[0] != agent_cid:
        print(f"[relay] dropped connection from cid {peer[0]}", flush=True)
        c.close()
        continue
    threading.Thread(target=serve, args=(c,), daemon=True).start()

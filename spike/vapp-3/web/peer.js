// VAPP-3 spike: the browser side. Plain JS, no framework. Mirrors rust/peer-core:
//  - envelopes (signal.rs), signed bytes (signing.rs), bench protocol (bench.rs), byte for byte.
//
// Bench protocol: ONE ordered reliable channel `exp`, binary, first byte = opcode, little-endian:
//   0x01 PING [seq u32][client_ms f64] → 0x02 PONG (same payload)
//   0x03 BODY_START [total u64] · 0x04 BODY_CHUNK [bytes ≤ 16 KiB] · 0x05 BODY_END → 0x06 BODY_ACK [received u64]
//   0x07 REQUEST_BODY [total u64] (server streams START/CHUNK/END, client replies BODY_ACK) · 0x08 CLOSE
const OP = { PING: 1, PONG: 2, BODY_START: 3, BODY_CHUNK: 4, BODY_END: 5, BODY_ACK: 6, REQUEST_BODY: 7, CLOSE: 8 }
const CHUNK = 16 * 1024
const HIGH_WATER = 1024 * 1024
const LOW_WATER = 256 * 1024

const P = new URLSearchParams(location.search)
const cfg = {
  policy: P.get("policy") ?? "all",
  runs: Number(P.get("runs") ?? 20),
  source: P.get("source") ?? guessSource(),
  scenario: P.get("scenario") ?? "same-machine",
  turn: P.get("turn") ?? "192.168.178.71:3478",
  turnUser: P.get("turnUser") ?? "exp",
  turnPass: P.get("turnPass") ?? "spike",
  session: P.get("session") ?? "vapp3-dev",
  room: P.get("room") ?? "dev",
  auto: P.get("auto") === "1",
  tamper: P.get("tamper"), // sdp | sig | control | answer (daemon runs --tamper-sdp; WE must reject)
  bodyBytes: Number(P.get("body") ?? 2_000_000),
  pings: Number(P.get("pings") ?? 100),
  daemonPolicy: P.get("daemonPolicy") ?? "all",
  notes: P.get("notes") ?? "",
}
const rows = []
window.__vapp3 = { rows, done: false, cfg, errors: [] }

function guessSource() {
  return /Chrome\//.test(navigator.userAgent) ? "web-chromium" : "web-safari"
}

const $ = (id) => document.getElementById(id)
function log(...a) {
  const s = a.map((x) => (typeof x === "string" ? x : JSON.stringify(x))).join(" ")
  console.log(s)
  $("log").textContent += `${s}\n`
}
const status = (s) => ($("status").textContent = s)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const now = () => performance.now()

// ---------------------------------------------------------------- b64url + ed25519 (WebCrypto)
const b64u = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")
const unb64u = (s) => Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4)), (c) => c.charCodeAt(0))
const enc = new TextEncoder()

let identity = null
async function getIdentity() {
  if (identity) return identity
  const kp = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"])
  const pub = b64u(await crypto.subtle.exportKey("raw", kp.publicKey))
  identity = { kp, pub }
  return identity
}
const signedBytes = (hash, fp, ufrag, session, from) =>
  enc.encode(`exp-dtls-fp-v1\n${hash.toLowerCase()}\n${fp.toUpperCase()}\n${ufrag}\n${session}\n${from}`)
async function sign(msg) {
  const id = await getIdentity()
  return b64u(await crypto.subtle.sign({ name: "Ed25519" }, id.kp.privateKey, msg))
}
async function verify(pubB64, sigB64, msg) {
  try {
    const key = await crypto.subtle.importKey("raw", unb64u(pubB64), { name: "Ed25519" }, false, ["verify"])
    return await crypto.subtle.verify({ name: "Ed25519" }, key, unb64u(sigB64), msg)
  } catch (e) {
    return false
  }
}
function sdpFingerprint(sdp) {
  const m = sdp.match(/^a=fingerprint:(\S+) (\S+)\r?$/m)
  return m ? [m[1].toLowerCase(), m[2].toUpperCase()] : null
}
const sdpUfrag = (sdp) => sdp.match(/^a=ice-ufrag:(\S+)\r?$/m)?.[1] ?? null
function tamperFingerprint(sdp) {
  return sdp.replace(/^(a=fingerprint:\S+ )([0-9A-Fa-f])/m, (_, pre, c) => pre + (c === "0" ? "1" : "0"))
}

// ---------------------------------------------------------------- relay (steer relay viewer)
class Relay {
  constructor() {
    this.daemon = null // {id, pub}
    this.waiters = new Map() // peerId -> [envelope queue, resolve?]
    this.synced = false
  }
  async open() {
    const t = await (await fetch(`/tickets?room=${cfg.room}`)).json()
    this.ws = new WebSocket(`${t.relay}/ws?ticket=${t.viewer}`)
    await new Promise((res, rej) => {
      this.ws.onopen = res
      this.ws.onerror = () => rej(new Error("relay ws error"))
    })
    this.ws.onmessage = (e) => this.onFrame(e.data)
    this.ws.onclose = () => log("relay closed")
    this.ws.send(JSON.stringify({ t: "join", channel: "activity" }))
    let replayHello = null
    this.onHello = (h) => { if (!this.synced) replayHello = h; else this.daemon = h }
    this.onSynced = () => { if (replayHello) this.daemon = replayHello }
    const t0 = now()
    while (!this.daemon && now() - t0 < 20000) await sleep(50)
    if (!this.daemon) throw new Error("no daemon hello in the room")
    log(`daemon ${this.daemon.id} pub ${this.daemon.pub}`)
  }
  onFrame(data) {
    let f
    try { f = JSON.parse(data) } catch { return }
    if (f.t === "activity_synced" || f.t === "history_pending") { this.synced = true; this.onSynced?.(); return }
    if (f.t !== "activity" || f.event?.kind !== "narration") return
    let env
    try { env = JSON.parse(f.event.text) } catch { return }
    if (env.v !== 1) return
    if (env.type === "hello") { this.onHello?.({ id: env.from, pub: env.pub }); return }
    if (!env.to) return
    const w = this.waiters.get(env.to)
    if (!w) return
    w.queue.push(env)
    w.wake?.()
  }
  send(env) {
    const data = JSON.stringify(env)
    if (data.length > 8192) log(`WARN envelope ${data.length} B > 8 KiB input cap`)
    this.ws.send(JSON.stringify({ t: "input", data }))
  }
  register(peerId) { const w = { queue: [], wake: null }; this.waiters.set(peerId, w); return w }
  unregister(peerId) { this.waiters.delete(peerId) }
  async next(w, types, timeoutMs) {
    const deadline = now() + timeoutMs
    for (;;) {
      const i = w.queue.findIndex((e) => types.includes(e.type))
      if (i >= 0) return w.queue.splice(i, 1)[0]
      const left = deadline - now()
      if (left <= 0) return null
      await new Promise((r) => { w.wake = r; setTimeout(r, Math.min(left, 250)) })
      w.wake = null
    }
  }
}

// ---------------------------------------------------------------- data channel helpers
class Chan {
  constructor(dc) {
    this.dc = dc
    this.q = []
    this.wake = null
    dc.binaryType = "arraybuffer"
    dc.bufferedAmountLowThreshold = LOW_WATER
    dc.onmessage = (e) => { this.q.push(new Uint8Array(e.data)); this.wake?.() }
  }
  async send(buf) {
    if (this.dc.bufferedAmount > HIGH_WATER) {
      await new Promise((r) => { this.dc.onbufferedamountlow = () => { this.dc.onbufferedamountlow = null; r() } })
    }
    this.dc.send(buf)
  }
  async recv(timeoutMs) {
    const deadline = now() + timeoutMs
    while (!this.q.length) {
      const left = deadline - now()
      if (left <= 0 || this.dc.readyState === "closed") return null
      await new Promise((r) => { this.wake = r; setTimeout(r, Math.min(left, 250)) })
      this.wake = null
    }
    return this.q.shift()
  }
  async recvOp(op, timeoutMs) {
    const deadline = now() + timeoutMs
    for (;;) {
      const m = await this.recv(deadline - now())
      if (!m) throw new Error(`timeout waiting for op ${op}`)
      if (m[0] === op) return m
    }
  }
}
function u64msg(op, n) { const b = new ArrayBuffer(9); const v = new DataView(b); v.setUint8(0, op); v.setBigUint64(1, BigInt(n), true); return b }
const readU64 = (m) => Number(new DataView(m.buffer, m.byteOffset, m.byteLength).getBigUint64(1, true))
function pingMsg(seq, ms) { const b = new ArrayBuffer(13); const v = new DataView(b); v.setUint8(0, OP.PING); v.setUint32(1, seq, true); v.setFloat64(5, ms, true); return b }
const readSeq = (m) => new DataView(m.buffer, m.byteOffset, m.byteLength).getUint32(1, true)
const pct = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor(p * (s.length - 1) + 0.5)] }

async function bench(ch) {
  const r = {}
  const rtts = []
  for (let seq = 0; seq < cfg.pings; seq++) {
    const t0 = now()
    await ch.send(pingMsg(seq, t0))
    for (;;) { const m = await ch.recvOp(OP.PONG, 5000); if (readSeq(m) === seq) break }
    rtts.push(now() - t0)
  }
  r.rttP50Ms = pct(rtts, 0.5)
  r.rttP95Ms = pct(rtts, 0.95)
  // upload
  let t0 = now()
  await ch.send(u64msg(OP.BODY_START, cfg.bodyBytes))
  const chunk = new Uint8Array(CHUNK + 1).fill(0xa5)
  chunk[0] = OP.BODY_CHUNK
  for (let left = cfg.bodyBytes; left > 0; left -= CHUNK) {
    const n = Math.min(left, CHUNK)
    await ch.send(n === CHUNK ? chunk : chunk.slice(0, n + 1))
  }
  await ch.send(new Uint8Array([OP.BODY_END]))
  const ack = await ch.recvOp(OP.BODY_ACK, 60000)
  r.upMbps = (cfg.bodyBytes * 8) / ((now() - t0) / 1000) / 1e6
  if (readU64(ack) !== cfg.bodyBytes) r.error = `upload ack ${readU64(ack)} != ${cfg.bodyBytes}`
  // download
  t0 = now()
  await ch.send(u64msg(OP.REQUEST_BODY, cfg.bodyBytes))
  let received = 0
  for (;;) {
    const m = await ch.recv(60000)
    if (!m) throw new Error("download timeout")
    if (m[0] === OP.BODY_CHUNK) received += m.length - 1
    else if (m[0] === OP.BODY_END) break
  }
  r.downMbps = (received * 8) / ((now() - t0) / 1000) / 1e6
  await ch.send(u64msg(OP.BODY_ACK, received))
  if (received !== cfg.bodyBytes) r.error = `download ${received} != ${cfg.bodyBytes}`
  return r
}

async function selectedPair(pc) {
  const stats = await pc.getStats()
  let pair = null
  let transport = null
  stats.forEach((s) => { if (s.type === "transport") transport = s })
  if (transport?.selectedCandidatePairId) pair = stats.get(transport.selectedCandidatePairId)
  if (!pair) stats.forEach((s) => { if (s.type === "candidate-pair" && (s.selected || (s.nominated && s.state === "succeeded"))) pair = s })
  const l = pair && stats.get(pair.localCandidateId)
  const r = pair && stats.get(pair.remoteCandidateId)
  if (P.get("debugStats")) stats.forEach((s) => { if (/candidate|transport/.test(s.type)) log(`STAT ${JSON.stringify(s)}`) })
  const cert = transport?.remoteCertificateId ? stats.get(transport.remoteCertificateId) : null
  // A TURN-relayed local candidate can show up as `prflx` (url turn:…, relayProtocol set) when the
  // TURN server's outbound traffic is NATed again (coturn in Docker): classify it as relay.
  const viaTurn = l && (l.candidateType === "relay" || (l.url ?? "").startsWith("turn") || l.relayProtocol)
  return {
    localPath: viaTurn ? "relay" : l?.candidateType ?? null,
    localPathRaw: l?.candidateType ?? null,
    remotePath: r?.candidateType ?? null,
    localCandidate: l ? `${l.candidateType} ${l.address ?? l.ip}:${l.port} ${l.protocol}${l.relayProtocol ? ` via ${l.relayProtocol}` : ""}` : null,
    remoteCandidate: r ? `${r.candidateType} ${r.address ?? r.ip}:${r.port}` : null,
    remoteCert: cert ? `${cert.fingerprintAlgorithm} ${cert.fingerprint}`.toUpperCase() : null,
  }
}

// ---------------------------------------------------------------- one run
async function oneRun(relay, i) {
  const me = `${cfg.source}-${Math.random().toString(16).slice(2, 8)}-${i}`
  const w = relay.register(me)
  const daemon = relay.daemon
  const env = (type, extra) => ({ v: 1, from: me, to: daemon.id, sessionId: cfg.session, type, ...extra })
  const base = { kind: "bench", at: new Date().toISOString(), source: cfg.source, scenario: cfg.scenario, policy: cfg.policy,
    connectMs: null, rttP50Ms: null, rttP95Ms: null, upMbps: null, downMbps: null, bodyBytes: cfg.bodyBytes,
    localPath: null, remotePath: null, ok: false, error: null, reconnect: null, notes: cfg.notes, daemonPolicy: cfg.daemonPolicy, userAgent: navigator.userAgent }
  const iceServers = [{ urls: `stun:${cfg.turn}` }, { urls: `turn:${cfg.turn}?transport=udp`, username: cfg.turnUser, credential: cfg.turnPass }]
  const pc = new RTCPeerConnection({ iceServers, iceTransportPolicy: cfg.policy === "relay" ? "relay" : "all" })
  const ch = new Chan(pc.createDataChannel("exp", { ordered: true }))
  const opened = new Promise((res, rej) => { ch.dc.onopen = res; ch.dc.onerror = (e) => rej(new Error(`dc error ${e?.error?.message ?? ""}`)) })
  const pending = []
  let offerSent = false
  pc.onicecandidate = (e) => {
    const m = e.candidate ? env("candidate", { candidate: e.candidate.candidate, sdpMid: e.candidate.sdpMid ?? "0" }) : env("end_of_candidates", {})
    if (e.candidate && !e.candidate.candidate) return
    offerSent ? relay.send(m) : pending.push(m)
  }
  const tamperRow = (rejected, reason, variant = cfg.tamper) => ({ kind: "tamper", at: new Date().toISOString(), source: cfg.source, variant, rejected, reason, notes: cfg.notes })
  try {
    const t0 = now()
    await pc.setLocalDescription(await pc.createOffer())
    let sdp = pc.localDescription.sdp
    const [hash, fp] = sdpFingerprint(sdp)
    let sig = await sign(signedBytes(hash, fp, sdpUfrag(sdp), cfg.session, me))
    if (cfg.tamper === "sdp") sdp = tamperFingerprint(sdp)
    if (cfg.tamper === "sig") sig = (sig[0] === "A" ? "B" : "A") + sig.slice(1)
    relay.send(env("offer", { sdp, pub: (await getIdentity()).pub, sig }))
    offerSent = true
    pending.splice(0).forEach((m) => relay.send(m))
    const reply = await relay.next(w, ["answer", "reject"], 15000)
    if (!reply) throw new Error("no answer within 15s")
    if (reply.type === "reject") {
      if (cfg.tamper) return tamperRow(true, reply.reason)
      throw new Error(`rejected: ${reply.reason}`)
    }
    // Verify BEFORE applying: key must be the hello key, signature over the answer's fp + ufrag.
    const afp = sdpFingerprint(reply.sdp)
    const ok = reply.pub === daemon.pub && afp && (await verify(reply.pub, reply.sig, signedBytes(afp[0], afp[1], sdpUfrag(reply.sdp), cfg.session, reply.from)))
    if (!ok) {
      const reason = reply.pub !== daemon.pub ? "unexpected_peer_key" : "fingerprint_signature_invalid"
      relay.send(env("reject", { reason }))
      if (cfg.tamper) return tamperRow(true, `answer rejected by browser: ${reason}`, cfg.tamper === "answer" ? "sdp" : cfg.tamper)
      throw new Error(`answer ${reason}`)
    }
    if (cfg.tamper === "answer") return tamperRow(false, "tampered answer was NOT caught", "sdp")
    await pc.setRemoteDescription({ type: "answer", sdp: reply.sdp })
    // Late candidates from the daemon (str0m puts all of its candidates in the SDP; handle trickle anyway).
    const pump = (async () => {
      for (;;) {
        const c = await relay.next(w, ["candidate"], 1000)
        if (pc.connectionState === "closed") return
        if (c) await pc.addIceCandidate({ candidate: c.candidate, sdpMid: c.sdpMid ?? "0" }).catch(() => {})
      }
    })()
    await Promise.race([opened, sleep(20000).then(() => { throw new Error(`channel not open in 20s (ice ${pc.iceConnectionState})`) })])
    const connectMs = now() - t0
    if (cfg.tamper) return tamperRow(false, "accepted: channel open")
    const r = await bench(ch)
    const pair = await selectedPair(pc)
    const fpOk = pair.remoteCert ? pair.remoteCert === `${afp[0]} ${afp[1]}`.toUpperCase() : null
    await ch.send(new Uint8Array([OP.CLOSE]))
    const view = await relay.next(w, ["result"], 3000)
    void pump
    return { ...base, ...r, connectMs, localPath: pair.localPath, localPathRaw: pair.localPathRaw, remotePath: pair.remotePath, localCandidate: pair.localCandidate,
      remoteCandidate: pair.remoteCandidate, dtlsFingerprintMatchesSdp: fpOk, ok: !r.error, error: r.error ?? null,
      daemonView: view?.payload?.daemonView ?? null }
  } catch (e) {
    if (cfg.tamper) return tamperRow(false, `error: ${e.message}`)
    return { ...base, error: String(e.message ?? e), notes: `${cfg.notes} ice=${pc.iceConnectionState} conn=${pc.connectionState}`.trim() }
  } finally {
    try { relay.send(env("bye", {})) } catch {}
    pc.close()
    relay.unregister(me)
  }
}

function render() {
  const b = rows.filter((r) => r.kind === "bench")
  const ok = b.filter((r) => r.ok)
  const f = (x) => (x == null || Number.isNaN(x) ? "–" : x.toFixed(2))
  const m = (k, p) => (ok.length ? pct(ok.map((r) => r[k]), p) : NaN)
  let h = `<table><tr><th>#</th><th>kind</th><th>ok</th><th>connect ms</th><th>rtt p50</th><th>rtt p95</th><th>up Mbps</th><th>down Mbps</th><th>path</th><th>error/reason</th></tr>`
  rows.forEach((r, i) => {
    h += `<tr><td>${i}</td><td>${r.kind}</td><td>${r.kind === "tamper" ? `rejected=${r.rejected}` : r.ok}</td><td>${f(r.connectMs)}</td><td>${f(r.rttP50Ms)}</td><td>${f(r.rttP95Ms)}</td><td>${f(r.upMbps)}</td><td>${f(r.downMbps)}</td><td>${r.localPath ?? ""}↔${r.remotePath ?? ""}</td><td>${r.error ?? r.reason ?? ""}</td></tr>`
  })
  h += `</table><p>ok ${ok.length}/${b.length} · connect p50 ${f(m("connectMs", 0.5))} p95 ${f(m("connectMs", 0.95))} · rtt p50 ${f(m("rttP50Ms", 0.5))} · up ${f(m("upMbps", 0.5))} · down ${f(m("downMbps", 0.5))}</p>`
  $("summary").innerHTML = h
}

async function main() {
  status("connecting relay")
  const relay = new Relay()
  await relay.open()
  for (let i = 0; i < cfg.runs; i++) {
    status(`run ${i + 1}/${cfg.runs}`)
    const row = await oneRun(relay, i)
    rows.push(row)
    log(JSON.stringify(row))
    await fetch("/results", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(row) })
    render()
    await sleep(300)
  }
  status("done")
  window.__vapp3.done = true
  document.title = "done"
}

$("go").onclick = () => main().catch((e) => { log(`FATAL ${e.message}`); window.__vapp3.errors.push(String(e)); document.title = "done" })
if (cfg.auto) $("go").click()

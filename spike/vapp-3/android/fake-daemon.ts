// Relay smoke publisher for PeerHello (NOT the real daemon): opens the room, publishes a `hello`
// envelope and logs every viewer `input` envelope. Proves the phone/emulator reaches the relay.
//   bun spike/vapp-3/android/fake-daemon.ts [--relay ws://localhost:4002] [--seconds 120]
import { readFileSync } from "node:fs"
const args = new Map<string, string>()
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i].replace(/^--/, ""), process.argv[i + 1] ?? "")
const t = JSON.parse(readFileSync(new URL("../results/.tickets-dev.json", import.meta.url), "utf8"))
const relay = (args.get("relay") ?? "ws://localhost:4002").replace(/\/$/, "")
const seconds = Number(args.get("seconds") ?? 120)
const ws = new WebSocket(`${relay}/ws?ticket=${t.publisher}`)
let seq = 0
const publish = (env: object) =>
  ws.send(JSON.stringify({ t: "activity", seq: ++seq, event: { kind: "narration", text: JSON.stringify(env), messageId: `vapp3-${seq}` } }))
ws.onopen = () => {
  ws.send(JSON.stringify({ t: "hello", sessionId: t.sessionId }))
  publish({ v: 1, from: "fake-daemon", sessionId: t.sessionId, type: "hello", pub: "fake", version: "fake-daemon" })
  console.log("publisher up, hello published")
}
ws.onmessage = (m) => {
  const f = JSON.parse(String(m.data))
  if (f.t === "input") console.log("INPUT", String(f.data).slice(0, 300))
  else console.log("frame", String(m.data).slice(0, 200))
}
ws.onclose = (e) => console.log("closed", e.code, e.reason)
setTimeout(() => process.exit(0), seconds * 1000)

// Mint a publisher ticket (daemon) + N viewer tickets (browser/phones) for ONE scratch room on a
// steer relay. The relay never checks the DB: any signed sessionId opens a room.
//   bun spike/vapp-3/scripts/mint-tickets.ts [--secret dev-steer-secret] [--relay ws://localhost:4002] [--ttl 43200] [--session vapp3-xxxx]
// Prints JSON {sessionId, relay, publisher, viewer, publisherUrl, viewerUrl}.
import { signSteerTicket } from "@exp/steer-ticket"

const args = new Map<string, string>()
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i].replace(/^--/, ""), process.argv[i + 1] ?? "")
const secret = args.get("secret") ?? process.env.STEER_RELAY_SECRET ?? "dev-steer-secret"
const relay = (args.get("relay") ?? process.env.STEER_RELAY_URL ?? "ws://localhost:4002").replace(/\/$/, "")
const ttl = Number(args.get("ttl") ?? 12 * 3600)
const sessionId = args.get("session") ?? `vapp3-${Math.random().toString(36).slice(2, 8)}`
const now = Math.floor(Date.now() / 1000)
const base = { sub: "vapp3-spike", team: "vapp3", sessionId, iat: now, exp: now + ttl }
const publisher = signSteerTicket({ ...base, role: "publisher" }, secret)
const viewer = signSteerTicket({ ...base, role: "viewer" }, secret)
console.log(JSON.stringify({
  sessionId, relay, ttlSeconds: ttl,
  publisher, viewer,
  publisherUrl: `${relay}/ws?ticket=${publisher}`,
  viewerUrl: `${relay}/ws?ticket=${viewer}`,
}, null, 2))

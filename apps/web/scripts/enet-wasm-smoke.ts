// EXP-1457 smoke: the browser build of eNet (public/enet/, from
// scripts/build-enet-wasm.sh) runs the web install glue (WebInstall: stored
// identity, signed registration, allowlist from `installs.list`) and talks to
// a NATIVE eNet node through a local `iroh-relay --dev`, relay-only like a
// browser. Two installs of one team exchange acked chat pings both ways; an
// install of another team is refused by both.
//
//   cargo install iroh-relay --version <Cargo.lock> --features server --locked
//   (cd apps/desktop && cargo build -p enet-core --example relay_peer)
//   IROH_RELAY=…/iroh-relay RELAY_PEER=…/relay_peer bun scripts/enet-wasm-smoke.ts
//
// Bun gives the wasm what a browser would (WebSocket, fetch, WebCrypto); the
// identity store is in memory here (IndexedDB in the app).

import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process"
import { createInterface } from "node:readline"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import type { IdentityStore, SealedIdentity } from "../src/lib/enet/identity-store"
import { relayTicket } from "../src/lib/enet/relay-ticket"
import { WebInstall, type EnetModule, type EnetNode, type InstallsApi } from "../src/lib/enet/web-install"
import { createPublicKey, verify } from "node:crypto"
import { installMessage, type InstallRegistration } from "../src/lib/enet/install-registration"
import fixture from "../../../packages/domain-contract/fixtures/install-register.json"

const RELAY_PORT = 3340
const RELAY_URL = `http://127.0.0.1:${RELAY_PORT}`
const WAIT_MS = 30_000

const children: ChildProcessWithoutNullStreams[] = []
const fail = (why: string): never => {
  for (const c of children) c.kill()
  console.error(`FAIL ${why}`)
  process.exit(1)
}
const timeout = <T>(p: Promise<T>, what: string) =>
  Promise.race([p, new Promise<T>((_, reject) => setTimeout(() => reject(new Error(`timed out: ${what}`)), WAIT_MS))])

type WasmNodeJs = EnetNode & {
  ticket: () => string
  online: (ms: number) => Promise<void>
  addTicket: (t: string) => string
  onEvent: (cb: (e: { kind: string; from?: string; peer?: string; body?: Uint8Array }) => void) => void
  sendChat: (to: string, body: Uint8Array) => Promise<{ id: Uint8Array }>
  events: Array<{ kind: string; from?: string; peer?: string; body?: Uint8Array }>
}

async function loadWasm() {
  const dir = join(import.meta.dir, `../public/enet`)
  const wasm = await import(join(dir, `exponential_net_wasm.js`))
  wasm.initSync({ module: readFileSync(join(dir, `exponential_net_wasm_bg.wasm`)) })
  const nodes: WasmNodeJs[] = []
  const enet: EnetModule = {
    generateIdentity: wasm.generateIdentity,
    pushPublicKey: wasm.pushPublicKey,
    identitySign: wasm.identitySign,
    startNode: async (relay, identity) => {
      const node = (await wasm.WasmNode.start(relay, identity)) as WasmNodeJs
      await node.online(WAIT_MS)
      // A node has ONE event consumer and WebInstall takes it: tee every
      // event into \`node.events\` for the checks below.
      const consume = node.onEvent.bind(node)
      node.events = []
      node.onEvent = (cb) => consume((e) => (node.events.push(e), cb(e)))
      nodes.push(node)
      return node
    },
  }
  return { enet, nodes }
}

/** The native peer (enet-core's relay_peer example) as a line protocol. */
async function nativePeer() {
  const proc = spawn(process.env.RELAY_PEER ?? `relay_peer`, [RELAY_URL])
  children.push(proc)
  proc.stderr.on(`data`, () => {})
  const lines: string[] = []
  const waiters: Array<() => void> = []
  createInterface({ input: proc.stdout }).on(`line`, (l) => {
    lines.push(l)
    for (const w of waiters.splice(0)) w()
  })
  const expectLine = (prefix: string) =>
    timeout(
      new Promise<string>((resolve) => {
        const look = () => {
          const i = lines.findIndex((l) => l.startsWith(prefix))
          if (i >= 0) resolve(lines.splice(i, 1)[0]!)
          else waiters.push(look)
        }
        look()
      }),
      `native ${prefix}`
    )
  const [, endpointId, ticket] = (await expectLine(`READY `)).split(` `)
  return { endpointId: endpointId!, ticket: ticket!, send: (l: string) => proc.stdin.write(`${l}\n`), expectLine }
}

function memoryStore(): IdentityStore {
  const map = new Map<string, SealedIdentity>()
  return { get: async (k) => map.get(k), put: async (k, v) => void map.set(k, v), delete: async (k) => void map.delete(k) }
}

async function main() {
  const relay = spawn(process.env.IROH_RELAY ?? `iroh-relay`, [`--dev`])
  children.push(relay)
  relay.stderr.on(`data`, () => {})
  relay.stdout.on(`data`, () => {})
  for (let i = 0; ; i++) {
    if (await fetch(`${RELAY_URL}/ping`).then((r) => r.ok, () => false)) break
    if (i > 100) fail(`relay did not come up`)
    await Bun.sleep(100)
  }

  const desktop = await nativePeer()
  const { enet, nodes } = await loadWasm()

  // A fake registry: team T = {web install, desktop}; team X = {foreign}.
  const registered: InstallRegistration[] = []
  const teams: Record<string, string[]> = { T: [desktop.endpointId], X: [] }
  const api = (team: string): InstallsApi => ({
    register: async (input) => {
      registered.push(input)
      teams[team]!.push(input.endpointId)
      return { installId: input.endpointId }
    },
    list: async (teamId) => (teams[teamId] ?? []).map((endpointId) => ({ endpointId })),
    revoke: async () => ({}),
  })

  const web = new WebInstall(`user-web`, { loadEnet: async () => enet, api: api(`T`), store: memoryStore(), relayUrl: RELAY_URL })
  await web.setTeams([`T`])
  await timeout(web.start(), `web install start`)
  const foreign = new WebInstall(`user-foreign`, { loadEnet: async () => enet, api: api(`X`), store: memoryStore(), relayUrl: RELAY_URL })
  await foreign.setTeams([`X`])
  await timeout(foreign.start(), `foreign install start`)
  const [webNode, foreignNode] = nodes as [WasmNodeJs, WasmNodeJs]

  if (registered.length !== 2 || registered.some((r) => r.kind !== `web` || r.signature.length !== 86))
    fail(`registrations: ${JSON.stringify(registered)}`)
  // The real wasm signature verifies the way the server checks it.
  for (const r of registered) {
    const key = createPublicKey({
      key: Buffer.concat([Buffer.from(`302a300506032b6570032100`, `hex`), Buffer.from(r.endpointId, `hex`)]),
      format: `der`,
      type: `spki`,
    })
    const userId = r === registered[0] ? `user-web` : `user-foreign`
    const message = installMessage(userId, r.endpointId, r.x25519Public, r.signedAt)
    if (!verify(null, Buffer.from(message), key, Buffer.from(r.signature, `base64url`))) fail(`signature does not verify`)
  }
  // The wasm replays the shared vector (fixtures/install-register.json).
  const fixed = new Uint8Array(Buffer.from(fixture.identity, `hex`))
  if (Buffer.from(enet.pushPublicKey(fixed)).toString(`base64url`) !== fixture.x25519Public) fail(`pushPublicKey drift`)
  const fixedSig = Buffer.from(enet.identitySign(fixed, new TextEncoder().encode(fixture.message))).toString(`hex`)
  if (fixedSig !== fixture.signatureHex) fail(`identitySign drift: ${fixedSig}`)
  console.log(`ok wasm signatures verify + replay install-register.json`)

  if (!web.allowlist.has(desktop.endpointId) || web.allowlist.size !== 1) fail(`web allowlist ${[...web.allowlist]}`)

  const webEvents = webNode.events
  const webEvent = (pred: (e: (typeof webEvents)[number]) => boolean, what: string) =>
    timeout(
      (async () => {
        for (;;) {
          const e = webEvents.find(pred)
          if (e) return e
          await Bun.sleep(50)
        }
      })(),
      what
    )

  // The desktop allows its team's installs (what its own installs.list gives).
  desktop.send(`allow ${webNode.endpointId}`)
  desktop.send(`ticket ${webNode.ticket()}`)
  // EXP-1471: the web learned the desktop's address from its allowlist
  // (relayTicket), no ticket handed over; the foreign node needs one.
  if (relayTicket(webNode.endpointId, RELAY_URL) !== webNode.ticket()) fail(`relayTicket drift: ${webNode.ticket()}`)
  foreignNode.addTicket(desktop.ticket)

  // web → desktop: acked.
  await timeout(webNode.sendChat(desktop.endpointId, new TextEncoder().encode(`ping from web`)), `web → desktop ack`)
  const got = await desktop.expectLine(`CHAT ${webNode.endpointId} `)
  if (!got.endsWith(`ping from web`)) fail(`desktop got ${got}`)
  console.log(`ok web → desktop acked`)

  // desktop → web: acked.
  desktop.send(`send ${webNode.endpointId} ping from desktop`)
  await desktop.expectLine(`ACKED ${webNode.endpointId}`)
  const chat = await webEvent((e) => e.kind === `chat`, `web chat event`)
  if (chat.from !== desktop.endpointId || new TextDecoder().decode(chat.body) !== `ping from desktop`)
    fail(`web got ${JSON.stringify(chat)}`)
  console.log(`ok desktop → web acked`)

  // foreign → desktop: refused.
  const refusedAtDesktop = await foreignNode
    .sendChat(desktop.endpointId, new TextEncoder().encode(`let me in`))
    .then(() => false, (e: Error) => /refused|403/i.test(String(e?.message ?? e)))
  if (!refusedAtDesktop) fail(`foreign reached the desktop`)
  await desktop.expectLine(`REFUSED ${foreignNode.endpointId}`)
  console.log(`ok foreign → desktop refused`)

  // foreign → web: refused (the web node's allowlist has only its team).
  foreignNode.addTicket(webNode.ticket())
  const refusedAtWeb = await foreignNode
    .sendChat(webNode.endpointId, new TextEncoder().encode(`let me in`))
    .then(() => false, (e: Error) => /refused|403/i.test(String(e?.message ?? e)))
  if (!refusedAtWeb) fail(`foreign reached the web install`)
  await webEvent((e) => e.kind === `peerRefused` && e.peer === foreignNode.endpointId, `web peerRefused`)
  if (webEvents.some((e) => e.kind === `chat` && e.from === foreignNode.endpointId)) fail(`foreign chat delivered`)
  console.log(`ok foreign → web refused`)

  await web.stop()
  await foreign.stop()
  desktop.send(`quit`)
  for (const c of children) c.kill()
  console.log(`PASS`)
  process.exit(0)
}

main().catch((e) => fail(String(e?.stack ?? e)))

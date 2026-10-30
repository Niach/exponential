// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  forgetIdentity,
  loadOrCreateIdentity,
  type IdentityStore,
  type SealedIdentity,
} from "./identity-store"
import {
  ALLOWLIST_REFRESH_MS,
  ENET_RELAY_URL,
  WebInstall,
  type EnetModule,
  type EnetNode,
  type InstallsApi,
} from "./web-install"

// EXP-1457 step 1: the browser install's lifecycle against a fake wasm
// module and a fake installs API (the real wasm node is exercised by
// scripts/enet-wasm-smoke.ts).

function memoryStore(): IdentityStore & { map: Map<string, SealedIdentity> } {
  const map = new Map<string, SealedIdentity>()
  return {
    map,
    get: async (k) => map.get(k),
    put: async (k, v) => void map.set(k, v),
    delete: async (k) => void map.delete(k),
  }
}

let counter = 0
function fakeEnet() {
  const nodes: Array<EnetNode & { allowed: Set<string>; down: boolean; identity: string; relay: string }> = []
  const enet: EnetModule = {
    generateIdentity: () => new Uint8Array(32).fill(++counter),
    pushPublicKey: () => new Uint8Array(32).fill(7),
    identitySign: () => new Uint8Array(64).fill(9),
    startNode: async (relay, identity) => {
      const allowed = new Set<string>()
      const node = {
        endpointId: Buffer.from(identity).toString(`hex`),
        identity: Buffer.from(identity).toString(`hex`),
        relay,
        allowed,
        down: false,
        allowPeer: (id: string) => void allowed.add(id),
        revokePeer: (id: string) => void allowed.delete(id),
        shutdown: async () => {
          node.down = true
        },
      }
      nodes.push(node)
      return node
    },
  }
  return { enet, nodes }
}

function fakeApi(teams: Record<string, string[]>) {
  const api = {
    register: vi.fn(async () => ({ installId: `install-1` })),
    list: vi.fn(async (teamId: string) => (teams[teamId] ?? []).map((endpointId) => ({ endpointId }))),
    revoke: vi.fn(async () => ({})),
  } satisfies InstallsApi
  return api
}

const SELF = (n: number) => Buffer.from(new Uint8Array(32).fill(n)).toString(`hex`)

afterEach(() => {
  vi.useRealTimers()
})

describe(`identity store`, () => {
  it(`keeps one identity per account, sealed under a non-extractable key`, async () => {
    const store = memoryStore()
    const gen = vi.fn(() => new Uint8Array(32).fill(0xab))
    const first = await loadOrCreateIdentity(`u1`, gen, store)
    const again = await loadOrCreateIdentity(`u1`, gen, store)
    expect(gen).toHaveBeenCalledTimes(1)
    expect(again).toEqual(first)

    const sealed = store.map.get(`u1`)!
    expect(sealed.key.extractable).toBe(false)
    await expect(crypto.subtle.exportKey(`raw`, sealed.key)).rejects.toThrow()
    expect(Buffer.from(sealed.sealed).includes(Buffer.from(first))).toBe(false)

    await loadOrCreateIdentity(`u2`, () => new Uint8Array(32).fill(1), store)
    expect(store.map.size).toBe(2)
  })

  it(`mints a new identity after forget or an unreadable record`, async () => {
    const store = memoryStore()
    const first = await loadOrCreateIdentity(`u1`, () => new Uint8Array(32).fill(1), store)
    await forgetIdentity(`u1`, store)
    expect(await loadOrCreateIdentity(`u1`, () => new Uint8Array(32).fill(2), store)).not.toEqual(first)

    store.map.get(`u1`)!.iv = new Uint8Array(12)
    expect(await loadOrCreateIdentity(`u1`, () => new Uint8Array(32).fill(3), store)).toEqual(
      new Uint8Array(32).fill(3)
    )
  })
})

describe(`WebInstall`, () => {
  it(`starts relay-only on the stored identity and registers on every start`, async () => {
    const store = memoryStore()
    const { enet, nodes } = fakeEnet()
    const api = fakeApi({})
    const a = new WebInstall(`u1`, { loadEnet: async () => enet, api, store })
    await a.start()
    await a.stop()
    const b = new WebInstall(`u1`, { loadEnet: async () => enet, api, store })
    await b.start()

    expect(nodes.map((n) => n.relay)).toEqual([ENET_RELAY_URL, ENET_RELAY_URL])
    expect(nodes[0]!.identity).toBe(nodes[1]!.identity)
    expect(nodes[0]!.down).toBe(true)
    expect(api.register).toHaveBeenCalledTimes(2)
    const input = (api.register.mock.calls[0] as unknown as [Record<string, string>])[0]
    expect(input).toMatchObject({ kind: `web`, endpointId: nodes[0]!.endpointId })
    expect(input.signature).toHaveLength(86)
    expect(input.x25519Public).toHaveLength(43)
    await b.stop()
  })

  it(`allows every install of every team, refreshes on team change and every 10 minutes`, async () => {
    vi.useFakeTimers()
    const store = memoryStore()
    const { enet, nodes } = fakeEnet()
    const teams: Record<string, string[]> = { A: [`a1`, `a2`], B: [`b1`, `a2`] }
    const api = fakeApi(teams)
    const install = new WebInstall(`u1`, { loadEnet: async () => enet, api, store })
    await install.setTeams([`A`, `B`])
    await install.start()
    const node = nodes[0]!
    teams.A!.push(node.endpointId)
    await install.refresh()
    expect([...node.allowed].sort()).toEqual([`a1`, `a2`, `b1`])

    await install.setTeams([`A`])
    expect([...node.allowed].sort()).toEqual([`a1`, `a2`])

    teams.A = [`a1`, `a3`]
    await vi.advanceTimersByTimeAsync(ALLOWLIST_REFRESH_MS)
    expect([...node.allowed].sort()).toEqual([`a1`, `a3`])
    await install.stop()
    const calls = api.list.mock.calls.length
    await vi.advanceTimersByTimeAsync(ALLOWLIST_REFRESH_MS * 2)
    expect(api.list.mock.calls.length).toBe(calls)
  })

  it(`explicit sign-out revokes the install and forgets its identity`, async () => {
    const store = memoryStore()
    const { enet, nodes } = fakeEnet()
    const api = fakeApi({})
    const install = new WebInstall(`u1`, { loadEnet: async () => enet, api, store })
    await install.start()
    await install.stop({ revoke: true })
    expect(nodes[0]!.down).toBe(true)
    expect(api.revoke).toHaveBeenCalledWith(`install-1`)
    expect(store.map.has(`u1`)).toBe(false)
  })

  it(`a plain stop (session ended elsewhere) keeps the install`, async () => {
    const store = memoryStore()
    const { enet } = fakeEnet()
    const api = fakeApi({})
    const install = new WebInstall(`u1`, { loadEnet: async () => enet, api, store })
    await install.start()
    await install.stop()
    expect(api.revoke).not.toHaveBeenCalled()
    expect(store.map.has(`u1`)).toBe(true)
  })

  it(`stopping before the node is up never starts it`, async () => {
    const store = memoryStore()
    const { enet, nodes } = fakeEnet()
    const api = fakeApi({})
    let release!: () => void
    const gate = new Promise<void>((r) => (release = r))
    const install = new WebInstall(`u1`, {
      loadEnet: async () => {
        await gate
        return enet
      },
      api,
      store,
    })
    const started = install.start()
    await install.stop({ revoke: true })
    release()
    await started
    expect(nodes).toHaveLength(0)
    expect(api.register).not.toHaveBeenCalled()
    expect(api.revoke).not.toHaveBeenCalled()
  })

  it(`never allows itself`, async () => {
    const { enet, nodes } = fakeEnet()
    const install = new WebInstall(`u1`, { loadEnet: async () => enet, api: fakeApi({ A: [SELF(counter + 1)] }), store: memoryStore() })
    await install.setTeams([`A`])
    await install.start()
    expect(nodes[0]!.allowed.size).toBe(0)
  })
})

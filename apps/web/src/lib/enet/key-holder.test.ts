// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import pushFixture from "@exp/domain-contract/fixtures/push-seal.json"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { b64urlEncode, openBody, pushPublicKey, unwrapTreeKey } from "@exp/domain-contract/sealed-content"
import { deriveInstallSecret } from "./install-key"
import { KeyHolder, MISSING_RETRY_MS, type PendingPair, type SessionKeysApi } from "./key-holder"
import { memoryTreeKeyStore } from "./tree-key-store"

// EXP-1459: the browser key holder against fixtures/sealed-content.json
// (the install, its wrap, the body envelope) and a fake sessionKeys API.

const hex = (text: string) => Uint8Array.from(text.match(/../g)!.map((b) => parseInt(b, 16)))
const ROOT = `00000000-0000-4000-8000-000000000001`
const INSTALL = `00000000-0000-4000-8000-0000000000aa`
const OTHER = `00000000-0000-4000-8000-0000000000bb`

function fakeApi(over: Partial<SessionKeysApi> = {}): SessionKeysApi & { calls: Record<string, unknown[]> } {
  const calls: Record<string, unknown[]> = { mine: [], pending: [], wrap: [], create: [] }
  return {
    calls,
    mine: async (input) => {
      calls.mine!.push(input)
      return [{ rootId: ROOT, epoch: 1, wrapped: fixture.wrap.wrapped }]
    },
    pending: async (input) => {
      calls.pending!.push(input)
      return []
    },
    wrap: async (input) => {
      calls.wrap!.push(input)
      return { written: input.wraps.length, skipped: 0 }
    },
    create: async (input) => {
      calls.create!.push(input)
      return {}
    },
    ...over,
  }
}

describe(`deriveInstallSecret`, () => {
  it(`is PushKeypair::derive (push-seal.json derive)`, async () => {
    const secret = await deriveInstallSecret(hex(pushFixture.derive.identity))
    expect(Buffer.from(secret).toString(`hex`)).toBe(pushFixture.derive.secret)
    expect(Buffer.from(pushPublicKey(secret)).toString(`hex`)).toBe(pushFixture.derive.public)
  })
})

describe(`KeyHolder`, () => {
  it(`unwraps what sessionKeys.mine holds, opens the fixture body and caches the key`, async () => {
    const store = memoryTreeKeyStore()
    const holder = new KeyHolder(`u1`, INSTALL, hex(fixture.install.secret), { api: fakeApi(), store })
    const seen = vi.fn()
    holder.subscribe(seen)
    await holder.start()
    expect(seen).toHaveBeenCalled()
    const opened = openBody(fixture.body.envelope, holder.keysFor(ROOT))
    expect(opened).toEqual({ status: `opened`, epoch: 1, text: fixture.body.text })
    expect(holder.current(ROOT)).toEqual({ epoch: 1, publicKey: fixture.tree.publicKey })

    // A second tab (or a reload) opens offline from the cache.
    const offline = fakeApi({ mine: async () => Promise.reject(new Error(`offline`)) })
    const again = new KeyHolder(`u1`, INSTALL, hex(fixture.install.secret), { api: offline, store })
    await again.start().catch(() => {})
    expect(openBody(fixture.body.envelope, again.keysFor(ROOT)).status).toBe(`opened`)
    // Another account in the profile sees nothing.
    expect(await store.all(`u2`)).toEqual([])
  })

  it(`never keeps a wrap that is not for this install`, async () => {
    const holder = new KeyHolder(`u1`, INSTALL, hex(fixture.tree.secret), { api: fakeApi(), store: memoryTreeKeyStore() })
    await holder.start()
    expect(openBody(fixture.body.envelope, holder.keysFor(ROOT)).status).toBe(`noKey`)
  })

  it(`answers pending: wraps for readers of trees whose current key it holds`, async () => {
    const target = hex(`11`.repeat(32))
    const pairs: PendingPair[] = [
      { rootId: ROOT, epoch: 1, publicKey: fixture.tree.publicKey, targetInstallId: OTHER, targetX25519Public: b64urlEncode(pushPublicKey(target)) },
      // A public key the root does not carry with this secret: skipped.
      { rootId: ROOT, epoch: 1, publicKey: fixture.rotatedTree.publicKey, targetInstallId: OTHER, targetX25519Public: b64urlEncode(pushPublicKey(target)) },
      // A root it holds nothing for: skipped.
      { rootId: OTHER, epoch: 1, publicKey: fixture.tree.publicKey, targetInstallId: OTHER, targetX25519Public: b64urlEncode(pushPublicKey(target)) },
    ]
    const api = fakeApi({ pending: async () => pairs })
    const holder = new KeyHolder(`u1`, INSTALL, hex(fixture.install.secret), { api, store: memoryTreeKeyStore() })
    await holder.start()
    expect(await holder.answerPending()).toBe(1)
    const sent = api.calls.wrap[0] as { installId: string; wraps: Array<{ rootId: string; targetInstallId: string; wrapped: string }> }
    expect(sent.installId).toBe(INSTALL)
    expect(sent.wraps).toHaveLength(1)
    expect(Buffer.from(unwrapTreeKey(target, sent.wraps[0]!.wrapped)).toString(`hex`)).toBe(fixture.tree.secret)
  })

  it(`a new tree key is wrapped for every reader, this install included`, async () => {
    const holder = new KeyHolder(`u1`, INSTALL, hex(fixture.install.secret), { api: fakeApi(), store: memoryTreeKeyStore() })
    const other = hex(`22`.repeat(32))
    const key = holder.newTreeKey([
      { installId: INSTALL, x25519Public: fixture.install.x25519Public },
      { installId: OTHER, x25519Public: b64urlEncode(pushPublicKey(other)) },
      { installId: OTHER, x25519Public: b64urlEncode(pushPublicKey(other)) },
    ])
    expect(key.epoch).toBe(1)
    expect(key.wraps.map((w) => w.installId)).toEqual([INSTALL, OTHER])
    for (const [secret, wrap] of [[hex(fixture.install.secret), key.wraps[0]!], [other, key.wraps[1]!]] as const) {
      expect(unwrapTreeKey(secret, wrap.wrapped)).toEqual(key.secret)
    }
    expect(() => holder.newTreeKey([{ installId: OTHER, x25519Public: b64urlEncode(pushPublicKey(other)) }])).toThrow()
  })

  it(`createTree keys an existing root and holds the key`, async () => {
    const api = fakeApi({ mine: async () => [] })
    const holder = new KeyHolder(`u1`, INSTALL, hex(fixture.install.secret), { api, store: memoryTreeKeyStore() })
    const made = await holder.createTree(ROOT, [{ installId: INSTALL, x25519Public: fixture.install.x25519Public }])
    expect(api.calls.create[0]).toMatchObject({ rootId: ROOT, installId: INSTALL, epoch: 1, publicKey: made.publicKey })
    expect(holder.current(ROOT)).toEqual(made)
  })

  it(`asks for a missing root once per retry window, batched`, async () => {
    let now = 1_000
    const api = fakeApi({ mine: vi.fn(async () => []) })
    const holder = new KeyHolder(`u1`, INSTALL, hex(fixture.install.secret), { api, store: memoryTreeKeyStore(), now: () => now })
    holder.request(ROOT, 1)
    holder.request(OTHER, 1)
    holder.request(ROOT, 1)
    await new Promise((r) => setTimeout(r, 0))
    expect(api.mine).toHaveBeenCalledTimes(1)
    expect(api.mine).toHaveBeenCalledWith({ installId: INSTALL, rootIds: [ROOT, OTHER] })
    holder.request(ROOT, 1)
    await new Promise((r) => setTimeout(r, 0))
    expect(api.mine).toHaveBeenCalledTimes(1)
    now += MISSING_RETRY_MS
    holder.request(ROOT, 1)
    await new Promise((r) => setTimeout(r, 0))
    expect(api.mine).toHaveBeenCalledTimes(2)
  })
})

// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import pushFixture from "@exp/domain-contract/fixtures/push-seal.json"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { b64urlEncode, openBody, pushPublicKey, treeKeyFromSecret, unwrapTreeKey } from "@exp/domain-contract/sealed-content"
import { deriveInstallSecret } from "./install-key"
import { CachedKeys, KeyHolder, MISSING_RETRY_MS, type PendingPair, type SessionKeysApi } from "./key-holder"
import { currentKeyHolder, currentKeySource, keyVersion, setCachedKeys, setCurrentKeyHolder } from "./current-key-holder"
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
    due: async () => [],
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

// fixtures/sealed-content.json `coldStart`: the keys cached on the last run
// open content on the first render, before the install is resolved (no
// network), and a key landing later re-opens the locked row.
// #EXP-1951 (sealed-content.json `rotation.holderCases`): one holder pass
// against a SERVER DOUBLE that answers what each case says. The double may
// keep serving rows of a dropped install (`serves: foreign`); the holder
// wraps a new epoch for itself and for the pairs `pending` lists, nobody else.
describe(`rotation (#EXP-1951)`, () => {
  type Case = (typeof fixture.rotation.holderCases)[number] & { passes?: number; serves?: string }
  const SELF = INSTALL
  const idOf = (name: string) => (name === `self` ? SELF : `00000000-0000-4000-8000-${Buffer.from(name).toString(`hex`).padEnd(12, `0`).slice(0, 12)}`)
  const nameOf = (id: string, readers: string[]) => (id === SELF ? `self` : (readers.find((name) => idOf(name) === id) ?? id))
  const secretAt = (epoch: number) => hex(epoch === 1 ? fixture.tree.secret : fixture.rotatedTree.secret)
  const trpcError = (code: string) => Object.assign(new Error(code), { data: { code } })

  function double(c: Case) {
    const answers = [...c.create]
    const done = new Set<string>()
    const conflicted = new Map<string, number>()
    const created: Array<{ rootId: string; epoch: number; publicKey: string }> = []
    const wrapped = new Set<string>()
    const sent = { creates: [] as Array<{ rootId: string; epoch: number; wraps: string[] }>, wraps: [] as Array<{ rootId: string; epoch: number; to: string }> }
    const api: SessionKeysApi = {
      mine: async ({ rootIds }) => [
        ...(c.serves === `foreign` ? Object.keys(c.held).map((rootId) => ({ rootId, epoch: 1, wrapped: fixture.rotation.foreignWrap })) : []),
        ...[...conflicted].map(([rootId, epoch]) => ({ rootId, epoch, wrapped: fixture.rotation.rotatedWrap })),
      ].filter((row) => !rootIds || rootIds.includes(row.rootId)),
      due: async () => c.due.filter((root) => !done.has(root.rootId)),
      create: async (input) => {
        sent.creates.push({ rootId: input.rootId, epoch: input.epoch, wraps: input.wraps.map((wrap) => nameOf(wrap.installId, c.readers)) })
        // Its own wrap really is for this install.
        expect(treeKeyFromSecret(unwrapTreeKey(hex(fixture.install.secret), input.wraps[0]!.wrapped)).publicKey).toBe(input.publicKey)
        const answer = answers.shift() ?? `ok`
        if (answer === `blip`) throw new Error(`Failed to fetch`)
        if (answer === `CONFLICT`) {
          done.add(input.rootId)
          conflicted.set(input.rootId, input.epoch)
        }
        if (answer !== `ok`) throw trpcError(answer)
        done.add(input.rootId)
        created.push({ rootId: input.rootId, epoch: input.epoch, publicKey: input.publicKey })
        return {}
      },
      pending: async () =>
        created.flatMap((key) =>
          c.readers
            .filter((name) => !wrapped.has(`${key.rootId}\n${name}`))
            .map((name) => ({ ...key, targetInstallId: idOf(name), targetX25519Public: fixture.tree.publicKey }))
        ),
      wrap: async (input) => {
        for (const wrap of input.wraps) {
          const to = nameOf(wrap.targetInstallId, c.readers)
          wrapped.add(`${wrap.rootId}\n${to}`)
          sent.wraps.push({ rootId: wrap.rootId, epoch: wrap.epoch, to })
        }
        return {}
      },
    }
    return { api, sent }
  }

  for (const c of fixture.rotation.holderCases as Case[]) {
    it(c.name, async () => {
      const store = memoryTreeKeyStore()
      for (const [rootId, epochs] of Object.entries(c.held)) for (const epoch of epochs as number[]) await store.put(`u1`, { rootId, epoch, secret: secretAt(epoch) })
      const { api, sent } = double(c)
      const holder = new KeyHolder(`u1`, SELF, hex(fixture.install.secret), { api, store })
      await holder.start()
      for (let pass = 0; pass < (c.passes ?? 1); pass++) await holder.pass()
      expect(sent.creates).toEqual(c.creates)
      expect(sent.wraps).toEqual(c.wraps)
      const holds = Object.fromEntries(Object.keys({ ...c.held, ...c.holds }).map((rootId) => [rootId, [...holder.keysFor(rootId).keys()].sort()]).filter(([, epochs]) => (epochs as number[]).length > 0))
      expect(holds).toEqual(c.holds)
    })
  }

  // #EXP-2119 OLDER EPOCHS (`olderEpochCases`).
  for (const c of fixture.rotation.olderEpochCases) {
    it(c.name, async () => {
      const store = memoryTreeKeyStore()
      for (const epoch of c.held) await store.put(`u1`, { rootId: ROOT, epoch, secret: secretAt(epoch) })
      const target = hex(fixture.install.secret)
      const pair = {
        rootId: ROOT,
        epoch: c.pair.epoch,
        publicKey: c.pair.publicKey && fixture.tree.publicKey,
        targetInstallId: OTHER,
        targetX25519Public: fixture.install.x25519Public,
      }
      let asked = 0
      const api = fakeApi({ mine: async () => [], pending: async (input) => (expect(input.olderEpochs).toBe(true), asked++ === 0 ? [pair] : []) })
      const holder = new KeyHolder(`u1`, SELF, target, { api, store })
      await holder.start()
      await holder.answerPending()
      const wraps = (api.calls.wrap as Array<{ wraps: Array<{ epoch: number; wrapped: string }> }>).flatMap((call) => call.wraps)
      expect(wraps.map((wrap) => unwrapTreeKey(target, wrap.wrapped))).toEqual(c.wraps === null ? [] : [secretAt(c.wraps)])
    })
  }

  it(`an older server without \`due\` still gets its pending wraps answered`, async () => {
    const api = fakeApi({ due: async () => Promise.reject(Object.assign(new Error(`No procedure`), { data: { code: `NOT_FOUND` } })) })
    const holder = new KeyHolder(`u1`, SELF, hex(fixture.install.secret), { api, store: memoryTreeKeyStore() })
    await holder.start()
    await holder.pass()
    expect(api.calls.pending).toHaveLength(1)
    expect(api.calls.create).toHaveLength(0)
  })
})

describe(`cold start`, () => {
  for (const c of fixture.coldStart.cases) {
    it(c.name, async () => {
      expect(c.holder).toBe(`none`)
      const store = memoryTreeKeyStore()
      for (const [epoch, secret] of Object.entries(c.persisted)) {
        await store.put(`u1`, { rootId: ROOT, epoch: Number(epoch), secret: hex(secret as string) })
      }
      expect(currentKeyHolder()).toBeUndefined()
      const cache = await CachedKeys.load(`u1`, store)
      const before = keyVersion()
      setCachedKeys(cache)
      expect(keyVersion()).not.toBe(before)
      const first = openBody(c.body, currentKeySource()?.keysFor(ROOT) ?? new Map())
      expect(first.status).toBe(c.firstFrame)
      if (c.firstFrame === `opened`) expect(first.text).toBe(c.text)
      // The key arrives (the holder's first pass): the holder takes over.
      const offline = fakeApi({
        mine: async () => (c.arrives ? [{ rootId: ROOT, epoch: 1, wrapped: fixture.wrap.wrapped }] : []),
      })
      const holder = new KeyHolder(`u1`, INSTALL, hex(fixture.install.secret), { api: offline, store })
      await holder.loadCache()
      setCurrentKeyHolder(holder)
      setCachedKeys(undefined)
      await holder.refresh()
      const after = openBody(c.body, currentKeySource()?.keysFor(ROOT) ?? new Map())
      expect(after.status).toBe(c.after)
      if (c.after === `opened`) expect(after.text).toBe(c.text)
      setCurrentKeyHolder(undefined)
      holder.stop()
      cache.clear()
    })
  }
})

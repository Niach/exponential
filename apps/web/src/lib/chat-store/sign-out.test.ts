// @vitest-environment node
// #EDE-120 (chat-store.json SIGN-OUT): a deliberate sign-out stops the chat
// store's worker in every tab (the `exp-chat-store` BroadcastChannel) and
// deletes the account's OPFS files, each under its Web Lock, never another
// account's; an unmount (a session expiry) only stops it.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { deleteChatStoreFiles, type WipeDeps, type WipeDir } from "./sign-out"

vi.mock(`@/lib/collections`, () => ({ sessionCollection: { get: () => undefined } }))
vi.mock(`@/lib/enet/current-key-holder`, () => ({ currentKeySource: () => undefined, subscribeKeyChange: () => () => {} }))
vi.mock(`@/lib/enet/sealed-open`, () => ({ openBodyCached: () => null }))

/** One OPFS: dir path → file names (a nested dir is a key of its own). */
function memoryOpfs(files: string[]) {
  const tree = new Map<string, Set<string>>()
  const held = new Set<string>()
  const add = (path: string) => {
    const parts = path.split(`/`)
    for (let i = 0; i < parts.length; i++) {
      const parent = parts.slice(0, i).join(`/`)
      if (!tree.has(parent)) tree.set(parent, new Set())
      tree.get(parent)!.add(parts[i]!)
    }
  }
  files.forEach(add)
  const isDir = (path: string) => tree.has(path)
  const dir = (path: string): WipeDir => ({
    async getDirectoryHandle(name) {
      const child = path ? `${path}/${name}` : name
      if (!isDir(child)) throw Object.assign(new Error(`missing`), { name: `NotFoundError` })
      return dir(child)
    },
    async removeEntry(name) {
      const child = path ? `${path}/${name}` : name
      if (!tree.get(path)?.has(name)) throw Object.assign(new Error(`missing`), { name: `NotFoundError` })
      if (held.has(child)) throw Object.assign(new Error(`held`), { name: `NoModificationAllowedError` })
      if (isDir(child) && tree.get(child)!.size > 0) throw Object.assign(new Error(`not empty`), { name: `InvalidModificationError` })
      tree.get(path)!.delete(name)
      tree.delete(child)
    },
    async *entries() {
      for (const name of tree.get(path) ?? []) {
        const child = path ? `${path}/${name}` : name
        yield [name, { kind: isDir(child) ? `directory` : `file` }] as [string, { kind: string }]
      }
    },
  })
  const list = () => {
    const out: string[] = []
    const walk = (path: string) => {
      for (const name of tree.get(path) ?? []) {
        const child = path ? `${path}/${name}` : name
        if (isDir(child)) walk(child)
        else out.push(child)
      }
    }
    walk(``)
    return out.sort()
  }
  return { root: async () => dir(``), held, list, isDir }
}

function deps(opfs: ReturnType<typeof memoryOpfs>, locks: string[] = []): WipeDeps {
  return {
    root: opfs.root,
    lock: async (name, body) => {
      locks.push(name)
      return body()
    },
    backoffMs: 0,
  }
}

describe(`deleteChatStoreFiles`, () => {
  it(`deletes the account's logs under their locks and the emptied dir, never another account's`, async () => {
    const opfs = memoryOpfs([`chat-store/u1/r1.log`, `chat-store/u1/r2.log`, `chat-store/u10/r1.log`, `chat-store/u2/r1.log`, `other/u1/r1.log`])
    const locks: string[] = []
    expect(await deleteChatStoreFiles(`u1`, deps(opfs, locks))).toEqual([])
    expect(opfs.list()).toEqual([`chat-store/u10/r1.log`, `chat-store/u2/r1.log`, `other/u1/r1.log`])
    expect(opfs.isDir(`chat-store/u1`)).toBe(false)
    expect(locks.sort()).toEqual([`exp-chat-store:u1/r1.log`, `exp-chat-store:u1/r2.log`])
  })

  it(`a file still held is tried again, then left (with its dir)`, async () => {
    const opfs = memoryOpfs([`chat-store/u1/r1.log`, `chat-store/u1/r2.log`])
    opfs.held.add(`chat-store/u1/r2.log`)
    expect(await deleteChatStoreFiles(`u1`, { ...deps(opfs), tries: 3 })).toEqual([`chat-store/u1/r2.log`])
    expect(opfs.list()).toEqual([`chat-store/u1/r2.log`])
  })

  it(`nothing stored, or an unsafe account id, deletes nothing`, async () => {
    const opfs = memoryOpfs([`chat-store/u2/r1.log`])
    expect(await deleteChatStoreFiles(`u1`, deps(opfs))).toEqual([])
    expect(await deleteChatStoreFiles(`..`, deps(memoryOpfs([`chat-store/../r1.log`])))).toEqual([])
    expect(opfs.list()).toEqual([`chat-store/u2/r1.log`])
  })
})

class FakeWorker {
  static all: FakeWorker[] = []
  posted: { op: string; args?: unknown[] }[] = []
  terminated = false
  onmessage: ((event: MessageEvent) => void) | null = null
  constructor() {
    FakeWorker.all.push(this)
  }
  postMessage(message: { id: number; op: string; args?: unknown[] }) {
    this.posted.push({ op: message.op, args: message.args })
    queueMicrotask(() => this.onmessage?.({ data: { id: message.id, result: true } } as MessageEvent))
  }
  terminate() {
    this.terminated = true
  }
}

describe(`the sign-out broadcast`, () => {
  beforeEach(() => {
    vi.resetModules()
    FakeWorker.all = []
    vi.stubGlobal(`Worker`, FakeWorker)
  })
  afterEach(() => vi.unstubAllGlobals())

  const otherTab = () => new BroadcastChannel(`exp-chat-store`)
  const flush = () => new Promise((resolve) => setTimeout(resolve, 20))

  it(`another tab's sign-out of this account stops this tab's worker; another account's does not`, async () => {
    const client = await import(`./client`)
    client.startChatStore(`u1`)
    expect(await client.chatStoreCall(`meta`, `k`)).toBe(true)
    const [worker] = FakeWorker.all
    const tab = otherTab()
    tab.postMessage({ t: `signed-out`, account: `u2` })
    await flush()
    expect(worker!.terminated).toBe(false)
    tab.postMessage({ t: `signed-out`, account: `u1` })
    await flush()
    expect(worker!.terminated).toBe(true)
    expect(await client.chatStoreCall(`meta`, `k`)).toBeUndefined()
    tab.close()
    client.stopChatStore()
  })

  it(`a deliberate sign-out stops this tab's worker and tells every other tab`, async () => {
    const client = await import(`./client`)
    client.startChatStore(`u1`)
    await client.chatStoreCall(`meta`, `k`)
    const tab = otherTab()
    const heard: unknown[] = []
    tab.onmessage = (event) => heard.push(event.data)
    await client.signOutChatStore()
    await flush()
    expect(FakeWorker.all[0]!.terminated).toBe(true)
    expect(heard).toEqual([{ t: `signed-out`, account: `u1` }])
    tab.close()
  })

  it(`an unmount (a session expiry) stops the worker and tells nobody`, async () => {
    const client = await import(`./client`)
    client.startChatStore(`u1`)
    await client.chatStoreCall(`meta`, `k`)
    const tab = otherTab()
    const heard: unknown[] = []
    tab.onmessage = (event) => heard.push(event.data)
    client.stopChatStore()
    await flush()
    expect(FakeWorker.all[0]!.terminated).toBe(true)
    expect(heard).toEqual([])
    tab.close()
  })

  // A chat's first rows arrive before its layout's effect starts the store
  // (a child's effects run first): they are kept and stored at the start.
  it(`messages a chat loaded before the start are stored once it starts`, async () => {
    const client = await import(`./client`)
    client.recordMessages([{ id: `m1`, sessionId: `s1`, rootId: `r1`, authorKind: `agent`, body: `hello`, createdAt: `2026-11-18T10:00:00Z` }])
    client.startChatStore(`u1`)
    await flush()
    const put = FakeWorker.all[0]!.posted.find((entry) => entry.op === `putMessages`)
    expect(put?.args?.[0]).toEqual([{ id: `m1`, rootId: `r1`, sessionId: `s1`, authorKind: `agent`, body: `hello`, createdAt: Date.parse(`2026-11-18T10:00:00Z`) }])
    client.stopChatStore()
  })

  it(`a stop forgets the rows kept before a start`, async () => {
    const client = await import(`./client`)
    client.recordMessages([{ id: `m1`, sessionId: `s1`, rootId: `r1`, authorKind: `agent`, body: `hello`, createdAt: `2026-11-18T10:00:00Z` }])
    client.stopChatStore()
    client.startChatStore(`u2`)
    await client.chatStoreCall(`meta`, `k`)
    expect(FakeWorker.all[0]!.posted.some((entry) => entry.op === `putMessages`)).toBe(false)
    client.stopChatStore()
  })
})

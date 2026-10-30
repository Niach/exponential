// EXP-1459: this browser install's unwrapped tree keys, cached in the same
// IndexedDB as its identity and sealed the same way (AES-GCM under a
// NON-EXTRACTABLE key, identity-store.ts): no script can export the cache
// key, a secret exists in plaintext only in memory. One record per
// (account, root, epoch); a second account in the profile has its own.

import { openSecret, sealSecret, TREE_KEY_STORE, withStore, type SealedIdentity } from "./identity-store"

export type CachedTreeKey = { rootId: string; epoch: number; secret: Uint8Array }

/** Where cached tree keys live (IndexedDB in the app, a Map in tests). */
export type TreeKeyStore = {
  all: (userId: string) => Promise<CachedTreeKey[]>
  put: (userId: string, key: CachedTreeKey) => Promise<void>
  clear: (userId: string) => Promise<void>
}

type Record = { userId: string; rootId: string; epoch: number; value: SealedIdentity }

const recordKey = (userId: string, rootId: string, epoch: number) => `${userId}\n${rootId}\n${epoch}`

/** The account's key range (`\n` sorts below every id character). */
const accountRange = (userId: string) => IDBKeyRange.bound(`${userId}\n`, `${userId}\n￿`)

export const indexedDbTreeKeyStore: TreeKeyStore = {
  all: async (userId) => {
    const rows = (await withStore(`readonly`, (s) => s.getAll(accountRange(userId)), TREE_KEY_STORE)) as Record[]
    const out: CachedTreeKey[] = []
    for (const row of rows) {
      const secret = await openSecret(row.value).catch(() => undefined)
      if (secret?.length === 32) out.push({ rootId: row.rootId, epoch: row.epoch, secret })
    }
    return out
  },
  put: async (userId, key) => {
    const record: Record = { userId, rootId: key.rootId, epoch: key.epoch, value: await sealSecret(key.secret) }
    await withStore(`readwrite`, (s) => s.put(record, recordKey(userId, key.rootId, key.epoch)), TREE_KEY_STORE)
  },
  clear: async (userId) => {
    await withStore(`readwrite`, (s) => s.delete(accountRange(userId)), TREE_KEY_STORE)
  },
}

/** An in-memory store (tests, browsers without IndexedDB). */
export function memoryTreeKeyStore(): TreeKeyStore {
  const rows = new Map<string, CachedTreeKey>()
  return {
    all: async (userId) =>
      [...rows.entries()].filter(([k]) => k.startsWith(`${userId}\n`)).map(([, v]) => ({ ...v, secret: new Uint8Array(v.secret) })),
    put: async (userId, key) => {
      rows.set(recordKey(userId, key.rootId, key.epoch), { ...key, secret: new Uint8Array(key.secret) })
    },
    clear: async (userId) => {
      for (const k of [...rows.keys()]) if (k.startsWith(`${userId}\n`)) rows.delete(k)
    },
  }
}

// #EXP-1950 (fixtures/install-trust.json): this browser install's PIN STORE,
// beside its tree keys (tree-key-store.ts) and sealed the same way (AES-GCM
// under a non-extractable key, identity-store.ts): which installs it trusts
// and what it recorded about each key it holds. One record per account.

import type { TrustStore } from "@exp/domain-contract/install-trust"
import { openSecret, sealSecret, TRUST_STORE, withStore, type SealedIdentity } from "./identity-store"

/** Where the pin store lives (IndexedDB in the app, a Map in tests). */
export type TrustStoreIO = {
  get: (userId: string) => Promise<TrustStore | undefined>
  put: (userId: string, store: TrustStore) => Promise<void>
}

const encode = (store: TrustStore) => new TextEncoder().encode(JSON.stringify(store))
const decode = (bytes: Uint8Array): TrustStore | undefined => {
  try {
    const value = JSON.parse(new TextDecoder().decode(bytes)) as TrustStore
    return typeof value?.self === `string` && typeof value.pins === `object` ? value : undefined
  } catch {
    return undefined
  }
}

export const indexedDbTrustStore: TrustStoreIO = {
  get: async (userId) => {
    const row = (await withStore(`readonly`, (s) => s.get(userId), TRUST_STORE)) as SealedIdentity | undefined
    if (!row) return undefined
    const bytes = await openSecret(row).catch(() => undefined)
    return bytes ? decode(bytes) : undefined
  },
  put: async (userId, store) => {
    const value = await sealSecret(encode(store))
    await withStore(`readwrite`, (s) => s.put(value, userId), TRUST_STORE)
  },
}

/** An in-memory store (tests, browsers without IndexedDB). */
export function memoryTrustStore(): TrustStoreIO & { rows: Map<string, TrustStore> } {
  const rows = new Map<string, TrustStore>()
  return {
    rows,
    get: async (userId) => {
      const row = rows.get(userId)
      return row ? (JSON.parse(JSON.stringify(row)) as TrustStore) : undefined
    },
    put: async (userId, store) => {
      rows.set(userId, JSON.parse(JSON.stringify(store)) as TrustStore)
    },
  }
}

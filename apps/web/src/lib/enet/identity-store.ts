// EXP-1457 step 1: the browser install's eNet identity (32 ed25519 secret
// bytes), generated once per account per browser profile and kept ONLY in
// this profile's IndexedDB: never synced, never uploaded.
//
// The wasm node needs the raw 32 bytes, so the identity itself cannot be a
// non-extractable WebCrypto key. What is stored instead: the bytes sealed with
// AES-GCM under a NON-EXTRACTABLE AES key (a CryptoKey, structured-cloned into
// the same record). No script can ever export that key; the plaintext exists
// only in memory while the node runs. Keyed by user id: a second account in
// the same profile gets its own identity (an endpoint id belongs to one
// account, the server answers CONFLICT otherwise).

const DB_NAME = `exp-enet`
const STORE = `identities`

export type SealedIdentity = {
  key: CryptoKey
  iv: Uint8Array<ArrayBuffer>
  sealed: ArrayBuffer
}

/** Where sealed identities live (IndexedDB in the app, a Map in tests). */
export type IdentityStore = {
  get: (userId: string) => Promise<SealedIdentity | undefined>
  put: (userId: string, value: SealedIdentity) => Promise<void>
  delete: (userId: string) => Promise<void>
}

function request<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

function openDb(): Promise<IDBDatabase> {
  const req = indexedDB.open(DB_NAME, 1)
  req.onupgradeneeded = () => req.result.createObjectStore(STORE)
  return request(req)
}

async function withStore<T>(mode: IDBTransactionMode, run: (store: IDBObjectStore) => IDBRequest<T>) {
  const db = await openDb()
  try {
    return await request(run(db.transaction(STORE, mode).objectStore(STORE)))
  } finally {
    db.close()
  }
}

export const indexedDbIdentityStore: IdentityStore = {
  get: (userId) => withStore(`readonly`, (s) => s.get(userId)),
  put: async (userId, value) => {
    await withStore(`readwrite`, (s) => s.put(value, userId))
  },
  delete: async (userId) => {
    await withStore(`readwrite`, (s) => s.delete(userId))
  },
}

async function seal(identity: Uint8Array): Promise<SealedIdentity> {
  const key = await crypto.subtle.generateKey({ name: `AES-GCM`, length: 256 }, false, [
    `encrypt`,
    `decrypt`,
  ])
  const iv = crypto.getRandomValues(new Uint8Array(12))
  const sealed = await crypto.subtle.encrypt({ name: `AES-GCM`, iv }, key, new Uint8Array(identity))
  return { key, iv, sealed }
}

async function open(value: SealedIdentity): Promise<Uint8Array> {
  const plain = await crypto.subtle.decrypt({ name: `AES-GCM`, iv: value.iv }, value.key, value.sealed)
  return new Uint8Array(plain)
}

/** This account's identity in this profile; `generate` mints it the first time. */
export async function loadOrCreateIdentity(
  userId: string,
  generate: () => Uint8Array,
  store: IdentityStore = indexedDbIdentityStore
): Promise<Uint8Array> {
  const existing = await store.get(userId)
  if (existing) {
    const identity = await open(existing).catch(() => undefined)
    if (identity?.length === 32) return identity
  }
  const identity = generate()
  if (identity.length !== 32) throw new Error(`identity must be 32 bytes`)
  await store.put(userId, await seal(identity))
  return identity
}

/** Drops the identity (after `installs.revoke`: a revoked id never registers again). */
export function forgetIdentity(userId: string, store: IdentityStore = indexedDbIdentityStore) {
  return store.delete(userId)
}

// #FEED-237 (message-delivery.json FILES): an outbox entry's picked files are
// part of the entry. The bytes (a Blob, structured-cloned) and names live in
// this profile's IndexedDB keyed by the entry's clientId, so a reload resumes
// the remaining uploads; the entry itself (localStorage) keeps the keys still
// to upload (`pending`) and every landed attachment id.

/** One picked file: `key` is unique within its entry. */
export type OutboxFile = { key: string; name: string; type: string; blob: Blob }

/** Where an entry's files live (IndexedDB in the app, a Map in tests). */
export type OutboxFileStore = {
  get: (clientId: string) => Promise<OutboxFile[] | undefined>
  put: (clientId: string, files: OutboxFile[]) => Promise<void>
  delete: (clientId: string) => Promise<void>
}

const DB_NAME = `exp-outbox-files`
const STORE = `files`

function request<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

function openDb(): Promise<IDBDatabase> {
  const req = indexedDB.open(DB_NAME, 1)
  req.onupgradeneeded = () => {
    if (!req.result.objectStoreNames.contains(STORE)) req.result.createObjectStore(STORE)
  }
  return request(req)
}

async function withStore<T>(mode: IDBTransactionMode, run: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await openDb()
  try {
    return await request(run(db.transaction(STORE, mode).objectStore(STORE)))
  } finally {
    db.close()
  }
}

export const indexedDbOutboxFileStore: OutboxFileStore = {
  get: (clientId) => withStore(`readonly`, (s) => s.get(clientId) as IDBRequest<OutboxFile[] | undefined>),
  put: async (clientId, files) => {
    await withStore(`readwrite`, (s) => s.put(files, clientId))
  },
  delete: async (clientId) => {
    await withStore(`readwrite`, (s) => s.delete(clientId))
  },
}

/** A Map-backed store (tests; a browser without IndexedDB). */
export function memoryOutboxFileStore(): OutboxFileStore & { map: Map<string, OutboxFile[]> } {
  const map = new Map<string, OutboxFile[]>()
  return {
    map,
    get: async (clientId) => map.get(clientId),
    put: async (clientId, files) => {
      map.set(clientId, files)
    },
    delete: async (clientId) => {
      map.delete(clientId)
    },
  }
}

let current: OutboxFileStore | null = null

/** The store the outbox uses (IndexedDB when the browser has it). */
export function outboxFileStore(): OutboxFileStore {
  if (!current) current = typeof indexedDB === `undefined` ? memoryOutboxFileStore() : indexedDbOutboxFileStore
  return current
}

/** Tests: swap the store (null = back to the default). */
export function setOutboxFileStore(store: OutboxFileStore | null) {
  current = store
}

/** The picked files as stored entries, keyed by position + name. */
export function toOutboxFiles(files: readonly File[]): OutboxFile[] {
  return files.map((file, index) => ({ key: `${index}:${file.name}`, name: file.name, type: file.type, blob: file }))
}

/** A stored file back as a File for the upload. */
export function outboxFileAsFile(file: OutboxFile): File {
  return file.blob instanceof File && file.blob.name === file.name
    ? file.blob
    : new File([file.blob], file.name, { type: file.type })
}

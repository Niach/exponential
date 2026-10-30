// EXP-1459: this browser install as a HOLDER of session tree keys
// (fixtures/sealed-content.json is the spec). The install's X25519 secret is
// `PushKeypair::derive(identity)` (install-key.ts); every tree key it may read
// arrives wrapped to that key (`sessionKeys.mine`), is unwrapped here, kept
// in memory and cached sealed in IndexedDB (tree-key-store.ts). Every tab
// holds keys to OPEN content; only the tab running the eNet node (the Web
// Lock in use-web-install.ts) answers `sessionKeys.pending` (on start, then
// every minute): automatic, no approval. Writers create a tree's first key
// (`createTree`) with wraps for the readers they can resolve.

import {
  generateTreeKey,
  treeKeyFromSecret,
  unwrapTreeKey,
  wrapTreeKey,
  type TreeKeys,
} from "@exp/domain-contract/sealed-content"
import type { TreeKeyStore } from "./tree-key-store"

export const PENDING_INTERVAL_MS = 60 * 1000
/** A root whose key is missing is asked for at most this often. */
export const MISSING_RETRY_MS = 30 * 1000
const PENDING_LIMIT = 200
const PENDING_ROUNDS = 10
const MINE_BATCH = 500

export type PendingPair = {
  rootId: string
  epoch: number
  publicKey: string
  targetInstallId: string
  targetX25519Public: string
}

export type TreeWrap = { installId: string; wrapped: string }

/** tRPC `sessionKeys` (lib/trpc/session-keys.ts). */
export type SessionKeysApi = {
  mine: (input: { installId: string; rootIds?: string[] }) => Promise<Array<{ rootId: string; epoch: number; wrapped: string }>>
  pending: (input: { installId: string; limit?: number }) => Promise<PendingPair[]>
  wrap: (input: {
    installId: string
    wraps: Array<{ rootId: string; epoch: number; targetInstallId: string; wrapped: string }>
  }) => Promise<unknown>
  create: (input: { rootId: string; installId: string; epoch: number; publicKey: string; wraps: TreeWrap[] }) => Promise<unknown>
}

/** An install a writer wraps a new tree key for. */
export type ReaderInstall = { installId: string; x25519Public: string }

/** A freshly generated tree key with its wraps (a new root's first key). */
export type NewTreeKey = { epoch: 1; publicKey: string; secret: Uint8Array; wraps: TreeWrap[] }

const EMPTY: TreeKeys = new Map()

export class KeyHolder {
  private keys = new Map<string, Map<number, Uint8Array>>()
  private listeners = new Set<() => void>()
  private asked = new Map<string, number>()
  private queued = new Set<string>()
  private flushing: Promise<void> | undefined
  private stopped = false
  /** Bumps whenever a key arrives (React re-reads opened content). */
  version = 0

  constructor(
    readonly userId: string,
    readonly installId: string,
    private readonly installSecret: Uint8Array,
    private readonly deps: { api: SessionKeysApi; store: TreeKeyStore; now?: () => number }
  ) {}

  /** The cache first (content opens offline), then every wrap the server holds. */
  async start(): Promise<void> {
    const cached = await this.deps.store.all(this.userId).catch(() => [])
    for (const key of cached) this.hold(key.rootId, key.epoch, key.secret)
    if (cached.length > 0) this.notify()
    await this.refresh()
  }

  stop() {
    this.stopped = true
    for (const epochs of this.keys.values()) for (const secret of epochs.values()) secret.fill(0)
    this.keys.clear()
    this.listeners.clear()
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** The tree secrets of one root, by epoch (a stable empty map when none). */
  keysFor(rootId: string): TreeKeys {
    return this.keys.get(rootId) ?? EMPTY
  }

  /** The newest epoch this install holds for `rootId`, with its public key. */
  current(rootId: string): { epoch: number; publicKey: string } | undefined {
    const epochs = this.keys.get(rootId)
    if (!epochs || epochs.size === 0) return undefined
    const epoch = Math.max(...epochs.keys())
    return { epoch, publicKey: treeKeyFromSecret(epochs.get(epoch)!).publicKey }
  }

  /** Loads the wraps this install holds (every root, or `rootIds`). */
  async refresh(rootIds?: string[]): Promise<void> {
    if (rootIds?.length === 0) return
    const batches = rootIds ? chunk(rootIds, MINE_BATCH) : [undefined]
    let changed = false
    for (const batch of batches) {
      const rows = await this.deps.api.mine({ installId: this.installId, ...(batch ? { rootIds: batch } : {}) })
      if (this.stopped) return
      for (const row of rows) {
        if (this.keys.get(row.rootId)?.has(row.epoch)) continue
        let secret: Uint8Array
        try {
          secret = unwrapTreeKey(this.installSecret, row.wrapped)
        } catch {
          continue // not wrapped to this install's key: never shown, never cached
        }
        this.hold(row.rootId, row.epoch, secret)
        await this.deps.store.put(this.userId, { rootId: row.rootId, epoch: row.epoch, secret }).catch(() => {})
        changed = true
      }
    }
    if (changed) this.notify()
  }

  /** Content of `rootId` sealed at `epoch` did not open (`noKey`): ask the
   *  server for the wrap, batched and at most every MISSING_RETRY_MS per root. */
  request(rootId: string, epoch: number | null) {
    if (this.stopped || (epoch !== null && this.keys.get(rootId)?.has(epoch))) return
    const now = (this.deps.now ?? Date.now)()
    const last = this.asked.get(rootId)
    if (last !== undefined && now - last < MISSING_RETRY_MS) return
    this.asked.set(rootId, now)
    this.queued.add(rootId)
    this.flushing ??= Promise.resolve().then(async () => {
      const rootIds = [...this.queued]
      this.queued.clear()
      this.flushing = undefined
      await this.refresh(rootIds).catch(() => {})
    })
  }

  /** As a holder: wraps every pending reader of a tree whose current key
   *  this install holds. */
  async answerPending(): Promise<number> {
    let written = 0
    for (let round = 0; round < PENDING_ROUNDS && !this.stopped; round++) {
      const pairs = await this.deps.api.pending({ installId: this.installId, limit: PENDING_LIMIT })
      const wraps: Array<{ rootId: string; epoch: number; targetInstallId: string; wrapped: string }> = []
      for (const pair of pairs) {
        const secret = this.keys.get(pair.rootId)?.get(pair.epoch)
        // Only for the key the root really carries.
        if (!secret || treeKeyFromSecret(secret).publicKey !== pair.publicKey) continue
        try {
          wraps.push({
            rootId: pair.rootId,
            epoch: pair.epoch,
            targetInstallId: pair.targetInstallId,
            wrapped: wrapTreeKey(pair.targetX25519Public, secret),
          })
        } catch {
          // A malformed reader key: skip it, the others still get theirs.
        }
      }
      if (wraps.length > 0) {
        await this.deps.api.wrap({ installId: this.installId, wraps })
        written += wraps.length
      }
      // A full page whose pairs this install could not wrap would repeat.
      if (pairs.length < PENDING_LIMIT || wraps.length === 0) break
    }
    return written
  }

  /** A new tree key (epoch 1) wrapped for `readers` (this install always
   *  among them); `holdFor(rootId)` keeps it once the root exists. */
  newTreeKey(readers: ReaderInstall[]): NewTreeKey {
    const key = generateTreeKey()
    const wraps = new Map<string, string>()
    for (const reader of readers) {
      if (wraps.has(reader.installId)) continue
      try {
        wraps.set(reader.installId, wrapTreeKey(reader.x25519Public, key.secret))
      } catch {
        // A malformed reader key: that install asks a holder later.
      }
    }
    if (!wraps.has(this.installId)) {
      throw new Error(`this install is not among the readers`)
    }
    return {
      epoch: 1,
      publicKey: key.publicKey,
      secret: key.secret,
      wraps: [...wraps].map(([installId, wrapped]) => ({ installId, wrapped })),
    }
  }

  /** Keeps a tree key this install created (the root now exists). */
  async keep(rootId: string, epoch: number, secret: Uint8Array): Promise<void> {
    this.hold(rootId, epoch, secret)
    this.notify()
    await this.deps.store.put(this.userId, { rootId, epoch, secret }).catch(() => {})
  }

  /** The first key of an EXISTING keyless root (`sessionKeys.create`). */
  async createTree(rootId: string, readers: ReaderInstall[]): Promise<{ epoch: number; publicKey: string }> {
    const key = this.newTreeKey(readers)
    await this.deps.api.create({
      rootId,
      installId: this.installId,
      epoch: key.epoch,
      publicKey: key.publicKey,
      wraps: key.wraps,
    })
    await this.keep(rootId, key.epoch, key.secret)
    return { epoch: key.epoch, publicKey: key.publicKey }
  }

  private hold(rootId: string, epoch: number, secret: Uint8Array) {
    let epochs = this.keys.get(rootId)
    if (!epochs) {
      epochs = new Map()
      this.keys.set(rootId, epochs)
    } else {
      // A new Map per change: memoized readers see a new identity.
      epochs = new Map(epochs)
      this.keys.set(rootId, epochs)
    }
    epochs.set(epoch, secret)
  }

  private notify() {
    this.version++
    for (const listener of this.listeners) listener()
  }
}

function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = []
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size))
  return out
}

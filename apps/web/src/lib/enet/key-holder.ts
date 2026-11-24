// EXP-1459: this browser install as a HOLDER of session tree keys
// (fixtures/sealed-content.json is the spec). The install's X25519 secret is
// `PushKeypair::derive(identity)` (install-key.ts); every tree key it may read
// arrives wrapped to that key (`sessionKeys.mine`), is unwrapped here, kept
// in memory and cached sealed in IndexedDB (tree-key-store.ts). Every tab
// holds keys to OPEN content; only the tab running the eNet node (the Web
// Lock in use-web-install.ts) runs the holder pass (on start, then every
// minute): it rotates the roots that are due (#EXP-1951, `rotation`: a
// reader was dropped) and answers `sessionKeys.pending`: automatic, no
// approval. Writers create a tree's first key (`createTree`) with wraps for
// the readers they can resolve.
// #EXP-1950 (fixtures/install-trust.json, wrap-signature.json): with its
// identity this holder SIGNS every wrap it makes (create, pending, rotation;
// a new root's inline seal right after the send, `sessionKeys.resign`), and
// with its `InstallTrust` it keeps a wrap only by R1, wraps only for targets
// R4 allows (an install that asked and is not approved gets nothing) and
// follows `installs.trust` on every pass. Release 1 records, refuses only
// what the fixture refuses now.

import {
  createFailure,
  generateTreeKey,
  rotationAfter,
  rotationEpoch,
  treeKeyFromSecret,
  unwrapTreeKey,
  wrapTreeKey,
  type CreateFailure,
  type TreeKeys,
} from "@exp/domain-contract/sealed-content"
import { resignAfter, signWrap } from "@exp/domain-contract/wrap-signature"
import type { InstallTrust } from "./install-trust"
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
  /** #EXP-1950: the target's ed25519 key (absent on an older server: the wrap goes unsigned). */
  targetEndpointId?: string | null
}

export type TreeWrap = { installId: string; wrapped: string; signature?: string }

/** A served wrap (`sessionKeys.mine`). */
export type MineRow = { rootId: string; epoch: number; wrapped: string; signature?: string | null; signerEndpointId?: string | null }

/** tRPC `sessionKeys` (lib/trpc/session-keys.ts). */
export type SessionKeysApi = {
  mine: (input: { installId: string; rootIds?: string[] }) => Promise<MineRow[]>
  pending: (input: { installId: string; limit?: number }) => Promise<PendingPair[]>
  wrap: (input: {
    installId: string
    wraps: Array<{ rootId: string; epoch: number; targetInstallId: string; wrapped: string; signature?: string }>
  }) => Promise<unknown>
  create: (input: { rootId: string; installId: string; epoch: number; publicKey: string; wraps: TreeWrap[] }) => Promise<unknown>
  /** #EXP-1951: the roots this install rotates (`epoch` = the one to supersede). */
  due: (input: { installId: string; limit?: number }) => Promise<Array<{ rootId: string; epoch: number }>>
  /** #EXP-1950 (wrap-signature.json `resign`): signed wraps replace unsigned rows. */
  resign?: (input: {
    installId: string
    wraps: Array<{ rootId: string; epoch: number; installId: string; wrapped: string; signature: string }>
  }) => Promise<{ replaced: number; skipped: number }>
}

/** An install a writer wraps a new tree key for. */
export type ReaderInstall = { installId: string; x25519Public: string; endpointId?: string | null; userId?: string | null }

/** A freshly generated tree key with its wraps (a new root's first key);
 *  `recipients` = each wrap's recipient endpoint id, for signing it once the
 *  root id is known (`adoptNewRoot`). */
export type NewTreeKey = { epoch: 1; publicKey: string; secret: Uint8Array; wraps: TreeWrap[]; recipients?: Map<string, string> }

const EMPTY: TreeKeys = new Map()

export class KeyHolder {
  private keys = new Map<string, Map<number, Uint8Array>>()
  private listeners = new Set<() => void>()
  private asked = new Map<string, number>()
  private queued = new Set<string>()
  private flushing: Promise<void> | undefined
  private stopped = false
  /** #EXP-1951: rotations the server refused this install, `root\nepoch`. */
  private left = new Set<string>()
  /** Bumps whenever a key arrives (React re-reads opened content). */
  version = 0

  constructor(
    readonly userId: string,
    readonly installId: string,
    private readonly installSecret: Uint8Array,
    private readonly deps: {
      api: SessionKeysApi
      store: TreeKeyStore
      now?: () => number
      /** #EXP-1950: the install's 32 identity bytes (signs every wrap it makes). */
      identity?: Uint8Array
      /** #EXP-1950: the pin store and the four rules. */
      trust?: InstallTrust
    }
  ) {}

  /** #EXP-1950: this install's trust (none = the rules are off, as before). */
  get trust(): InstallTrust | undefined {
    return this.deps.trust
  }

  /** The signature of a wrap this install makes; undefined without an identity or the recipient's key. */
  private sign(key: { rootId: string; epoch: number; publicKey: string }, recipientEndpointId: string | null | undefined, wrapped: string): string | undefined {
    if (!this.deps.identity || !recipientEndpointId) return undefined
    try {
      return signWrap(this.deps.identity, { rootId: key.rootId, epoch: key.epoch, treePublicKey: key.publicKey, recipientEndpointId, wrapped })
    } catch {
      return undefined
    }
  }

  /** The cache first (content opens offline), then every wrap the server holds. */
  async start(): Promise<void> {
    await this.loadCache()
    await this.refresh()
  }

  /** The keys this install cached (IndexedDB), no network. */
  async loadCache(): Promise<void> {
    const cached = await this.deps.store.all(this.userId).catch(() => [])
    for (const key of cached) {
      this.hold(key.rootId, key.epoch, key.secret)
      // Decision (c): a key held before the pin store knew it stays, `legacy`.
      if (this.deps.trust && !this.deps.trust.store.roots[key.rootId]?.epochs[String(key.epoch)]) this.deps.trust.record(key.rootId, key.epoch, `legacy`)
    }
    if (cached.length > 0) this.notify()
  }

  stop() {
    this.stopped = true
    this.deps.identity?.fill(0)
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
        if (this.deps.trust) {
          // R1 (install-trust.json `keepCases`): kept and recorded, or refused.
          const verdict = this.deps.trust.keep(row, this.installSecret)
          if (!verdict.keep) continue
          secret = verdict.secret
        } else {
          try {
            secret = unwrapTreeKey(this.installSecret, row.wrapped)
          } catch {
            continue // not wrapped to this install's key: never shown, never cached
          }
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
      const wraps: Array<{ rootId: string; epoch: number; targetInstallId: string; wrapped: string; signature?: string }> = []
      for (const pair of pairs) {
        const secret = this.keys.get(pair.rootId)?.get(pair.epoch)
        // Only for the key the root really carries.
        if (!secret || treeKeyFromSecret(secret).publicKey !== pair.publicKey) continue
        // R4: never for a target that is not pinned (and asked) or whose key is not the pinned one.
        const verdict = this.deps.trust?.target({ installId: pair.targetInstallId, x25519Public: pair.targetX25519Public, endpointId: pair.targetEndpointId })
        if (verdict && !verdict.wrap) continue
        try {
          const wrapped = wrapTreeKey(pair.targetX25519Public, secret)
          const signature = this.sign(pair, pair.targetEndpointId, wrapped)
          wraps.push({
            rootId: pair.rootId,
            epoch: pair.epoch,
            targetInstallId: pair.targetInstallId,
            wrapped,
            ...(signature ? { signature } : {}),
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

  /** #EXP-1951 (sealed-content.json `rotation`): a fresh key at the next
   *  epoch for every root the server names as due whose current secret this
   *  install holds, wrapped for THIS install only: the current readers get
   *  theirs from `pending`, which lists readers and nobody else. */
  async rotateDue(): Promise<number> {
    const due = await this.deps.api.due({ installId: this.installId, limit: PENDING_LIMIT })
    if (this.stopped || due.length === 0) return 0
    // The server holds a wrap of each for this install: unwrap what is missing.
    await this.refresh(due.filter((root) => !this.keys.get(root.rootId)?.has(root.epoch)).map((root) => root.rootId)).catch(() => {})
    const own = treeKeyFromSecret(this.installSecret).publicKey
    let rotated = 0
    for (const root of due) {
      if (this.stopped) break
      const epochs = this.keys.get(root.rootId)
      const epoch = rotationEpoch({
        flagged: root.epoch,
        current: Math.max(root.epoch, ...(epochs?.keys() ?? [])),
        held: epochs?.has(root.epoch) ?? false,
        left: this.left.has(`${root.rootId}\n${root.epoch}`),
      })
      if (epoch === null) continue
      const key = generateTreeKey()
      let failure: CreateFailure | null = null
      try {
        const wrapped = wrapTreeKey(own, key.secret)
        const signature = this.sign({ rootId: root.rootId, epoch, publicKey: key.publicKey }, this.deps.trust?.endpointId, wrapped)
        await this.deps.api.create({
          rootId: root.rootId,
          installId: this.installId,
          epoch,
          publicKey: key.publicKey,
          wraps: [{ installId: this.installId, wrapped, ...(signature ? { signature } : {}) }],
        })
      } catch (error) {
        const data = (error as { data?: { code?: unknown } } | null)?.data
        failure = createFailure({ code: typeof data?.code === `string` ? data.code : undefined, message: error instanceof Error ? error.message : undefined })
      }
      switch (rotationAfter(failure)) {
        case `adopt`:
          await this.keep(root.rootId, epoch, key.secret)
          rotated++
          break
        case `reread`:
          await this.refresh([root.rootId]).catch(() => {})
          break
        case `leave`:
          this.left.add(`${root.rootId}\n${root.epoch}`)
          break
        case `retry`:
          break
      }
    }
    return rotated
  }

  /** One holder pass: the rotations that are due, then the pending wraps
   *  (the new epochs' readers among them). */
  async pass(): Promise<void> {
    await this.deps.trust?.sync().catch(() => {})
    await this.rotateDue().catch(() => {})
    await this.answerPending()
    await this.resignPending().catch(() => {})
  }

  /** A new tree key (epoch 1) wrapped for `readers` (this install always
   *  among them); `holdFor(rootId)` keeps it once the root exists. */
  newTreeKey(readers: ReaderInstall[]): NewTreeKey {
    const key = generateTreeKey()
    const wraps = new Map<string, string>()
    const recipients = new Map<string, string>()
    for (const reader of readers) {
      if (wraps.has(reader.installId)) continue
      // R4: a reader this install may not wrap for is skipped (its holders decide later).
      if (reader.installId !== this.installId && this.deps.trust && !this.deps.trust.target(reader).wrap) continue
      try {
        wraps.set(reader.installId, wrapTreeKey(reader.x25519Public, key.secret))
        if (reader.endpointId) recipients.set(reader.installId, reader.endpointId)
      } catch {
        // A malformed reader key: that install asks a holder later.
      }
    }
    if (!wraps.has(this.installId)) {
      throw new Error(`this install is not among the readers`)
    }
    if (this.deps.trust) recipients.set(this.installId, this.deps.trust.endpointId)
    return {
      epoch: 1,
      publicKey: key.publicKey,
      secret: key.secret,
      wraps: [...wraps].map(([installId, wrapped]) => ({ installId, wrapped })),
      recipients,
    }
  }

  /** #EXP-1950 (install-trust.json `newRoot`): a key this install created
   *  in the send that created its root. Its wraps went unsigned (the root id
   *  was unknown); now they are signed (`sessionKeys.resign`), and again on
   *  every pass until the server took them. */
  async adoptNewRoot(rootId: string, key: NewTreeKey): Promise<void> {
    await this.keep(rootId, key.epoch, key.secret)
    if (!this.deps.identity || !this.deps.api.resign) return
    const wraps: Array<{ rootId: string; epoch: number; installId: string; wrapped: string; signature: string }> = []
    for (const wrap of key.wraps) {
      const signature = this.sign({ rootId, epoch: key.epoch, publicKey: key.publicKey }, key.recipients?.get(wrap.installId), wrap.wrapped)
      if (signature) wraps.push({ rootId, epoch: key.epoch, installId: wrap.installId, wrapped: wrap.wrapped, signature })
    }
    if (wraps.length === 0) return
    this.unsignedRoots.set(rootId, wraps)
    await this.resignPending().catch(() => {})
  }

  /** Signed wraps of new roots the server has not taken yet. */
  private unsignedRoots = new Map<string, Array<{ rootId: string; epoch: number; installId: string; wrapped: string; signature: string }>>()

  /** Sends the signed wraps of new roots (`resign`); an older server
   *  (NOT_FOUND) or a blip = again on a later pass, quietly. */
  async resignPending(): Promise<number> {
    const resign = this.deps.api.resign
    if (!resign || this.unsignedRoots.size === 0) return 0
    let replaced = 0
    for (const [rootId, wraps] of [...this.unsignedRoots]) {
      try {
        replaced += (await resign({ installId: this.installId, wraps })).replaced
        this.unsignedRoots.delete(rootId)
      } catch (error) {
        const data = (error as { data?: { code?: unknown } } | null)?.data
        if (resignAfter({ code: typeof data?.code === `string` ? data.code : null }) === `refused`) this.unsignedRoots.delete(rootId)
      }
    }
    return replaced
  }

  /** Keeps a tree key this install created (the root now exists). */
  async keep(rootId: string, epoch: number, secret: Uint8Array): Promise<void> {
    this.hold(rootId, epoch, secret)
    // It made the key itself: verified (install-trust.json `newRoot.makerRecord`).
    this.deps.trust?.record(rootId, epoch, `verified`)
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
      wraps: key.wraps.map((wrap) => {
        const signature = this.sign({ rootId, epoch: key.epoch, publicKey: key.publicKey }, key.recipients?.get(wrap.installId), wrap.wrapped)
        return signature ? { ...wrap, signature } : wrap
      }),
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

/** The keys this install cached on its last run, readable BEFORE its install
 *  is resolved (`installs.mine`, a network call): a cold start opens sealed
 *  content on its first render, offline too (fixtures/sealed-content.json
 *  `coldStart`). Read-only: it never asks for wraps (the holder's first
 *  pass, once it runs, fetches every wrap). */
export class CachedKeys {
  private keys = new Map<string, Map<number, Uint8Array>>()

  constructor(readonly userId: string) {}

  static async load(userId: string, store: TreeKeyStore): Promise<CachedKeys> {
    const out = new CachedKeys(userId)
    for (const key of await store.all(userId).catch(() => [])) {
      let epochs = out.keys.get(key.rootId)
      if (!epochs) out.keys.set(key.rootId, (epochs = new Map()))
      epochs.set(key.epoch, key.secret)
    }
    return out
  }

  get size(): number {
    return this.keys.size
  }

  keysFor(rootId: string): TreeKeys {
    return this.keys.get(rootId) ?? EMPTY
  }

  request(_rootId: string, _epoch: number | null) {}

  clear() {
    for (const epochs of this.keys.values()) for (const secret of epochs.values()) secret.fill(0)
    this.keys.clear()
  }
}

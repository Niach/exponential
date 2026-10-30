// EXP-1459: mounts this account's KeyHolder in every tab (opening sealed
// content needs the keys everywhere) and runs the holder loop
// (`sessionKeys.pending` on start, then every minute) only in the tab that
// holds the eNet node's Web Lock (use-web-install.ts calls `setKeyLeader`).
// The install id is the `installs.mine` row whose `x25519Public` is this
// profile's derived key: the node's tab registers it, the others find it.

import { useEffect, useSyncExternalStore } from "react"
import { b64urlEncode, pushPublicKey, type TreeKeys } from "@exp/domain-contract/sealed-content"
import { trpc } from "@/lib/trpc-client"
import { currentKeyHolder, emitKeyChange, setCurrentKeyHolder, subscribeKeyChange } from "./current-key-holder"
import { loadIdentity } from "./identity-store"
import { deriveInstallSecret } from "./install-key"
import { KeyHolder, PENDING_INTERVAL_MS, type SessionKeysApi } from "./key-holder"
import { indexedDbTreeKeyStore } from "./tree-key-store"

/** How often a tab without an install yet looks again (the node's tab may
 *  still be registering it). */
const RESOLVE_RETRY_MS = 15 * 1000

const quiet = { context: { skipErrorToast: true } }

const api: SessionKeysApi = {
  mine: (input) => trpc.sessionKeys.mine.query(input, quiet),
  pending: (input) => trpc.sessionKeys.pending.query(input, quiet),
  wrap: (input) => trpc.sessionKeys.wrap.mutate(input, quiet),
  create: (input) => trpc.sessionKeys.create.mutate(input, quiet),
}

let leader = false
let leaderTimer: ReturnType<typeof setInterval> | undefined

export { currentKeyHolder }

function answerPending() {
  void currentKeyHolder()?.answerPending().catch(() => {})
}

function syncLeaderLoop() {
  clearInterval(leaderTimer)
  leaderTimer = undefined
  if (!leader || !currentKeyHolder()) return
  answerPending()
  leaderTimer = setInterval(answerPending, PENDING_INTERVAL_MS)
}

/** The eNet node's tab (the Web Lock holder) answers pending wraps. */
export function setKeyLeader(isLeader: boolean) {
  leader = isLeader
  syncLeaderLoop()
}

async function resolveInstall(userId: string): Promise<{ installId: string; secret: Uint8Array } | undefined> {
  const identity = await loadIdentity(userId)
  if (!identity) return undefined
  const secret = await deriveInstallSecret(identity)
  identity.fill(0)
  const x25519Public = b64urlEncode(pushPublicKey(secret))
  const mine = await trpc.installs.mine.query(undefined, quiet)
  const row = mine.find((r) => r.x25519Public === x25519Public && !r.revokedAt)
  if (!row) {
    secret.fill(0)
    return undefined
  }
  return { installId: row.installId, secret }
}

export function useKeyHolder(userId: string | undefined) {
  useEffect(() => {
    if (!userId || typeof indexedDB === `undefined`) return
    let stopped = false
    let retry: ReturnType<typeof setTimeout> | undefined
    let own: KeyHolder | undefined
    const attempt = async () => {
      const found = await resolveInstall(userId).catch(() => undefined)
      if (stopped) return
      if (!found) {
        retry = setTimeout(() => void attempt(), RESOLVE_RETRY_MS)
        return
      }
      own = new KeyHolder(userId, found.installId, found.secret, { api, store: indexedDbTreeKeyStore })
      own.subscribe(emitKeyChange)
      setCurrentKeyHolder(own)
      await own.start().catch(() => {})
      if (!stopped) syncLeaderLoop()
    }
    void attempt()
    return () => {
      stopped = true
      clearTimeout(retry)
      if (own && currentKeyHolder() === own) {
        setCurrentKeyHolder(undefined)
        syncLeaderLoop()
      }
      own?.stop()
    }
  }, [userId])
}

const EMPTY: TreeKeys = new Map()

/** The tree keys this install holds for `rootId` (re-renders when one arrives). */
export function useTreeKeys(rootId: string | null | undefined): TreeKeys {
  return useSyncExternalStore(
    subscribeKeyChange,
    () => (rootId ? (currentKeyHolder()?.keysFor(rootId) ?? EMPTY) : EMPTY),
    () => EMPTY
  )
}

/** Bumps whenever any key arrives (lists spanning many roots). */
export function useKeyVersion(): number {
  return useSyncExternalStore(
    subscribeKeyChange,
    () => currentKeyHolder()?.version ?? -1,
    () => -1
  )
}

import { useEffect, useMemo, useSyncExternalStore } from "react"
import type { SessionsSendInput } from "@exp/db-schema/domain"
import {
  outboxMerge,
  type OutboxEntry,
  type OutboxRow,
} from "@exp/domain-contract/message-delivery"
import { messageCollection } from "@/lib/collections"
import { sendSessionMessage } from "@/lib/enet/send-sealed"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"

// #FEED-237 (message-delivery.json): THE session composer's outbox. A send
// is an entry the moment Send is pressed (the field and its synced draft
// clear then), drawn as the viewer's own bubble at the bottom of the
// transcript with the clock; the server's answer gives it its messageId, a
// failure the red mark. Persisted per session in localStorage
// (`exp-outbox:<sessionId>`) so a reload keeps it: an unanswered `sending`
// entry is sent AGAIN on load with the same clientMessageId (F21: the server
// returns the row it already stored). An entry drops once its synced row
// arrives (`outboxMerge`), that row's bubble taking its place.

export interface WebOutboxEntry extends OutboxEntry {
  teamId: string
  attachmentIds?: string[]
  /** The send's other picks (an Assistant's Teams pick, an agent chat's
   *  MCP servers), sent again on a retry. */
  extra?: Record<string, unknown>
  /** Files still uploading when the page went away: never resent blind. */
  uploading?: boolean
}

const KEY_PREFIX = `exp-outbox:`
const entries = new Map<string, WebOutboxEntry[]>()
const listeners = new Set<() => void>()
const inFlight = new Set<string>()
const EMPTY: WebOutboxEntry[] = []

function storage(): Storage | null {
  try {
    return typeof window === `undefined` ? null : window.localStorage
  } catch {
    return null
  }
}

function load(sessionId: string): WebOutboxEntry[] {
  const cached = entries.get(sessionId)
  if (cached) return cached
  let list: WebOutboxEntry[] = []
  try {
    const raw = storage()?.getItem(`${KEY_PREFIX}${sessionId}`)
    const parsed: unknown = raw ? JSON.parse(raw) : []
    if (Array.isArray(parsed)) {
      list = parsed
        .filter(
          (entry): entry is WebOutboxEntry =>
            typeof entry === `object` &&
            entry !== null &&
            typeof entry.clientId === `string` &&
            typeof entry.text === `string` &&
            typeof entry.createdAt === `string`
        )
        // Uploads die with the page: such an entry waits for its Retry.
        .map((entry) => (entry.uploading && !entry.messageId ? { ...entry, state: `failed`, uploading: false } : entry))
    }
  } catch {
    list = []
  }
  entries.set(sessionId, list)
  return list
}

function save(sessionId: string, list: WebOutboxEntry[]) {
  entries.set(sessionId, list)
  try {
    const store = storage()
    if (list.length === 0) store?.removeItem(`${KEY_PREFIX}${sessionId}`)
    else store?.setItem(`${KEY_PREFIX}${sessionId}`, JSON.stringify(list))
  } catch {
    // A full or blocked storage keeps the in-memory outbox.
  }
  for (const listener of listeners) listener()
}

function patch(sessionId: string, clientId: string, change: Partial<WebOutboxEntry>) {
  const list = load(sessionId)
  if (!list.some((entry) => entry.clientId === clientId)) return
  save(
    sessionId,
    list.map((entry) => (entry.clientId === clientId ? { ...entry, ...change } : entry))
  )
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** Send (or resend) one entry; the entry carries the outcome. */
async function deliver(sessionId: string, clientId: string, upload?: () => Promise<string[]>) {
  if (inFlight.has(clientId)) return
  const entry = load(sessionId).find((candidate) => candidate.clientId === clientId)
  if (!entry || entry.messageId) return
  inFlight.add(clientId)
  try {
    let attachmentIds = entry.attachmentIds
    if (upload) {
      patch(sessionId, clientId, { uploading: true })
      attachmentIds = await upload()
      patch(sessionId, clientId, { uploading: false, attachmentIds })
    }
    const input = {
      ...(entry.extra ?? {}),
      teamId: entry.teamId,
      sessionId,
      text: entry.text || undefined,
      attachmentIds: attachmentIds && attachmentIds.length > 0 ? attachmentIds : undefined,
      clientMessageId: clientId,
    } as SessionsSendInput
    const result = await sendSessionMessage(input, SKIP_ERROR_TOAST)
    patch(sessionId, clientId, { messageId: result.messageId, state: `sending`, uploading: false })
    await messageCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
  } catch {
    patch(sessionId, clientId, { state: `failed`, uploading: false })
  } finally {
    inFlight.delete(clientId)
  }
}

/** The press: a new entry at the bottom, sent at once. `upload` runs the
 *  composer's file uploads inside the send (null = none). */
export function sendThroughOutbox(input: {
  teamId: string
  sessionId: string
  text: string
  upload?: () => Promise<string[]>
  extra?: Record<string, unknown>
}): string {
  const clientId = crypto.randomUUID()
  const entry: WebOutboxEntry = {
    clientId,
    teamId: input.teamId,
    text: input.text.trim(),
    createdAt: new Date().toISOString(),
    state: `sending`,
    messageId: null,
    ...(input.extra && Object.keys(input.extra).length > 0 ? { extra: input.extra } : {}),
  }
  save(input.sessionId, [...load(input.sessionId), entry])
  void deliver(input.sessionId, clientId, input.upload)
  return clientId
}

/** The failed bubble's Retry: the SAME clientId, back to the clock. */
export function retryOutboxEntry(sessionId: string, clientId: string) {
  patch(sessionId, clientId, { state: `sending` })
  void deliver(sessionId, clientId)
}

/** The failed bubble's Delete. */
export function deleteOutboxEntry(sessionId: string, clientId: string) {
  save(
    sessionId,
    load(sessionId).filter((entry) => entry.clientId !== clientId)
  )
}

/** The session's outbox entries still drawn, oldest first; the ones a
 *  synced row replaced leave the store. Unanswered sends resume on mount. */
export function useSessionOutbox(
  sessionId: string | null | undefined,
  viewerId: string | null,
  rows: readonly OutboxRow[]
): WebOutboxEntry[] {
  const list = useSyncExternalStore(
    subscribe,
    () => (sessionId ? load(sessionId) : EMPTY),
    () => EMPTY
  )
  const merged = useMemo(
    () => (viewerId && list.length > 0 ? outboxMerge({ viewerId, outbox: list, rows }) : null),
    [viewerId, list, rows]
  )
  useEffect(() => {
    if (!sessionId || !merged || merged.drop.length === 0) return
    const gone = new Set(merged.drop)
    save(
      sessionId,
      load(sessionId).filter((entry) => !gone.has(entry.clientId))
    )
  }, [sessionId, merged])
  useEffect(() => {
    if (!sessionId) return
    for (const entry of load(sessionId)) {
      if (entry.state === `sending` && !entry.messageId && !entry.uploading) void deliver(sessionId, entry.clientId)
    }
  }, [sessionId])
  return useMemo(() => {
    if (!merged) return list
    const order = new Map(merged.keep.map((id, index) => [id, index]))
    return list
      .filter((entry) => order.has(entry.clientId))
      .sort((a, b) => (order.get(a.clientId) ?? 0) - (order.get(b.clientId) ?? 0))
  }, [list, merged])
}

/** Tests: forget every in-memory outbox. */
export function resetOutboxForTests() {
  entries.clear()
  inFlight.clear()
}

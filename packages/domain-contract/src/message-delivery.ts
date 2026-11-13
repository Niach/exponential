// A person's own message, WhatsApp-style: one bubble at the bottom of the
// transcript from the press, its delivery mark (clock, one check, two
// checks, the red mark) and its menu. Pure and locked by
// `fixtures/message-delivery.json`, whose `$comment` holds the rules; every
// mirror replays `stateCases`, `outboxCases` and `actionCases`: desktop
// `domain::message_delivery`, iOS ExpCore `MessageDelivery.swift`, Android
// `domain/MessageDelivery.kt`.
import fixture from "../fixtures/message-delivery.json" with { type: "json" }

export const MESSAGE_DELIVERY_COPY = fixture.copy
export const MESSAGE_DELIVERY_MARKS = fixture.marks
export const MESSAGE_DELIVERY_TEST_IDS = fixture.testIds
export const MESSAGE_DELIVERY_MARK_SIZE = fixture.markSize
export const MESSAGE_DELIVERY_MATCH_WINDOW_MS = fixture.matchWindowMs
export const MESSAGE_DELIVERY_HANDOFF_MAX_MS = fixture.handoffMaxMs

/** genUI's `messageStatus`: the clock, one check, two checks, the red mark. */
export type DeliveryState = `sending` | `sent` | `read` | `failed`

export interface DeliveryStateInput {
  /** The outbox entry's state; null = a synced row alone. */
  local: `sending` | `failed` | null
  /** The server holds the row (an entry's messageId, or a synced row). */
  stored: boolean
  deliveredAt: string | null
  consumedAt: string | null
  /** The bubble is the live transcript's echo of the turn. */
  echoed: boolean
}

export function deliveryState(input: DeliveryStateInput): DeliveryState {
  if (input.echoed || input.consumedAt) return `read`
  if (input.deliveredAt) return `sent`
  if (input.local === `failed` && !input.stored) return `failed`
  return `sending`
}

export interface OutboxEntry {
  /** A fresh UUID per press; also the `sessions.send` `clientMessageId`. */
  clientId: string
  text: string
  createdAt: string
  state: `sending` | `failed`
  /** The row the server stored it as (null until the send answers). */
  messageId: string | null
}

export interface OutboxRow {
  id: string
  authorKind: string
  authorId: string | null
  body: string
  createdAt: string
}

const byAge = (a: OutboxEntry, b: OutboxEntry): number => {
  const at = Date.parse(a.createdAt) - Date.parse(b.createdAt)
  if (at !== 0) return at
  return a.clientId < b.clientId ? -1 : a.clientId > b.clientId ? 1 : 0
}

/** Which outbox entries a synced row replaced (`drop`) and which still draw
 *  (`keep`, oldest first). */
export function outboxMerge(input: {
  viewerId: string
  outbox: readonly OutboxEntry[]
  rows: readonly OutboxRow[]
}): { drop: string[]; keep: string[] } {
  const entries = [...input.outbox].sort(byAge)
  const rowIds = new Set(input.rows.map((row) => row.id))
  const matched = new Set<string>()
  const dropped = new Set<string>()
  // By id first: an acked entry's row is its row, whatever its text.
  for (const entry of entries) {
    if (entry.messageId && rowIds.has(entry.messageId)) {
      dropped.add(entry.clientId)
      matched.add(entry.messageId)
    }
  }
  const own = input.rows
    .filter((row) => row.authorKind === `user` && row.authorId === input.viewerId)
    .sort((a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  for (const entry of entries) {
    if (dropped.has(entry.clientId) || entry.messageId || entry.state === `failed`) continue
    const from = Date.parse(entry.createdAt) - MESSAGE_DELIVERY_MATCH_WINDOW_MS
    const row = own.find(
      (candidate) =>
        !matched.has(candidate.id) && candidate.body === entry.text && Date.parse(candidate.createdAt) >= from
    )
    if (!row) continue
    matched.add(row.id)
    dropped.add(entry.clientId)
  }
  return {
    drop: entries.filter((entry) => dropped.has(entry.clientId)).map((entry) => entry.clientId),
    keep: entries.filter((entry) => !dropped.has(entry.clientId)).map((entry) => entry.clientId),
  }
}

export type BubbleAction = `retry` | `delete` | `cancel`

/** The bubble's menu, in order (empty = no menu). */
export function bubbleActions(input: { state: DeliveryState; stored: boolean }): BubbleAction[] {
  if (input.state === `failed`) return [`retry`, `delete`]
  if (input.state === `read`) return []
  return input.stored ? [`cancel`] : []
}

export interface HandoffRow {
  id: string
  authorKind: string
  authorId: string | null
  body: string
  createdAt: string
  consumedAt: string | null
  card?: unknown
}

export interface HandoffEcho {
  text: string
  /** Epoch ms; null when the feed carries none. */
  at: number | null
}

/** HANDOFF: the consumed own rows whose transcript echo has not arrived yet
 *  (oldest first); each keeps its bubble so it never vanishes in between. */
export function ownHandoff(input: {
  viewerId: string
  now: string
  feedEmpty: boolean
  rows: readonly HandoffRow[]
  echoes: readonly HandoffEcho[]
}): string[] {
  if (input.feedEmpty) return []
  const since = Date.parse(input.now) - MESSAGE_DELIVERY_HANDOFF_MAX_MS
  const candidates = input.rows
    .filter(
      (row) =>
        row.authorKind === `user` &&
        row.authorId === input.viewerId &&
        row.card == null &&
        row.consumedAt != null &&
        Date.parse(row.consumedAt) >= since
    )
    .sort((a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  const taken = new Set<number>()
  const kept: string[] = []
  for (const row of candidates) {
    const body = row.body.trim()
    const from = Date.parse(row.createdAt) - MESSAGE_DELIVERY_MATCH_WINDOW_MS
    const echo = input.echoes.findIndex(
      (candidate, index) =>
        !taken.has(index) && candidate.text.trim() === body && (candidate.at == null || candidate.at >= from)
    )
    if (echo >= 0) taken.add(echo)
    else kept.push(row.id)
  }
  return kept
}

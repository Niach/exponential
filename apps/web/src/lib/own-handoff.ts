import {
  MESSAGE_DELIVERY_HANDOFF_MAX_MS,
  echoedPending,
  ownHandoff,
  type HandoffEcho,
  type HandoffRow,
} from "@exp/domain-contract/message-delivery"
import { subagentIdOf } from "@/lib/agent-feed"

/** A feed item as the handoff reads it (`steer-session-store` FeedItem). */
type HandoffFeedItem = { kind: string; text?: string; subagentId?: string; at?: number }

const NO_IDS: ReadonlySet<string> = new Set()

/** The transcript's person rows of the main lane. */
const feedEchoes = (feed: readonly HandoffFeedItem[]): HandoffEcho[] =>
  feed
    .filter((item) => item.kind === `user_message` && subagentIdOf(item) === null)
    .map((item) => ({ text: item.text ?? ``, at: typeof item.at === `number` ? item.at : null }))

/** #EXP-2129 (message-delivery.json ECHOED PENDING): the not-consumed rows
 *  the transcript echoes already; no pending bubble draws them. */
export function sessionEchoedPending(input: {
  rows: readonly HandoffRow[]
  feed: readonly HandoffFeedItem[]
}): ReadonlySet<string> {
  if (!input.rows.some((row) => row.authorKind === `user` && row.consumedAt == null)) return NO_IDS
  const ids = echoedPending({ rows: input.rows, echoes: feedEchoes(input.feed) })
  return ids.length === 0 ? NO_IDS : new Set(ids)
}

/** #EXP-2089 + #EXP-2129: the chat's rows drawn pending at its bottom: not
 *  the consumed ones (SENT INPUTS, HANDOFF), not what the transcript echoes. */
export function pendingChatRows<R extends { id: string; type: string }>(
  rows: readonly R[],
  drawn: { sent: ReadonlySet<string>; held: ReadonlySet<string>; echoed: ReadonlySet<string> }
): R[] {
  return rows.filter(
    (row) => row.type === `message` && !drawn.sent.has(row.id) && !drawn.held.has(row.id) && !drawn.echoed.has(row.id)
  )
}

/** #FEED-237 (message-delivery.json HANDOFF): the viewer's consumed own rows
 *  whose live-transcript echo has not arrived yet (they keep their bubble
 *  in the own tail, oldest first), and when the newest of them expires
 *  (`handoffMaxMs` after its consumedAt; null = nothing held). */
export function sessionOwnHandoff(input: {
  viewerId: string | null
  now: number
  feedEmpty: boolean
  rows: readonly HandoffRow[]
  feed: readonly HandoffFeedItem[]
}): { ids: ReadonlySet<string>; order: readonly string[]; expiresAt: number | null } {
  if (!input.viewerId || input.feedEmpty) return { ids: NO_IDS, order: [], expiresAt: null }
  const echoes = feedEchoes(input.feed)
  const order = ownHandoff({
    viewerId: input.viewerId,
    now: new Date(input.now).toISOString(),
    feedEmpty: false,
    rows: input.rows,
    echoes,
  })
  if (order.length === 0) return { ids: NO_IDS, order, expiresAt: null }
  const kept = new Set(order)
  let newest = -Infinity
  for (const row of input.rows) {
    if (kept.has(row.id) && row.consumedAt) newest = Math.max(newest, Date.parse(row.consumedAt))
  }
  return { ids: kept, order, expiresAt: Number.isFinite(newest) ? newest + MESSAGE_DELIVERY_HANDOFF_MAX_MS : null }
}

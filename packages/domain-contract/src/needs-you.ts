// #FEED-191 NEEDS YOU ×4 (fixtures/needs-you.json: its `$comment` is the
// spec): every plan and question waiting on the viewer. TS reference; desktop
// `domain::needs_you`, iOS ExpCore `NeedsYou.swift`, Android
// `domain/NeedsYou.kt` replay `cases`.

import fixture from "../fixtures/needs-you.json" with { type: "json" }
import { askDockLabel, askDockTitle } from "./ask-dock"
import { askPreview } from "./chats"
import { planCounts, planCountsLine, planRowLabel } from "./plan-page"
import { resultAge } from "./results-tab"
import type { TreeMessage } from "./session-tree"
import { cardState } from "./waiting-cards"

export const NEEDS_YOU_COPY = fixture.copy
export const NEEDS_YOU_TEST_IDS = fixture.testIds

export interface NeedsYouSessionInput {
  id: string
  rootId: string
  /** The session's display name (session-title; an agent conversation = the agent's). */
  name: string
  status: string
  agentId: string | null
  userId: string | null
}

export interface NeedsYouMessageInput {
  id: string
  sessionId: string
  authorSessionId: string | null
  body: string
  /** ISO timestamps, as the digests carry them. */
  createdAt: string
  answeredAt: string | null
  card: Record<string, unknown> | null
}

export interface NeedsYouRow {
  /** The waiting card's message: the origin's for a mirror it may read. */
  messageId: string
  sessionId: string
  rootId: string
  kind: `plan` | `question`
  /** `plan` = the chat's Plan tab; `card` = its card in the Conversation. */
  opens: `plan` | `card`
  title: string
  chat: string
  meta: string
  age: string
  createdAt: string
}

const ms = (iso: string) => {
  const at = Date.parse(iso)
  return Number.isNaN(at) ? 0 : at
}

const QUESTION_PREFIX = fixture.prefix.question

function rowOf(
  message: NeedsYouMessageInput,
  sessions: ReadonlyMap<string, NeedsYouSessionInput>,
  now: number
): NeedsYouRow | null {
  const card = message.card
  const session = sessions.get(message.sessionId)
  if (!card || !session) return null
  const root = sessions.get(session.rootId)
  if (!root) return null
  const base = {
    rootId: root.id,
    chat: root.name,
    age: resultAge(ms(message.createdAt), now),
    createdAt: message.createdAt,
  }
  if (card.kind === `plan` || card.kind === `question`) {
    if (cardState(card, message.answeredAt) !== `waiting`) return null
    const plan = card.kind === `plan`
    return {
      ...base,
      messageId: message.id,
      sessionId: session.id,
      kind: plan ? `plan` : `question`,
      opens: plan ? `plan` : `card`,
      title: plan
        ? planRowLabel(String(card.plan ?? ``))
        : askDockTitle(String(card.question ?? ``)) || askDockLabel(`question`),
      meta: plan ? planCountsLine(planCounts(card)) : ``,
    }
  }
  if (card.kind !== `ask` || message.answeredAt !== null) return null
  // A mirror: the origin's own ids where the viewer may read it, else the
  // mirror's, with the neutral copy (chats.json `askPreview`).
  const origin = typeof card.sessionId === `string` ? card.sessionId : message.authorSessionId
  const originSession = origin ? sessions.get(origin) : undefined
  const preview = askPreview(message as unknown as TreeMessage, new Set(sessions.keys()))
  const readable = Boolean(originSession) && typeof card.messageId === `string`
  const plan = preview.kind === `plan`
  return {
    ...base,
    messageId: readable ? String(card.messageId) : message.id,
    sessionId: readable ? originSession!.id : session.id,
    rootId: readable ? originSession!.rootId : root.id,
    chat: readable ? (sessions.get(originSession!.rootId)?.name ?? root.name) : root.name,
    kind: plan ? `plan` : `question`,
    opens: plan && readable ? `plan` : `card`,
    title: preview.text.startsWith(QUESTION_PREFIX) ? preview.text.slice(QUESTION_PREFIX.length) : preview.text,
    meta: ``,
  }
}

/** THE fold: one row per waiting card, once, newest first. */
export function needsYou(input: {
  now: number
  me: string | null
  sessions: readonly NeedsYouSessionInput[]
  messages: readonly NeedsYouMessageInput[]
}): NeedsYouRow[] {
  // Another member's agent conversation is theirs (chats.json `listAgents`).
  const live = input.sessions.filter(
    (session) => session.status !== `ended` && (session.agentId === null || session.userId === input.me)
  )
  const sessions = new Map(live.map((session) => [session.id, session]))
  const rows = new Map<string, NeedsYouRow>()
  const seen = new Set<string>()
  for (const message of input.messages) {
    if (seen.has(message.id)) continue
    seen.add(message.id)
    const row = rowOf(message, sessions, input.now)
    if (!row) continue
    const own = message.id === row.messageId
    // The card itself beats its mirror's copy of it.
    if (own || !rows.has(row.messageId)) rows.set(row.messageId, row)
  }
  return [...rows.values()].sort(
    (a, b) => ms(b.createdAt) - ms(a.createdAt) || (a.messageId < b.messageId ? 1 : a.messageId > b.messageId ? -1 : 0)
  )
}

// #EXP-1464: what a conversation's SESSION rows do when tapped ×4 (a
// thread's Conversation and a team agent's conversation alike). A session
// card whose session waits on a question or plan EXPANDS in place into that
// card; every other row naming a session (a session card, a `Report from`
// line, an event line such as `… was not picked up`) OPENS it by
// inbox-route.json `sessionTarget`. fixtures/thread-row-action.json; mirrors:
// desktop `domain::thread_row_action`, iOS ExpCore `ThreadRowAction.swift`,
// Android `domain/ThreadRowAction.kt`.
import fixture from "../fixtures/thread-row-action.json" with { type: "json" }
import type { SessionState } from "./session-tree"

export const THREAD_ROW_ACTION_COPY = fixture.copy
export const THREAD_ROW_ACTION_TEST_IDS = fixture.testIds

export interface ThreadRowActionInput {
  /** The Conversation row's type (`session`, `report`, `event`, …). */
  type: string
  /** The session the row names (null = none or not synced). */
  sessionId: string | null
  /** The conversation's own root (a row about it does nothing). */
  rootId: string
  /** That session's card state (session-card.json), null = unknown. */
  state: SessionState | null
  /** A watchdog or usage flag stands behind its `needs_you` (no card). */
  blocked: boolean
  /** The viewer answered the card this row expanded (client-held). */
  answered: boolean
}

export interface ThreadRowAction {
  action: `expand` | `open` | `none`
  sessionId: string | null
  /** Only a row that does something carries the chevron (and hover/press). */
  chevron: boolean
  /** Overrides the card's state word (`copy.answered`), null = its own. */
  kindWord: string | null
  /** Overrides the card's tone with the word, null = its own. */
  tone: string | null
}

const OPENING_TYPES = new Set([`session`, `report`, `event`])

const NONE: ThreadRowAction = { action: `none`, sessionId: null, chevron: false, kindWord: null, tone: null }

export function threadRowAction(input: ThreadRowActionInput): ThreadRowAction {
  const { type, sessionId } = input
  if (!sessionId || sessionId === input.rootId || !OPENING_TYPES.has(type)) return NONE
  if (type === `session` && input.state === `needs_you` && !input.blocked) {
    return { action: `expand`, sessionId, chevron: true, kindWord: null, tone: null }
  }
  const answered =
    type === `session` && input.answered && input.state !== null && input.state !== `needs_you` && input.state !== `done`
  return {
    action: `open`,
    sessionId,
    chevron: true,
    kindWord: answered ? fixture.copy.answered : null,
    tone: answered ? `muted` : null,
  }
}

/** The card an expanding row opens: the session's NEWEST waiting `question`
 *  (answeredAt null) or `plan` (no decision) message, else null. */
export function sessionWaitingCard(
  sessionId: string,
  messages: readonly {
    id: string
    sessionId: string
    createdAt: string
    answeredAt: string | null
    card: unknown
  }[]
): string | null {
  let best: { id: string; at: number } | null = null
  for (const message of messages) {
    if (message.sessionId !== sessionId) continue
    const card = message.card as { kind?: unknown; decision?: unknown } | null
    if (!card || typeof card !== `object`) continue
    const waiting =
      card.kind === `question`
        ? message.answeredAt === null
        : card.kind === `plan`
          ? card.decision === null || card.decision === undefined
          : false
    if (!waiting) continue
    const at = Date.parse(message.createdAt)
    if (!best || at > best.at || (at === best.at && message.id > best.id)) best = { id: message.id, at }
  }
  return best?.id ?? null
}

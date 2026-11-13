// #EXP-1561 / #EXP-1562: WAITING question and plan cards ×4 — a card's
// state (waiting / answered / dismissed / superseded), the dismissed card
// the server stamps, and the chat header's waiting chips. TS reference;
// desktop `domain::waiting_cards`, iOS ExpCore `WaitingCards.swift`,
// Android `domain/WaitingCards.kt` replay `fixtures/waiting-cards.json`.

import fixture from "../fixtures/waiting-cards.json" with { type: "json" }
import { askDockLabel, askDockTitle, type AskDockKind } from "./ask-dock"

export const WAITING_CARDS_COPY = fixture.copy
export const WAITING_CARDS_ICONS = fixture.icons
export const WAITING_CARDS_CONSTANTS = fixture.constants
export const WAITING_CARDS_TEST_IDS = fixture.testIds

export type CardState = `waiting` | `answered` | `dismissed` | `superseded`

/** A question/plan card's state; null = not a question or plan. */
export function cardState(card: unknown, answeredAt: string | null | undefined): CardState | null {
  if (!card || typeof card !== `object`) return null
  const c = card as { kind?: unknown; dismissed?: unknown; decision?: unknown; answer?: unknown }
  if (c.kind !== `plan` && c.kind !== `question`) return null
  if (c.dismissed === true || c.decision === `dismissed`) return `dismissed`
  if (c.kind === `plan`) {
    if (c.decision === `superseded`) return `superseded`
    return typeof c.decision === `string` ? `answered` : `waiting`
  }
  return answeredAt != null || typeof c.answer === `string` ? `answered` : `waiting`
}

/** The card the server stamps on a dismiss: `dismissed: true`; a plan's
 *  decision `dismissed` (never fresh); a sealed card keeps its envelope and
 *  gains `answered: true`. */
export function dismissedCard<T extends { kind: string }>(card: T): T & Record<string, unknown> {
  const next: Record<string, unknown> = { ...card }
  if (card.kind === `plan`) {
    next.decision = `dismissed`
    next.fresh = false
  }
  next.dismissed = true
  if (typeof (card as { sealed?: unknown }).sealed === `string`) next.answered = true
  return next as T & Record<string, unknown>
}

export interface WaitingCardInput {
  id: string
  kind: AskDockKind
  /** The plan, or the question's text (markdown). */
  text: string
}

export interface WaitingChip {
  id: string
  kind: AskDockKind
  /** The kind's concept icon. */
  icon: string
  label: string
}

export type WaitingChipsLayout = `hidden` | `chips` | `button`

/** A title clipped to `labelMax` characters (code points). */
export function clipWaitingLabel(title: string): string {
  const chars = Array.from(title)
  const max = fixture.constants.labelMax
  if (chars.length <= max) return title
  return `${chars.slice(0, max - 1).join(``).trimEnd()}${fixture.constants.ellipsis}`
}

/** The header's waiting chips (Conversation order kept): hidden when none
 *  waits, ONE button on phones, else one chip per card. */
export function waitingChips(
  cards: readonly WaitingCardInput[],
  phone: boolean
): { layout: WaitingChipsLayout; count: number; chips: WaitingChip[] } {
  const chips = cards.map((card) => ({
    id: card.id,
    kind: card.kind,
    icon: fixture.icons[card.kind],
    label: clipWaitingLabel(askDockTitle(card.text)) || askDockLabel(card.kind),
  }))
  const layout: WaitingChipsLayout = chips.length === 0 ? `hidden` : phone ? `button` : `chips`
  return { layout, count: chips.length, chips }
}

export type WaitingOpen = `plan` | `card` | `session`

/** #EXP-1785: where a picked chip (or menu row) goes: a plan = the plan
 *  page; the root's question = the Chat tab at its card; a question of a
 *  session under the root = that session opened in place at its card. */
export function waitingTarget(
  card: { kind: AskDockKind; sessionId: string },
  rootId: string
): { open: WaitingOpen; sessionId: string } {
  if (card.kind === `plan`) return { open: `plan`, sessionId: card.sessionId }
  return { open: card.sessionId === rootId ? `card` : `session`, sessionId: card.sessionId }
}

/** The phone button's accessible name. */
export function waitingCountLabel(count: number): string {
  return fixture.copy.waitingCount.replace(`{count}`, String(count))
}

export type DeepLinkStep = `ring` | `scroll` | `wait` | `page` | `missing`

/** What a pending deep link into a message knows this frame
 *  (waiting-cards.json DEEP LINKS). */
export interface DeepLinkInput {
  /** The target's row is in the view's loaded rows. */
  found: boolean
  /** That row was measured at its real height. */
  laidOut: boolean
  /** As last laid out the row sits centred (or the list can go no further). */
  centred: boolean
  /** An older page is in flight. */
  paging: boolean
  /** Older rows can still page in. */
  more: boolean
  /** Since the link was taken. */
  elapsedMs: number
}

/** The deep link's next step: ring it, scroll to it, wait a layout, page one
 *  older page in, or give up (`missing`). */
export function deepLinkStep(input: DeepLinkInput): DeepLinkStep {
  if (input.elapsedMs >= fixture.constants.deepLinkMs) return input.found ? `ring` : `missing`
  if (input.found) return !input.laidOut ? `wait` : input.centred ? `ring` : `scroll`
  if (input.paging) return `wait`
  return input.more ? `page` : `missing`
}

// THE ask cards ×4: a question or plan card waiting on the viewer is a
// DockCard of THE Dock (src/dock.ts, fixtures/dock.json); the composer never
// leaves. This file keeps the cards' own words and models: the row's label
// and title, the plan card's options ONE list ×4 (`planOptions`), a thread's
// waiting cards. TS reference; desktop `domain::ask_dock`, iOS ExpCore
// `AskDock.swift`, Android `domain/AskDock.kt` replay `fixtures/ask-dock.json`.

import fixture from "../fixtures/ask-dock.json" with { type: "json" }
import { expToolRowName } from "./exp-tool-group"

export type AskDockKind = `plan` | `question`

export interface AskDockPending {
  /** The card's id (a `messages.id`, or the session view's ask id). */
  id: string
  kind: AskDockKind
  /** The plan, or the question's text (markdown). */
  text: string
  /** The asking session's name in a thread; null in the session view. */
  sessionName: string | null
  /** Epoch ms (stack order, oldest first). */
  createdAt: number
  /** A plan's question count (a question: absent). */
  questions?: number
}

export const ASK_DOCK_COPY = fixture.copy
export const ASK_DOCK_ICONS = fixture.icons

export interface PlanOption {
  key: string
  label: string
  description?: string
  decision: `approved` | `rejected`
  fresh: boolean
}

/** THE plan card's options ×4: Yes · Yes, and start with a fresh context ·
 *  No, keep planning (the last opens the feedback field). */
export const PLAN_OPTIONS = fixture.planOptions as readonly PlanOption[]

/** The option a decided plan card names as its answer. */
export function planDecisionLabel(decision: `approved` | `rejected` | `superseded`, fresh: boolean): string {
  // FEED-140: a plan the agent replaced with a newer one.
  if (decision === `superseded`) return fixture.copy.superseded
  const wantsFresh = decision === `approved` && fresh
  return PLAN_OPTIONS.find((o) => o.decision === decision && o.fresh === wantsFresh)?.label ?? ``
}

const BLOCK_MARKER = /^(#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+|\[[ xX]\]\s+)/

/** The first plain line of a markdown body: fenced code blocks skipped
 *  whole, block markers (heading, quote, list, task box) stripped, links reduced to their text,
 *  `**` / `__` / backticks dropped. */
export function askDockTitle(text: string): string {
  let fenced = false
  for (const raw of text.split(`\n`)) {
    let line = raw.trim()
    if (line.startsWith("```") || line.startsWith(`~~~`)) {
      fenced = !fenced
      continue
    }
    if (fenced) continue
    let stripped = line.replace(BLOCK_MARKER, ``)
    while (stripped !== line) {
      line = stripped
      stripped = line.replace(BLOCK_MARKER, ``)
    }
    line = line
      .replace(/\[([^\]]*)\]\([^)]*\)/g, `$1`)
      .replaceAll(`**`, ``)
      .replaceAll(`__`, ``)
      .replaceAll("`", ``)
      .trim()
    if (line !== ``) return line
  }
  return ``
}

export function askDockLabel(kind: AskDockKind): string {
  return kind === `plan` ? ASK_DOCK_COPY.planReady : ASK_DOCK_COPY.question
}

/** A thread's waiting cards, in Conversation order: every `plan` row with
 *  no decision and every unanswered `question` row, of ANY session in the
 *  tree. `sessionName` names the asking session (null = unnamed). */
export function threadPendingAsks(
  rows: readonly (
    | { type: `plan`; id: string; sessionId: string; plan: string; decision: string | null; questions?: readonly unknown[] }
    | { type: `question`; id: string; sessionId: string; question: string; answered: boolean }
    | { type: string; id: string }
  )[],
  sessionName: (sessionId: string) => string | null
): AskDockPending[] {
  const pending: AskDockPending[] = []
  rows.forEach((row, index) => {
    if (row.type === `plan` && `plan` in row && row.decision === null) {
      pending.push({
        id: row.id,
        kind: `plan`,
        text: row.plan,
        sessionName: sessionName(row.sessionId),
        createdAt: index,
        questions: row.questions?.length ?? 0,
      })
    } else if (row.type === `question` && `question` in row && !row.answered) {
      pending.push({ id: row.id, kind: `question`, text: row.question, sessionName: sessionName(row.sessionId), createdAt: index })
    }
  })
  return pending
}

/** A thread's card captions: each session TAB's name (nested ones
 *  included); the root (no tab) = null. */
export function threadSessionNames(
  tabs: readonly (
    | { kind: `session`; sessionId: string; name: string; nested: readonly { sessionId: string; name: string }[] }
    | { kind: string }
  )[]
): (sessionId: string) => string | null {
  const names = new Map<string, string>()
  for (const tab of tabs) {
    if (tab.kind !== `session` || !(`sessionId` in tab)) continue
    names.set(tab.sessionId, tab.name)
    for (const nested of tab.nested) names.set(nested.sessionId, nested.name)
  }
  return (sessionId) => names.get(sessionId) ?? null
}

// ── EXP-1398: THE session view's cards ×4 ───────────────────────────────────

export type SessionCardKind = `plan` | `question` | `surface`

export interface SessionCard {
  /** The card's `messages.id`. */
  id: string
  kind: SessionCardKind
  /** A plan with no decision / an unanswered question (Dock + answerable). */
  waiting: boolean
}

/** The session view's cards: its own conversation's `plan` / `question`
 *  cards a TOOL posted (no `questionId`) and its `surface` cards, oldest
 *  first (createdAt, then id). Each draws inline; a waiting one also docks. */
export function sessionCards(
  messages: readonly { id: string; createdAt: string; answeredAt: string | null; card: unknown }[]
): SessionCard[] {
  const cards: (SessionCard & { createdAt: string })[] = []
  for (const message of messages) {
    const card = message.card as { kind?: unknown; questionId?: unknown; decision?: unknown } | null
    if (!card || typeof card !== `object`) continue
    const engine = card.questionId !== undefined && card.questionId !== null
    if (card.kind === `plan` && !engine) {
      cards.push({ id: message.id, kind: `plan`, waiting: card.decision === null || card.decision === undefined, createdAt: message.createdAt })
    } else if (card.kind === `question` && !engine) {
      cards.push({ id: message.id, kind: `question`, waiting: message.answeredAt === null, createdAt: message.createdAt })
    } else if (card.kind === `surface`) {
      cards.push({ id: message.id, kind: `surface`, waiting: false, createdAt: message.createdAt })
    }
  }
  cards.sort((a, b) => {
    const at = Date.parse(a.createdAt) - Date.parse(b.createdAt)
    if (at !== 0) return at
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  })
  return cards.map(({ id, kind, waiting }) => ({ id, kind, waiting }))
}

/** The display row of the call that posts a card of `kind`. */
export const SESSION_CARD_CALLS: Readonly<Record<SessionCardKind, string>> = {
  plan: `session_self_plan`,
  question: `session_self_ask`,
  surface: `session_self_show`,
}

/** Where each session card draws: right after the SETTLED, unfailed call
 *  that posted it, paired newest back PER KIND; a card with no call left =
 *  `unanchored` (card order, the transcript's head). `anchors` keys = feed
 *  item ids, each list oldest first. */
export function anchorSessionCards(
  feed: readonly { id: number; kind: string; name?: string | null; settled?: boolean; failed?: boolean }[],
  cards: readonly { id: string; kind: SessionCardKind }[]
): { anchors: Map<number, string[]>; unanchored: string[] } {
  const calls = new Map<string, number[]>()
  for (const item of feed) {
    if (item.kind !== `tool` || item.settled !== true || item.failed === true) continue
    const row = expToolRowName(item.name)
    if (row === null) continue
    calls.set(row, [...(calls.get(row) ?? []), item.id])
  }
  const used = new Map<string, number>()
  const anchors = new Map<number, string[]>()
  const unanchored: string[] = []
  for (let i = cards.length - 1; i >= 0; i--) {
    const card = cards[i]!
    const row = SESSION_CARD_CALLS[card.kind]
    const skip = used.get(row) ?? 0
    used.set(row, skip + 1)
    const ids = calls.get(row) ?? []
    const call = ids[ids.length - 1 - skip]
    if (call === undefined) unanchored.unshift(card.id)
    else anchors.set(call, [card.id, ...(anchors.get(call) ?? [])])
  }
  return { anchors, unanchored }
}

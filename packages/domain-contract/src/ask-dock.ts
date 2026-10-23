// THE ask cards ×4: a question or plan card waiting on the viewer is a
// DockCard of THE Dock (src/dock.ts, fixtures/dock.json); the composer never
// leaves. This file keeps the cards' own words and models: the row's label
// and title, the plan card's options ONE list ×4 (`planOptions`), a thread's
// waiting cards. TS reference; desktop `domain::ask_dock`, iOS ExpCore
// `AskDock.swift`, Android `domain/AskDock.kt` replay `fixtures/ask-dock.json`.

import fixture from "../fixtures/ask-dock.json" with { type: "json" }

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
export function planDecisionLabel(decision: `approved` | `rejected`, fresh: boolean): string {
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

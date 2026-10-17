// THE ask dock ×4: the question/plan cards waiting on the viewer take the
// composer's place, in the session view AND the thread (one rule, no
// thread-only treatment). One pending card = the big card (the composer
// hides); several = compact cards stacked in the composer's slot, one tap
// grows a card into the big one (only that one answers), a back arrow
// returns to the stack. The plan card's options are ONE list ×4
// (`planOptions`). TS reference; desktop `domain::ask_dock`, iOS ExpCore
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
}

export interface AskDockRow {
  id: string
  kind: AskDockKind
  /** The card's header label ("Plan ready" / "Question"). */
  label: string
  /** The first plain line of the text (the row truncates it). */
  title: string
  /** The asking session's name, else null. */
  caption: string | null
}

export type AskDockMode = `none` | `single` | `stack` | `focused`

export interface AskDockView {
  mode: AskDockMode
  /** Whether the composer yields its slot to the dock. */
  composerHidden: boolean
  /** The back arrow (only a focused card out of a stack). */
  back: boolean
  /** The big card's id (`single` | `focused`), else null. */
  cardId: string | null
  /** The compact cards (`stack` only), oldest first. */
  rows: AskDockRow[]
}

export const ASK_DOCK_COPY = fixture.copy
export const ASK_DOCK_ICONS = fixture.icons
export const ASK_DOCK_TEST_IDS = fixture.testIds
export const ASK_DOCK_MOTION = fixture.motion
export const ASK_DOCK_BODY_MAX_HEIGHT_FRACTION = fixture.bodyMaxHeightFraction
export const ASK_DOCK_STACK_GAP = fixture.stackGap

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

export function askDock(input: {
  pending: readonly AskDockPending[]
  focusedId: string | null
}): AskDockView {
  const pending = [...input.pending].sort(
    (a, b) => a.createdAt - b.createdAt || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  )
  if (pending.length === 0) {
    return { mode: `none`, composerHidden: false, back: false, cardId: null, rows: [] }
  }
  if (pending.length === 1) {
    return { mode: `single`, composerHidden: true, back: false, cardId: pending[0]!.id, rows: [] }
  }
  const focused = pending.find((p) => p.id === input.focusedId)
  if (focused) {
    return { mode: `focused`, composerHidden: true, back: true, cardId: focused.id, rows: [] }
  }
  return {
    mode: `stack`,
    composerHidden: true,
    back: false,
    cardId: null,
    rows: pending.map((p) => {
      const title = askDockTitle(p.text)
      return {
        id: p.id,
        kind: p.kind,
        label: askDockLabel(p.kind),
        title: title === `` ? askDockLabel(p.kind) : title,
        caption: p.sessionName,
      }
    }),
  }
}

/** A thread's waiting cards, in Conversation order: every `plan` row with
 *  no decision and every unanswered `question` row, of ANY session in the
 *  tree. `sessionName` names the asking session (null = unnamed). */
export function threadPendingAsks(
  rows: readonly (
    | { type: `plan`; id: string; sessionId: string; plan: string; decision: string | null }
    | { type: `question`; id: string; sessionId: string; question: string; answered: boolean }
    | { type: string; id: string }
  )[],
  sessionName: (sessionId: string) => string | null
): AskDockPending[] {
  const pending: AskDockPending[] = []
  rows.forEach((row, index) => {
    if (row.type === `plan` && `plan` in row && row.decision === null) {
      pending.push({ id: row.id, kind: `plan`, text: row.plan, sessionName: sessionName(row.sessionId), createdAt: index })
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

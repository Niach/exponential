// N-12: THE Dock ×4: the floating band over the transcript's bottom edge
// holding the DockCards (a waiting plan or question, THE Queue, the queued
// messages, a status banner) above THE composer, which never leaves. Several
// cards pile like the toasts (`stackLayout`, src/stack.ts); a tap opens a
// card's bottom sheet. TS reference; desktop `domain::dock`, iOS ExpCore
// `Dock.swift`, Android `domain/Dock.kt` replay `fixtures/dock.json`.

import fixture from "../fixtures/dock.json" with { type: "json" }
import { askDockLabel, askDockTitle } from "./ask-dock"
import type { StackConstants } from "./stack"

export type DockKind = `plan` | `question` | `tasks` | `queued` | `suggestions` | `banner`
export type DockTint = `plan` | `question` | `muted` | `warning`

export type DockSource =
  | {
      kind: `plan`
      id: string
      /** The plan (markdown). */
      text: string
      /** The asking session's name in a thread; null in the session view. */
      sessionName: string | null
      /** How many questions the card asks. */
      questions: number
      /** Epoch ms (the asks' order, oldest first). */
      createdAt: number
    }
  | { kind: `question`; id: string; text: string; sessionName: string | null; createdAt: number }
  | {
      kind: `tasks`
      /** THE Queue (task-queue.json `taskQueue`), or null when nothing is to do. */
      queue: {
        todo: { title: string; items: readonly { label: string; description: string | null; state: string }[] }
        completed: { title: string } | null
      } | null
    }
  | { kind: `queued`; messages: readonly { id: string; text: string }[] }
  | {
      kind: `suggestions`
      /** The suggestions row's id (a `messages.id`). */
      id: string
      /** The header's count (`3 suggested tasks`). */
      label: string
      /** The header's `started` part, else null. */
      caption: string | null
      /** The first open item's title. */
      title: string
      /** How many items are still open. */
      open: number
    }
  | {
      kind: `banner`
      id: string
      /** Concept icon. */
      icon: string
      label: string
      text: string
      tone: `muted` | `warning`
      /** The banner's one button word, else null. */
      action: string | null
    }

export interface DockRow {
  id: string
  kind: DockKind
  tint: DockTint
  /** Concept icon. */
  icon: string
  label: string
  caption: string | null
  meta: string | null
  title: string
  /** A tap opens the sheet. */
  expandable: boolean
  /** A banner's button word, else null. */
  action: string | null
}

export type DockMode = `none` | `single` | `pile` | `fanned`

export interface DockView {
  mode: DockMode
  /** Top to bottom; the front card is the LAST. */
  rows: DockRow[]
  frontId: string | null
  /** The open sheet's card, when that card exists and is expandable. */
  openId: string | null
  /** How many piled cards stay out of sight (beyond `stack.visible`). */
  hidden: number
}

export const DOCK_COPY = fixture.copy
export const DOCK_ICONS = fixture.icons
export const DOCK_TEST_IDS = fixture.testIds
export const DOCK_STACK: StackConstants = fixture.stack
export const DOCK_SHEET = fixture.sheet
export const DOCK_MOTION = fixture.motion
/** How long after a host's data loaded an arriving card still counts as
 *  "already waiting" (never opens itself). */
export const DOCK_AUTO_OPEN_GRACE_MS = fixture.autoOpenGraceMs

const ORDER: Record<DockKind, number> = {
  queued: fixture.order.indexOf(`queued`),
  tasks: fixture.order.indexOf(`tasks`),
  suggestions: fixture.order.indexOf(`suggestions`),
  banner: fixture.order.indexOf(`banner`),
  question: fixture.order.indexOf(`ask`),
  plan: fixture.order.indexOf(`ask`),
}

function counted(n: number, copy: { one: string; many: string }): string {
  return n === 1 ? copy.one : copy.many.replace(`{count}`, String(n))
}

function compareIds(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0
}

/** The cards, in the Dock's order (queued, tasks, banners, then the asks
 *  oldest first). An empty queue or no queued message draws nothing. */
export function dockRows(sources: readonly DockSource[]): DockRow[] {
  const rows: { row: DockRow; createdAt: number }[] = []
  for (const source of sources) {
    if (source.kind === `queued`) {
      const first = source.messages[0]
      if (!first) continue
      rows.push({
        createdAt: 0,
        row: {
          id: `queued`,
          kind: `queued`,
          tint: `muted`,
          icon: fixture.icons.queued,
          label: counted(source.messages.length, fixture.copy.queued),
          caption: null,
          meta: null,
          title: first.text,
          expandable: true,
          action: null,
        },
      })
    } else if (source.kind === `tasks`) {
      const first = source.queue?.todo.items[0]
      if (!source.queue || !first) continue
      rows.push({
        createdAt: 0,
        row: {
          id: `tasks`,
          kind: `tasks`,
          tint: `muted`,
          icon: fixture.icons.tasks,
          label: source.queue.todo.title,
          caption: source.queue.completed?.title ?? null,
          meta: first.description,
          title: first.label,
          expandable: true,
          action: null,
        },
      })
    } else if (source.kind === `suggestions`) {
      rows.push({
        createdAt: 0,
        row: {
          id: source.id,
          kind: `suggestions`,
          tint: `muted`,
          icon: fixture.icons.suggestions,
          label: source.label,
          caption: source.caption,
          meta: null,
          title: source.title,
          expandable: true,
          action: null,
        },
      })
    } else if (source.kind === `banner`) {
      rows.push({
        createdAt: 0,
        row: {
          id: `banner:${source.id}`,
          kind: `banner`,
          tint: source.tone,
          icon: source.icon,
          label: source.label,
          caption: null,
          meta: null,
          title: source.text,
          expandable: false,
          action: source.action,
        },
      })
    } else {
      const label = askDockLabel(source.kind)
      const title = askDockTitle(source.text)
      rows.push({
        createdAt: source.createdAt,
        row: {
          id: source.id,
          kind: source.kind,
          tint: source.kind,
          icon: source.kind === `plan` ? fixture.icons.plan : fixture.icons.question,
          label,
          caption: source.sessionName,
          meta: source.kind === `plan` && source.questions > 0 ? counted(source.questions, fixture.copy.questions) : null,
          title: title === `` ? label : title,
          expandable: true,
          action: null,
        },
      })
    }
  }
  return rows
    .sort(
      (a, b) =>
        ORDER[a.row.kind] - ORDER[b.row.kind] || a.createdAt - b.createdAt || compareIds(a.row.id, b.row.id)
    )
    .map((entry) => entry.row)
}

/** The band: the pile (or the fan while `expanded`), the front card, the
 *  open sheet. */
export function dock(input: { rows: readonly DockRow[]; expanded: boolean; openId: string | null }): DockView {
  const rows = [...input.rows]
  const front = rows.at(-1) ?? null
  const open = input.openId === null ? null : (rows.find((row) => row.id === input.openId) ?? null)
  const mode: DockMode =
    rows.length === 0 ? `none` : rows.length === 1 ? `single` : input.expanded ? `fanned` : `pile`
  return {
    mode,
    rows,
    frontId: front?.id ?? null,
    openId: open?.expandable ? open.id : null,
    hidden: mode === `pile` ? Math.max(0, rows.length - fixture.stack.visible) : 0,
  }
}

/** The ask (plan or question) that just arrived, front-most first, else
 *  null. `previous` = every card id seen before this frame. */
export function dockAutoOpen(input: {
  previous: readonly string[]
  rows: readonly { id: string; kind: DockKind }[]
}): string | null {
  const seen = new Set(input.previous)
  for (let i = input.rows.length - 1; i >= 0; i -= 1) {
    const row = input.rows[i]!
    if ((row.kind === `plan` || row.kind === `question`) && !seen.has(row.id)) return row.id
  }
  return null
}

/** EXP-1323's suggested-tasks rows with any OPEN item, as Dock sources: the
 *  header splits at ` · ` into the label (the count) and the started
 *  caption; the title is the first open item's. A row with no open item is
 *  none of the Dock's (it stays in the Conversation). */
export function suggestionSources(
  rows: readonly (
    | { type: `suggestions`; id: string; header: string; items: readonly { state: string; title: string }[] }
    | { type: string; id: string }
  )[]
): Extract<DockSource, { kind: `suggestions` }>[] {
  const out: Extract<DockSource, { kind: `suggestions` }>[] = []
  for (const row of rows) {
    if (row.type !== `suggestions` || !(`items` in row)) continue
    const open = row.items.filter((item) => item.state === `open`)
    if (open.length === 0) continue
    const [label, caption] = row.header.split(` · `)
    out.push({ kind: `suggestions`, id: row.id, label: label ?? row.header, caption: caption ?? null, title: open[0]!.title, open: open.length })
  }
  return out
}

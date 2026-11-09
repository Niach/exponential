// #EXP-1732: THE session screen ×4 (a chat, an agent's or the Assistant's
// conversation, an app's editor session): the header tabs per kind, the
// gear, and the wide-screen beside rule. fixtures/session-screen.json (its
// `$comment` is the spec); mirrors: desktop `domain::session_screen`, iOS
// ExpCore `SessionScreen.swift`, Android `domain/SessionScreen.kt`.

import fixture from "../fixtures/session-screen.json" with { type: "json" }

export const SESSION_SCREEN_COPY = fixture.copy
export const SESSION_SCREEN_TEST_IDS = fixture.testIds
export const SESSION_SCREEN_BESIDE = fixture.beside

export type SessionScreenKind = `session` | `agent` | `assistant` | `app`
export type SessionScreenTab = `chat` | `issues` | `sessions` | `results` | `preview` | `data` | `versions`
export type SessionScreenOpen = `issue` | `session` | `plan` | `settings`
export type SessionScreenLayout = `chat` | `beside` | `tab` | `item`

export interface SessionScreenTabEntry {
  id: SessionScreenTab
  label: string
  /** A muted number after the label; null = none (0 included). */
  count: number | null
}

export interface SessionScreenCounts {
  issues: number
  sessions: number
  results: number
}

export interface SessionScreenInput {
  kind: string
  /** Web ≥ `SESSION_SCREEN_BESIDE.breakpoint` and the IDE. */
  wide: boolean
  /** The route's `?tab=` (legacy values included). */
  requested: string | null | undefined
  /** The item opened over the tab (an issue, a session, a plan, settings). */
  open: string | null | undefined
  counts: SessionScreenCounts
}

export interface SessionScreen {
  tabs: SessionScreenTabEntry[]
  active: SessionScreenTab
  gear: boolean
  open: SessionScreenOpen | null
  layout: SessionScreenLayout
}

type KindSpec = { tabs: string[]; gear: boolean; wideDefault: string }
const KINDS = fixture.kinds as Record<string, KindSpec>
const LEGACY = fixture.legacyTabs as Record<string, string>
const COUNTED = fixture.counted as string[]
const OPENS: readonly string[] = [`issue`, `session`, `plan`, `settings`]

/** The screen's kind from its ROOT session (an app wins over its agent). */
export function sessionScreenKind(input: {
  agentId: string | null | undefined
  appId: string | null | undefined
  assistant: boolean
}): SessionScreenKind {
  if (input.appId) return `app`
  if (input.agentId) return input.assistant ? `assistant` : `agent`
  return `session`
}

/** The header's subtitle: `status · device · trigger`, blanks dropped. */
export function sessionScreenSubtitle(input: {
  status: string | null | undefined
  device: string | null | undefined
  trigger: string | null | undefined
}): string {
  return [input.status, input.device, input.trigger]
    .map((part) => (part ?? ``).trim())
    .filter((part) => part.length > 0)
    .join(fixture.copy.separator)
}

/** The kind's tabs (no counts), e.g. for a skeleton header. */
export function sessionScreenTabs(kind: string): SessionScreenTab[] {
  return (KINDS[kind] ?? KINDS.session).tabs as SessionScreenTab[]
}

/** Whether the kind's header carries THE gear. */
export function sessionScreenGear(kind: string): boolean {
  return (KINDS[kind] ?? KINDS.session).gear
}

export function sessionScreen(input: SessionScreenInput): SessionScreen {
  const spec = KINDS[input.kind] ?? KINDS.session
  const labels = fixture.copy.tabs as Record<string, string>
  const counts = input.counts as unknown as Record<string, number>
  const tabs = spec.tabs.map((id) => {
    const value = COUNTED.includes(id) ? (counts[id] ?? 0) : 0
    return { id: id as SessionScreenTab, label: labels[id] ?? id, count: value > 0 ? value : null }
  })
  const requested = input.requested ?? null
  const req = requested !== null ? (LEGACY[requested] ?? requested) : null
  let open: SessionScreenOpen | null = input.open && OPENS.includes(input.open) ? (input.open as SessionScreenOpen) : null
  if (open === null && req === `settings`) open = `settings`
  if (open === `settings` && !spec.gear) open = null
  const follows = open === `session` ? `sessions` : open === `issue` ? `issues` : null
  const active = (
    follows !== null && spec.tabs.includes(follows)
      ? follows
      : req !== null && spec.tabs.includes(req)
        ? req
        : input.wide
          ? spec.wideDefault
          : `chat`
  ) as SessionScreenTab
  const layout: SessionScreenLayout = input.wide
    ? active !== `chat` || open !== null
      ? `beside`
      : `chat`
    : open !== null
      ? `item`
      : active === `chat`
        ? `chat`
        : `tab`
  return { tabs, active, gear: spec.gear, open, layout }
}

export interface SessionScreenStatusInput {
  status: string
  busy: boolean
  needsInput: boolean
  /** The tree's root. */
  root: boolean
}

/** THE tree's status word (`statusCases`): needs input > working > queued
 *  (the root) > done (the root ended) > idle. */
export function sessionScreenStatus(sessions: readonly SessionScreenStatusInput[]): string {
  const words = fixture.copy.status
  const live = sessions.filter((session) => session.status !== `ended`)
  const root = sessions.find((session) => session.root) ?? null
  if (live.some((session) => session.needsInput)) return words.needsInput
  if (live.some((session) => session.busy)) return words.working
  if (root?.status === `queued`) return words.queued
  if (root?.status === `ended`) return words.done
  return words.idle
}

/** TONE ×4: the subtitle wears apps.json `editorHeader`'s tones: `attention`
 *  (the needs-input amber) while the word is needs input (an app also while
 *  its device part is `noDeviceOnline`), `active` while working or queued,
 *  else `muted`. */
export function sessionScreenTone(status: string, offline = false): `attention` | `active` | `muted` {
  const words = fixture.copy.status
  if (status === words.needsInput || offline) return `attention`
  if (status === words.working || status === words.queued) return `active`
  return `muted`
}

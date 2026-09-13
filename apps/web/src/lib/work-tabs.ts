// EXP-870: the web's WORK TABS — browser-like tabs above the content card on
// md+, the twin of the desktop title-band strip (apps/desktop/crates/ui/src/
// screens.rs `TabKey`/`live_tab_plan`/`partition_tabs`). Same model, same
// rules:
//
//   * A tab is a WORK ITEM, not a URL. An issue and its run are ONE tab with
//     two faces (`issue` | `run`, the header's `Issue | Run` toggle); a run
//     that links no issue (chat, action, batch) is a run-only tab; a support
//     conversation is its own tab.
//   * EVERY live run of mine in the team (running | in_review, not stale —
//     automations and remote devices included) gets a tab automatically,
//     grouped first in stable order. Adding one never navigates.
//   * Closing a live tab records the run's state SIGNATURE (`LiveSig`): the
//     tab comes back only when that signature changes (it starts needing
//     input, it opens its PR). `agentBusy` is deliberately NOT in it — a run
//     flipping between turns must not re-open a tab every few seconds.
//   * Ended runs are never auto-added; a tab whose run ends stays until it is
//     closed (`live: false`).
//   * The ACTIVE tab is derived from the URL, never stored.
//
// Pure: no React, no storage, no router — every rule is a test
// (`work-tabs.test.ts`). The store is `hooks/use-work-tabs.ts`, the URL→tab
// wiring `components/team/work-tabs-sync.tsx`.

import type { CodingSession } from "@/db/schema"
import { blockedBadgeLabel } from "@/lib/agent-usage"

export type WorkTabFace = `issue` | `run`

/** A run's state signature — what makes a dismissed live tab come back. */
export interface LiveSig {
  /** Waiting on the person: a pending question/plan, or a usage wall. */
  attention: boolean
  /** Its PR is open (`in_review`). */
  review: boolean
}

export type WorkTab =
  | {
      kind: `issue`
      issueId: string
      face: WorkTabFace
      /** The run the Run face shows — bound by a route or a live run. */
      runId: string | null
      /** The `?from=` token the tab reopens with. */
      from: string | null
      live: boolean
    }
  | {
      kind: `run`
      runId: string
      from: string | null
      live: boolean
    }
  | {
      kind: `support`
      threadId: string
      from: string | null
    }

export interface WorkTabsState {
  tabs: WorkTab[]
  /** Closed live runs → the signature they were closed at. */
  dismissed: Record<string, LiveSig>
}

export const EMPTY_WORK_TABS: WorkTabsState = { tabs: [], dismissed: {} }

/** A tab's identity — one tab per work item. */
export function tabKey(tab: WorkTab): string {
  switch (tab.kind) {
    case `issue`:
      return `issue:${tab.issueId}`
    case `run`:
      return `run:${tab.runId}`
    case `support`:
      return `support:${tab.threadId}`
  }
}

/** The run a tab is bound to, when it has one. */
export function tabRunId(tab: WorkTab): string | null {
  return tab.kind === `support` ? null : tab.runId
}

export function tabIsLive(tab: WorkTab): boolean {
  return tab.kind !== `support` && tab.live
}

/** The signature of one live run. */
export function liveSig(
  session: Pick<CodingSession, `status` | `needsInput` | `blocked`>,
  now: Date
): LiveSig {
  return {
    attention:
      session.needsInput === true ||
      blockedBadgeLabel(session.blocked, now) !== null,
    review: session.status === `in_review`,
  }
}

function sameSig(a: LiveSig | undefined, b: LiveSig): boolean {
  return a !== undefined && a.attention === b.attention && a.review === b.review
}

// ── The route side ───────────────────────────────────────────────────────────

/** What the current URL names, before any collection lookup. */
export type RoutePath =
  | { kind: `issue`; boardSlug: string; identifier: string; from: string | null }
  | { kind: `run`; runId: string; from: string | null }
  | { kind: `support`; threadId: string; from: string | null }

/** The work item a pathname shows — `null` for every other screen (lists,
 * full pages, settings, the legacy redirect routes). */
export function routePathFromLocation(
  pathname: string,
  from: string | null | undefined
): RoutePath | null {
  const rest = pathname.match(/^\/t\/[^/]+(\/.*)?$/)?.[1]?.replace(/\/$/, ``)
  if (!rest) return null
  const token = from ? from : null
  // A hand-typed path with a broken `%` escape is simply not a work item —
  // never an exception in the team layout.
  const decode = (value: string) => {
    try {
      return decodeURIComponent(value)
    } catch {
      return null
    }
  }
  const issue = rest.match(/^\/boards\/([^/]+)\/issues\/([^/]+)$/)
  if (issue) {
    const boardSlug = decode(issue[1])
    const identifier = decode(issue[2])
    if (boardSlug === null || identifier === null) return null
    return { kind: `issue`, boardSlug, identifier, from: token }
  }
  const run = rest.match(/^\/sessions\/([^/]+)$/)
  if (run) {
    const runId = decode(run[1])
    return runId === null ? null : { kind: `run`, runId, from: token }
  }
  const thread = rest.match(/^\/support\/([^/]+)$/)
  if (thread) {
    const threadId = decode(thread[1])
    return threadId === null ? null : { kind: `support`, threadId, from: token }
  }
  return null
}

/** A route resolved against the collections — the issue's id, the run's
 * issue. */
export type RouteTab =
  | { kind: `issue`; issueId: string; from: string | null }
  | { kind: `run`; runId: string; issueId: string | null; from: string | null }
  | { kind: `support`; threadId: string; from: string | null }

/** The key of the tab a resolved route shows — the active tab. */
export function routeTabKey(route: RouteTab): string {
  switch (route.kind) {
    case `issue`:
      return `issue:${route.issueId}`
    case `run`:
      return route.issueId ? `issue:${route.issueId}` : `run:${route.runId}`
    case `support`:
      return `support:${route.threadId}`
  }
}

/** The face a resolved route shows. */
export function routeTabFace(route: RouteTab): WorkTabFace | null {
  if (route.kind === `support`) return null
  return route.kind === `issue` ? `issue` : `run`
}

/**
 * The URL opened a work item: focus its tab, creating it at the end when
 * absent. An EXPLICIT `from` on the route overwrites the stored one (a tab
 * reopened from another list follows that list); a route without one keeps
 * what the tab had. A run that syncs its `issueId` after its run tab was made
 * MERGES into the issue's tab, which takes the run tab's place.
 */
export function upsertFromRoute(
  state: WorkTabsState,
  route: RouteTab
): WorkTabsState {
  const tabs = [...state.tabs]
  const nextFrom = (stored: string | null) => route.from ?? stored

  if (route.kind === `support`) {
    const at = tabs.findIndex(
      (tab) => tab.kind === `support` && tab.threadId === route.threadId
    )
    if (at < 0) {
      tabs.push({ kind: `support`, threadId: route.threadId, from: route.from })
    } else {
      const tab = tabs[at] as Extract<WorkTab, { kind: `support` }>
      if (tab.from === nextFrom(tab.from)) return state
      tabs[at] = { ...tab, from: nextFrom(tab.from) }
    }
    return { ...state, tabs }
  }

  if (route.kind === `run` && !route.issueId) {
    const at = tabs.findIndex(
      (tab) => tab.kind === `run` && tab.runId === route.runId
    )
    if (at < 0) {
      tabs.push({ kind: `run`, runId: route.runId, from: route.from, live: false })
      return { ...state, tabs }
    }
    const tab = tabs[at] as Extract<WorkTab, { kind: `run` }>
    if (tab.from === nextFrom(tab.from)) return state
    tabs[at] = { ...tab, from: nextFrom(tab.from) }
    return { ...state, tabs }
  }

  const issueId = route.kind === `issue` ? route.issueId : route.issueId!
  const runId = route.kind === `run` ? route.runId : null
  const face: WorkTabFace = route.kind === `issue` ? `issue` : `run`
  let at = tabs.findIndex(
    (tab) => tab.kind === `issue` && tab.issueId === issueId
  )
  // The run's own run-only tab (made before its issue id synced) folds in.
  const orphan = runId
    ? tabs.findIndex((tab) => tab.kind === `run` && tab.runId === runId)
    : -1
  let orphanLive = false
  if (orphan >= 0) {
    orphanLive = (tabs[orphan] as Extract<WorkTab, { kind: `run` }>).live
    if (at < 0) {
      const old = tabs[orphan] as Extract<WorkTab, { kind: `run` }>
      tabs[orphan] = {
        kind: `issue`,
        issueId,
        face,
        runId,
        from: nextFrom(old.from),
        live: old.live,
      }
      return { ...state, tabs }
    }
    tabs.splice(orphan, 1)
    if (orphan < at) at -= 1
  }
  if (at < 0) {
    tabs.push({
      kind: `issue`,
      issueId,
      face,
      runId,
      from: route.from,
      live: false,
    })
    return { ...state, tabs }
  }
  const tab = tabs[at] as Extract<WorkTab, { kind: `issue` }>
  const next: WorkTab = {
    ...tab,
    face,
    // The Issue face keeps whatever run the tab already shows.
    runId: runId ?? tab.runId,
    from: nextFrom(tab.from),
    // A different run under the tab: its liveness is the folded-in run tab's
    // until the next reconcile says otherwise.
    live: runId !== null && runId !== tab.runId ? orphanLive : tab.live,
  }
  if (
    orphan < 0 &&
    next.face === tab.face &&
    next.runId === tab.runId &&
    next.from === tab.from &&
    next.live === tab.live
  ) {
    return state
  }
  tabs[at] = next
  return { ...state, tabs }
}

// ── The live side ────────────────────────────────────────────────────────────

/** One live run of mine, as the reconcile needs it. */
export interface LiveRun {
  runId: string
  issueId: string | null
  sig: LiveSig
}

/**
 * Bind, mark and auto-add live runs — the desktop's `live_tab_plan`:
 *
 *   * a tab bound to a live run is `live`; an issue tab with no live binding
 *     binds to its issue's live run (a resume swaps the run under the tab);
 *   * a live run with no tab gets one (face `run`, appended — `orderedTabs`
 *     groups it first) unless it was closed at THIS signature;
 *   * a tab whose run is no longer live keeps its place with `live: false`;
 *   * dismissals of runs that are not live any more are forgotten (ended runs
 *     are never auto-added, so nothing is left for them to suppress).
 */
export function reconcileLive(
  state: WorkTabsState,
  runs: readonly LiveRun[],
  // The run the URL is showing right now: a tab bound to it is being READ
  // (a past run of an issue that also has a live one), so it is never
  // rebound out from under the reader.
  viewedRunId: string | null = null
): WorkTabsState {
  const liveById = new Map(runs.map((run) => [run.runId, run]))
  const liveByIssue = new Map<string, LiveRun>()
  for (const run of runs) {
    if (run.issueId && !liveByIssue.has(run.issueId)) {
      liveByIssue.set(run.issueId, run)
    }
  }
  const bound = new Set<string>()
  let changed = false

  const tabs: WorkTab[] = state.tabs.map((tab) => {
    if (tab.kind === `support`) return tab
    if (tab.kind === `run`) {
      const live = liveById.has(tab.runId)
      if (live) bound.add(tab.runId)
      if (live === tab.live) return tab
      changed = true
      return { ...tab, live }
    }
    let runId = tab.runId
    if ((!runId || !liveById.has(runId)) && (viewedRunId === null || runId !== viewedRunId)) {
      const issueRun = liveByIssue.get(tab.issueId)
      if (issueRun && !bound.has(issueRun.runId)) runId = issueRun.runId
    }
    const live = runId !== null && liveById.has(runId)
    if (live && runId) bound.add(runId)
    if (runId === tab.runId && live === tab.live) return tab
    changed = true
    return { ...tab, runId, live }
  })

  const dismissed: Record<string, LiveSig> = {}
  for (const [runId, sig] of Object.entries(state.dismissed)) {
    if (liveById.has(runId)) dismissed[runId] = sig
    else changed = true
  }

  for (const run of runs) {
    if (bound.has(run.runId)) continue
    // An issue tab may already hold this run's issue with another live run.
    if (
      run.issueId &&
      tabs.some((tab) => tab.kind === `issue` && tab.issueId === run.issueId)
    ) {
      continue
    }
    if (sameSig(dismissed[run.runId], run.sig)) continue
    delete dismissed[run.runId]
    changed = true
    bound.add(run.runId)
    tabs.push(
      run.issueId
        ? {
            kind: `issue`,
            issueId: run.issueId,
            face: `run`,
            runId: run.runId,
            from: null,
            live: true,
          }
        : { kind: `run`, runId: run.runId, from: null, live: true }
    )
  }

  return changed ? { tabs, dismissed } : state
}

/** Close tabs. A LIVE tab records its run's signature so reconcile keeps it
 * closed until that changes; `sigOf` is the caller's current signature for a
 * run (null = not live). Closing never ends or kills a run. */
export function closeTabs(
  state: WorkTabsState,
  keys: readonly string[],
  sigOf: (runId: string) => LiveSig | null
): WorkTabsState {
  const closing = new Set(keys)
  if (!state.tabs.some((tab) => closing.has(tabKey(tab)))) return state
  const dismissed = { ...state.dismissed }
  const tabs = state.tabs.filter((tab) => {
    if (!closing.has(tabKey(tab))) return true
    const runId = tabRunId(tab)
    if (runId && tabIsLive(tab)) {
      const sig = sigOf(runId)
      if (sig) dismissed[runId] = sig
    }
    return false
  })
  return { tabs, dismissed }
}

/** Drop tabs whose target no longer resolves (a deleted issue, a run of a
 * team the user left). Only call once the collections are READY — a cold
 * load must not wipe tabs whose rows simply have not synced yet. */
export function pruneTabs(
  state: WorkTabsState,
  resolves: (tab: WorkTab) => boolean
): WorkTabsState {
  const tabs = state.tabs.filter(resolves)
  return tabs.length === state.tabs.length ? state : { ...state, tabs }
}

/** Strip order: live tabs first, each group in its stored order. */
export function orderedTabs(tabs: readonly WorkTab[]): WorkTab[] {
  return [...tabs.filter(tabIsLive), ...tabs.filter((tab) => !tabIsLive(tab))]
}

/** Where a tab click goes. `issueHref` resolves an issue id to its board slug
 * and identifier (null while it syncs — the tab then waits). */
export function tabHref(
  teamSlug: string,
  tab: WorkTab,
  resolveIssue: (issueId: string) => { boardSlug: string; identifier: string } | null
): { to: string; params: Record<string, string>; search: { from?: string } } | null {
  const search = tab.from ? { from: tab.from } : {}
  switch (tab.kind) {
    case `support`:
      return {
        to: `/t/$teamSlug/support/$threadId`,
        params: { teamSlug, threadId: tab.threadId },
        search,
      }
    case `run`:
      return {
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: tab.runId },
        search,
      }
    case `issue`: {
      if (tab.face === `run` && tab.runId) {
        return {
          to: `/t/$teamSlug/sessions/$sessionId`,
          params: { teamSlug, sessionId: tab.runId },
          search,
        }
      }
      const issue = resolveIssue(tab.issueId)
      if (!issue) return null
      return {
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: {
          teamSlug,
          boardSlug: issue.boardSlug,
          issueIdentifier: issue.identifier,
        },
        search,
      }
    }
  }
}

// ── The strip's overflow ─────────────────────────────────────────────────────

/**
 * Which tabs get a chip, in strip order — the rest collapse into the "+N"
 * dropdown. A port of the desktop's `partition_tabs` (EXP-288/326/343), same
 * vectors: chips pack in order until the next would not fit; the overflow
 * button's width is reserved only once something overflows; the ACTIVE chip's
 * width is committed first so it is always visible; one chip always survives.
 */
export function partitionTabs(
  widths: readonly number[],
  available: number,
  gap: number,
  overflowW: number,
  activeIndex: number | null
): number[] {
  const count = widths.length
  const room = Math.max(available, 0)
  const total =
    widths.reduce((sum, width) => sum + width, 0) + gap * Math.max(count - 1, 0)
  if (total <= room) return widths.map((_, index) => index)

  const budget = Math.max(room - overflowW - gap, 0)
  const visible: number[] = []
  let used = activeIndex === null ? 0 : widths[activeIndex]
  let chips = activeIndex === null ? 0 : 1
  for (let index = 0; index < count; index++) {
    if (index === activeIndex) {
      visible.push(index)
      continue
    }
    const next = used + widths[index] + (chips > 0 ? gap : 0)
    if (next > budget && chips > 0) break
    visible.push(index)
    used = next
    chips += 1
  }
  if (activeIndex !== null && !visible.includes(activeIndex)) {
    visible.push(activeIndex)
  }
  if (visible.length === 0 && count > 0) visible.push(0)
  return visible
}

// ── Persistence ──────────────────────────────────────────────────────────────

export function workTabsStorageKey(teamId: string): string {
  return `exp:work-tabs:v1:${teamId}`
}

function isSig(value: unknown): value is LiveSig {
  return (
    typeof value === `object` &&
    value !== null &&
    typeof (value as LiveSig).attention === `boolean` &&
    typeof (value as LiveSig).review === `boolean`
  )
}

function str(value: unknown): value is string {
  return typeof value === `string` && value !== ``
}

function optStr(value: unknown): string | null {
  return str(value) ? value : null
}

function parseTab(value: unknown): WorkTab | null {
  if (typeof value !== `object` || value === null) return null
  const raw = value as Record<string, unknown>
  switch (raw.kind) {
    case `issue`:
      if (!str(raw.issueId)) return null
      return {
        kind: `issue`,
        issueId: raw.issueId,
        face: raw.face === `run` && str(raw.runId) ? `run` : `issue`,
        runId: optStr(raw.runId),
        from: optStr(raw.from),
        live: raw.live === true,
      }
    case `run`:
      if (!str(raw.runId)) return null
      return {
        kind: `run`,
        runId: raw.runId,
        from: optStr(raw.from),
        live: raw.live === true,
      }
    case `support`:
      if (!str(raw.threadId)) return null
      return { kind: `support`, threadId: raw.threadId, from: optStr(raw.from) }
    default:
      return null
  }
}

/** Read a stored state back. Anything malformed is dropped — a corrupt or
 * foreign blob is an empty strip, never a crash. Duplicate keys keep the
 * first. */
export function parseWorkTabsState(raw: string | null | undefined): WorkTabsState {
  if (!raw) return EMPTY_WORK_TABS
  let json: unknown
  try {
    json = JSON.parse(raw)
  } catch {
    return EMPTY_WORK_TABS
  }
  if (typeof json !== `object` || json === null) return EMPTY_WORK_TABS
  const source = json as { tabs?: unknown; dismissed?: unknown }
  const seen = new Set<string>()
  const tabs: WorkTab[] = []
  for (const entry of Array.isArray(source.tabs) ? source.tabs : []) {
    const tab = parseTab(entry)
    if (!tab || seen.has(tabKey(tab))) continue
    seen.add(tabKey(tab))
    tabs.push(tab)
  }
  const dismissed: Record<string, LiveSig> = {}
  if (typeof source.dismissed === `object` && source.dismissed !== null) {
    for (const [runId, sig] of Object.entries(source.dismissed)) {
      if (isSig(sig)) dismissed[runId] = { attention: sig.attention, review: sig.review }
    }
  }
  return { tabs, dismissed }
}

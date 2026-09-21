// EXP-870: the web's WORK TABS — browser-like tabs above the content card on
// md+, the twin of the desktop title-band strip (apps/desktop/crates/ui/src/
// screens.rs `TabKey`/`partition_tabs`). Same model, same rules:
//
//   * A tab is a WORK ITEM, not a URL. An issue and its run are ONE tab with
//     two faces (`issue` | `run`, the header's `Issue | Run` toggle); a run
//     that links no issue (chat, action, batch) is a run-only tab; a support
//     conversation is its own tab.
//   * EXP-923: a LIVE run is NOT a tab. Every running run of mine lives in the
//     sidebar's "Running" section instead (`components/team/sidebar-running`),
//     and opening one from there navigates with the `running` origin, which
//     creates NO tab (`upsertFromRoute`) — it only reuses an issue tab that is
//     already open. The agent groups the strip used to draw went with it.
//   * A tab is bound to a run all the same (`reconcileLive`), so its chip can
//     draw the live dot and its Run face knows which run to steer; `live` is a
//     RENDERING fact now, never a permanence one: EVERY tab closes.
//   * A run that ends because its PR MERGED takes its tab with it
//     (`closeMergedRunTabs`); every other ending leaves the tab standing.
//   * The ACTIVE tab is derived from the URL, never stored.
//
// Pure: no React, no storage, no router — every rule is a test
// (`work-tabs.test.ts`). The store is `hooks/use-work-tabs.ts`, the URL→tab
// wiring `components/team/work-tabs-sync.tsx`.

export type WorkTabFace = `issue` | `run`

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
}

export const EMPTY_WORK_TABS: WorkTabsState = { tabs: [] }

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

// ── The route side ───────────────────────────────────────────────────────────

/** What the current URL names, before any collection lookup. */
export type RoutePath =
  | { kind: `issue`; boardSlug: string; identifier: string; from: string | null }
  | { kind: `run`; runId: string; from: string | null }
  | { kind: `support`; threadId: string; from: string | null }

/** The work item a pathname shows — `null` for every other screen (lists,
 * full pages, settings). */
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

/** EXP-923: the `?from=` token the sidebar's Running rows navigate with. A
 * navigation carrying it creates NO tab — the sidebar row IS the affordance,
 * and a live run must not pile a chip onto the strip behind your back. It
 * still FOCUSES a tab that is already open (you started the run from its
 * issue tab), which is why it is an origin rather than a separate route. */
export const TABLESS_ORIGIN = `running`

/** Whether a navigation's origin may create a tab. */
export function originCreatesTab(from: string | null | undefined): boolean {
  return from !== TABLESS_ORIGIN
}

/**
 * The URL opened a work item: focus its tab, creating it at the end when
 * absent. An EXPLICIT `from` on the route overwrites the stored one (a tab
 * reopened from another list follows that list); a route without one keeps
 * what the tab had. A run that syncs its `issueId` after its run tab was made
 * MERGES into the issue's tab, which takes the run tab's place.
 *
 * EXP-923: a `running`-origin route creates nothing and never restamps a
 * tab's stored origin — it only binds the run under a tab that already
 * exists.
 */
export function upsertFromRoute(
  state: WorkTabsState,
  route: RouteTab
): WorkTabsState {
  const tabs = [...state.tabs]
  const creates = originCreatesTab(route.from)
  const nextFrom = (stored: string | null) =>
    creates ? (route.from ?? stored) : stored

  if (route.kind === `support`) {
    const at = tabs.findIndex(
      (tab) => tab.kind === `support` && tab.threadId === route.threadId
    )
    if (at < 0) {
      if (!creates) return state
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
      if (!creates) return state
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
    if (!creates) return state
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
}

/**
 * Bind and MARK live runs — EXP-923 took the auto-add away (a live run is a
 * sidebar row now, never a chip), so this only keeps the open tabs honest:
 *
 *   * a tab bound to a live run is `live`; an issue tab with no live binding
 *     binds to its issue's live run (a resume swaps the run under the tab);
 *   * a tab whose run is no longer live keeps its place with `live: false`.
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

  return changed ? { tabs } : state
}

/** Close tabs. EXP-923: EVERY tab closes again — the live exception went with
 * the live tabs themselves. Closing never ends or kills a run. */
export function closeTabs(
  state: WorkTabsState,
  keys: readonly string[]
): WorkTabsState {
  const closing = new Set(keys)
  const closable = (tab: WorkTab) => closing.has(tabKey(tab))
  if (!state.tabs.some(closable)) return state
  return { tabs: state.tabs.filter((tab) => !closable(tab)) }
}

/** A run that just stopped being live, as the merge rule needs it. */
export interface EndedRun {
  runId: string
  issueId: string | null
  /** `coding_sessions.ended_by` — only `merge` closes anything. */
  endedBy: string | null
}

/**
 * EXP-923: a run whose PR MERGED is finished work, so its tab goes — together
 * with any other tab of the same issue (the Issue face has nothing left to
 * show either). Every OTHER ending (stopped, errored, swept) leaves the tab
 * standing: you still want to read it.
 */
export function closeMergedRunTabs(
  state: WorkTabsState,
  ended: readonly EndedRun[]
): WorkTabsState {
  const merged = ended.filter((run) => run.endedBy === `merge`)
  if (merged.length === 0) return state
  const runIds = new Set(merged.map((run) => run.runId))
  const issueIds = new Set(
    merged.flatMap((run) => (run.issueId ? [run.issueId] : []))
  )
  const keys = state.tabs
    .filter((tab) => {
      if (tab.kind === `support`) return false
      if (tab.runId !== null && runIds.has(tab.runId)) return true
      return tab.kind === `issue` && issueIds.has(tab.issueId)
    })
    .map(tabKey)
  return closeTabs(state, keys)
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
  // EXP-877: a state written by an older build carries a `dismissed` map —
  // read past it, the strip has no dismissal memory any more.
  const source = json as { tabs?: unknown }
  const seen = new Set<string>()
  const tabs: WorkTab[] = []
  for (const entry of Array.isArray(source.tabs) ? source.tabs : []) {
    const tab = parseTab(entry)
    if (!tab || seen.has(tabKey(tab))) continue
    seen.add(tabKey(tab))
    tabs.push(tab)
  }
  return { tabs }
}

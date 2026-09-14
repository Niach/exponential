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
//     grouped FIRST by agent (`groupedTabs`, contract order) in stable order.
//     Adding one never navigates.
//   * EXP-877: a live tab cannot be closed at all — no ×, no Close, and
//     `closeTabs` skips it. The dismissal memory that used to let one be
//     hidden until its state changed is gone with it: a run of mine is on the
//     strip for as long as it is alive.
//   * Ended runs are never auto-added; a tab whose run ends stays until it is
//     closed (`live: false`).
//   * The ACTIVE tab is derived from the URL, never stored.
//
// Pure: no React, no storage, no router — every rule is a test
// (`work-tabs.test.ts`). The store is `hooks/use-work-tabs.ts`, the URL→tab
// wiring `components/team/work-tabs-sync.tsx`.

import { contract } from "@exp/domain-contract"

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
  /** The run's `coding_sessions.agent` — which group its chip sits in
   *  (`groupedTabs`); null/unknown reads as claude. */
  agent: string | null
}

/**
 * Bind, mark and auto-add live runs — the desktop's `live_tab_plan`:
 *
 *   * a tab bound to a live run is `live`; an issue tab with no live binding
 *     binds to its issue's live run (a resume swaps the run under the tab);
 *   * a live run with no tab gets one (face `run`, appended — `orderedTabs`
 *     groups it first). EXP-877: unconditionally — a live tab cannot be
 *     closed, so a closed one that is live again simply comes back;
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

  for (const run of runs) {
    if (bound.has(run.runId)) continue
    // An issue tab may already hold this run's issue with another live run.
    if (
      run.issueId &&
      tabs.some((tab) => tab.kind === `issue` && tab.issueId === run.issueId)
    ) {
      continue
    }
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

  return changed ? { tabs } : state
}

/** Close tabs. EXP-877: a LIVE tab is never closed — the strip draws no × on
 * one, its context menu offers no Close, and Close others / Close all pass
 * over it. Closing never ends or kills a run. */
export function closeTabs(
  state: WorkTabsState,
  keys: readonly string[]
): WorkTabsState {
  const closing = new Set(keys)
  const closable = (tab: WorkTab) => closing.has(tabKey(tab)) && !tabIsLive(tab)
  if (!state.tabs.some(closable)) return state
  return { tabs: state.tabs.filter((tab) => !closable(tab)) }
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

// ── Agent groups (EXP-877) ───────────────────────────────────────────────────

/** The group a run belongs to: its agent, normalised. A row without one (or
 * with a blank) is a claude run — the same rule `AgentBrandMark` draws by. */
export function tabGroupAgent(agent: string | null | undefined): string {
  const id = (agent ?? ``).trim().toLowerCase()
  return id === `` ? KNOWN_AGENTS[0]! : id
}

const KNOWN_AGENTS: readonly string[] = contract.codingAgent.values

export interface WorkTabGroup {
  agent: string
  tabs: WorkTab[]
}

/**
 * EXP-877: the live tabs, grouped by AGENT — claude first, then codex (the
 * contract's own order), then any agent this build does not know, each in the
 * tabs' stored order. Empty groups are omitted entirely; everything that is
 * not live is `rest`, untouched and in stored order.
 */
export function groupedTabs(
  tabs: readonly WorkTab[],
  agentOf: (tab: WorkTab) => string | null | undefined
): { groups: WorkTabGroup[]; rest: WorkTab[] } {
  const byAgent = new Map<string, WorkTab[]>()
  const rest: WorkTab[] = []
  for (const tab of tabs) {
    if (!tabIsLive(tab)) {
      rest.push(tab)
      continue
    }
    const agent = tabGroupAgent(agentOf(tab))
    const bucket = byAgent.get(agent)
    if (bucket) bucket.push(tab)
    else byAgent.set(agent, [tab])
  }
  const groups: WorkTabGroup[] = []
  for (const agent of KNOWN_AGENTS) {
    const bucket = byAgent.get(agent)
    if (bucket) {
      groups.push({ agent, tabs: bucket })
      byAgent.delete(agent)
    }
  }
  // An agent this build has no contract value for still gets its own group,
  // after the known ones and in first-seen order.
  for (const [agent, bucket] of byAgent) groups.push({ agent, tabs: bucket })
  return { groups, rest }
}

/** Strip order: the live groups first (contract agent order), then everything
 * else in its stored order. */
export function orderedTabs(
  tabs: readonly WorkTab[],
  agentOf: (tab: WorkTab) => string | null | undefined = () => null
): WorkTab[] {
  const { groups, rest } = groupedTabs(tabs, agentOf)
  return [...groups.flatMap((group) => group.tabs), ...rest]
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

/** One thing the strip lays out, as the packing sees it. `packed` chips
 * compete for the row; everything else (a group's brand mark, its chevron, a
 * chip folded into a collapsed group) is ALWAYS drawn and only takes its width
 * off the budget. */
export interface StripUnitMetric {
  packed: boolean
  width: number
}

/**
 * EXP-877: which units the strip renders. The unpacked ones always survive —
 * a group mark past the packing break would strand its whole group, and its
 * chevron is what makes room in the first place — so their widths come off
 * the budget FIRST and only the chips are then packed (`partitionTabs`).
 * Returns unit indices in order; the packed ones it left out are the "+N".
 */
export function partitionUnits(
  units: readonly StripUnitMetric[],
  available: number,
  gap: number,
  overflowW: number,
  activeIndex: number | null
): number[] {
  const packed: number[] = []
  let fixed = 0
  units.forEach((unit, index) => {
    if (unit.packed) packed.push(index)
    else fixed += unit.width + gap
  })
  // The active unit's slot AMONG the packed ones — an active unit that is not
  // packed (it is always drawn) commits no width here.
  const activeAt = activeIndex === null ? -1 : packed.indexOf(activeIndex)
  const visiblePacked = partitionTabs(
    packed.map((index) => units[index]!.width),
    available - fixed,
    gap,
    overflowW,
    activeAt < 0 ? null : activeAt
  )
  const keep = new Set(visiblePacked.map((at) => packed[at]!))
  return units.flatMap((unit, index) =>
    !unit.packed || keep.has(index) ? [index] : []
  )
}

// ── Persistence ──────────────────────────────────────────────────────────────

export function workTabsStorageKey(teamId: string): string {
  return `exp:work-tabs:v1:${teamId}`
}

/** EXP-877: the collapsed agent groups — a SECOND store, per team and per
 * window, holding nothing but agent ids (a group folds to its brand mark). */
export function workTabGroupsStorageKey(teamId: string): string {
  return `exp:work-tab-groups:v1:${teamId}`
}

/** Read the collapsed-group list back: a string[] of agent ids, deduped.
 * Anything malformed is "nothing collapsed". */
export function parseCollapsedGroups(raw: string | null | undefined): string[] {
  if (!raw) return []
  let json: unknown
  try {
    json = JSON.parse(raw)
  } catch {
    return []
  }
  if (!Array.isArray(json)) return []
  const seen = new Set<string>()
  for (const entry of json) {
    if (typeof entry === `string` && entry !== ``) seen.add(entry)
  }
  return [...seen]
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

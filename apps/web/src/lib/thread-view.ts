// EXP-1284: what a THREAD looks like, resolved once and rendered identically
// ×4 (web here; desktop `domain/src/thread_view.rs`, iOS `ThreadView.swift`,
// Android `ThreadView.kt`), locked by `packages/domain-contract/fixtures/
// thread-view.json`. A thread holds n issues and m runs (`thread_items`);
// this module turns the synced rows into:
//
//   * its TITLE (the stored one, else the first run's name unless that is
//     the generic `Chat`, else the first issue's title, else `Chat`/`Thread`);
//   * its CHILD STRIP: Conversation, one chip per issue in link order, the
//     "+" and the Guide (dot when the thread's Guide has entries). On phones
//     the strip IS the pager;
//   * its CAPTIONS: the header's `2 runs · 2 devices` and a list row's
//     `2 issues · 1 run · macbook` (the device part only while a run is
//     live, naming the live runs' devices);
//   * its CONVERSATION: every run in start order as a `Session N` block (the product says SESSION) (state,
//     device, `resumed`, `started by Session K`), the issues its agent linked as
//     rows under it (`Linked` / `Filed and linked`), and the runs one parent
//     started as ONE parallel card once there are two or more of them;
//   * its LIST placement: Running (a live run of mine) vs Recent.
//
// Pure: no queries, no React. Callers pass what they synced; an item whose
// issue or run did not resolve is dropped (trashed board, purged run).

import { BUILTIN_CHAT_NAME } from "@/lib/builtin-actions"

export interface ThreadViewThread {
  id: string
  title: string | null
  results: unknown
  updatedAt: string
}

export interface ThreadViewItem {
  id: string
  kind: `issue` | `run`
  issueId: string | null
  sessionId: string | null
  source: `user` | `agent`
  addedBySessionId: string | null
  sortOrder: number
  createdAt: string
}

export interface ThreadViewIssue {
  id: string
  identifier: string
  title: string
  createdAt: string
}

export interface ThreadViewRun {
  id: string
  /** The run's identity subject (session identity: issue title, action
   *  name, chat title…), used where a run is named on its own. */
  name: string
  userId: string
  deviceLabel: string | null
  /** `running` | `in_review` | `ended`. */
  status: string
  needsInput: boolean
  agentBusy: boolean
  issueId: string | null
  parentSessionId: string | null
  resumedFromId: string | null
  startedAt: string
  endedAt: string | null
}

export type ThreadRunState = `live` | `needs_input` | `ended`

export type ThreadChild =
  | { kind: `conversation` }
  | { kind: `issue`; issueId: string; identifier: string; title: string }
  | { kind: `add` }
  | { kind: `guide`; dot: boolean }

export interface ThreadLinkRow {
  itemId: string
  issueId: string
  identifier: string
  title: string
  /** `Linked` | `Filed and linked` (the agent filed it, then linked it). */
  label: string
}

export interface ThreadRunBlock {
  kind: `run`
  runId: string
  /** `Session 1`, … in start order across the whole thread. */
  label: string
  /** The muted parts after the label: device, `resumed`, `started by Session K`. */
  caption: string[]
  state: ThreadRunState
  /** Ended runs fold to one line until Show work opens them. */
  folded: boolean
  links: ThreadLinkRow[]
}

export interface ThreadParallelRow {
  runId: string
  name: string
  issueId: string | null
  deviceLabel: string | null
  state: ThreadRunState
  startedAt: string
}

export interface ThreadParallelBlock {
  kind: `parallel`
  /** `Sessions 3 and 4`, `Sessions 3, 4 and 5`. */
  label: string
  /** `started by Session 2`. */
  caption: string[]
  /** Live while any of its runs is. */
  state: ThreadRunState
  runs: ThreadParallelRow[]
  links: ThreadLinkRow[]
}

export type ThreadBlock = ThreadRunBlock | ThreadParallelBlock

export interface ThreadView {
  title: string
  /** True when `title` is derived (the thread has no stored title). */
  titleDerived: boolean
  strip: ThreadChild[]
  headerCaption: string
  listCaption: string
  live: boolean
  needsInput: boolean
  busy: boolean
  conversation: ThreadBlock[]
  /** Run ids in start order (Run 1 first). */
  runIds: string[]
  /** Issue ids in link order. */
  issueIds: string[]
}

export const THREAD_FALLBACK_TITLE = `Thread`
export const THREAD_LINKED_LABEL = `Linked`
export const THREAD_FILED_LABEL = `Filed and linked`
/** An issue created at most this long before the agent linked it was FILED
 *  by that agent for the thread. */
export const THREAD_FILED_WINDOW_MS = 120_000

export function threadRunState(run: Pick<ThreadViewRun, `status` | `needsInput`>): ThreadRunState {
  if (run.status === `ended`) return `ended`
  return run.needsInput ? `needs_input` : `live`
}

function time(value: string | null | undefined): number {
  const parsed = value ? Date.parse(value) : NaN
  return Number.isFinite(parsed) ? parsed : 0
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`
}

/** `Sessions 3 and 4`-style joins: `3`, `3 and 4`, `3, 4 and 5`. */
function joinOrdinals(ordinals: number[]): string {
  if (ordinals.length <= 1) return ordinals.map(String).join(``)
  const head = ordinals.slice(0, -1).join(`, `)
  return `${head} and ${ordinals[ordinals.length - 1]}`
}

function devicesPart(runs: ThreadViewRun[]): string | null {
  const labels = [...new Set(runs.map((run) => run.deviceLabel?.trim()).filter(Boolean))] as string[]
  if (labels.length === 0) return null
  return labels.length === 1 ? labels[0]! : `${labels.length} devices`
}

function hasGuide(results: unknown): boolean {
  return Array.isArray(results) && results.length > 0
}

export function buildThreadView(input: {
  thread: ThreadViewThread
  items: ThreadViewItem[]
  issues: ThreadViewIssue[]
  runs: ThreadViewRun[]
}): ThreadView {
  const issuesById = new Map(input.issues.map((issue) => [issue.id, issue]))
  const runsById = new Map(input.runs.map((run) => [run.id, run]))
  const items = [...input.items].sort(
    (a, b) => a.sortOrder - b.sortOrder || time(a.createdAt) - time(b.createdAt) || a.id.localeCompare(b.id)
  )

  const issueItems = items.filter(
    (item) => item.kind === `issue` && item.issueId && issuesById.has(item.issueId)
  )
  const issues = issueItems.map((item) => issuesById.get(item.issueId!)!)
  const runs = items
    .filter((item) => item.kind === `run` && item.sessionId && runsById.has(item.sessionId))
    .map((item) => runsById.get(item.sessionId!)!)
    .sort((a, b) => time(a.startedAt) - time(b.startedAt) || a.id.localeCompare(b.id))

  // The thread is named after its first prompt (the first run's own name:
  // a chat's agent title, an action's name, the issue a run started on),
  // the first issue only while that run is still the generic `Chat`.
  const stored = input.thread.title?.trim()
  const firstRun = runs[0]?.name.trim() || ``
  const derived =
    (firstRun && firstRun !== BUILTIN_CHAT_NAME ? firstRun : ``) ||
    issues[0]?.title.trim() ||
    firstRun ||
    THREAD_FALLBACK_TITLE
  const title = stored || derived

  const strip: ThreadChild[] = [
    { kind: `conversation` },
    ...issues.map((issue) => ({
      kind: `issue` as const,
      issueId: issue.id,
      identifier: issue.identifier,
      title: issue.title,
    })),
    { kind: `add` },
    { kind: `guide`, dot: hasGuide(input.thread.results) },
  ]

  const liveRuns = runs.filter((run) => run.status !== `ended`)
  const runsPart = plural(runs.length, `session`, `sessions`)
  const headerParts = [runsPart, devicesPart(runs)].filter(Boolean) as string[]
  const issuesPart =
    issues.length === 0 ? `no issues` : plural(issues.length, `issue`, `issues`)
  const listParts = [issuesPart, runsPart, liveRuns.length ? devicesPart(liveRuns) : null].filter(
    Boolean
  ) as string[]

  const ordinal = new Map(runs.map((run, index) => [run.id, index + 1]))

  const linksOf = (runIds: Set<string>): ThreadLinkRow[] =>
    issueItems
      .filter(
        (item) =>
          item.source === `agent` && item.addedBySessionId != null && runIds.has(item.addedBySessionId)
      )
      .sort((a, b) => time(a.createdAt) - time(b.createdAt) || a.id.localeCompare(b.id))
      .map((item) => {
        const issue = issuesById.get(item.issueId!)!
        const filed = time(issue.createdAt) >= time(item.createdAt) - THREAD_FILED_WINDOW_MS
        return {
          itemId: item.id,
          issueId: issue.id,
          identifier: issue.identifier,
          title: issue.title,
          label: filed ? THREAD_FILED_LABEL : THREAD_LINKED_LABEL,
        }
      })

  // Children one in-thread parent started, grouped once there are 2+.
  const childrenOf = new Map<string, ThreadViewRun[]>()
  for (const run of runs) {
    if (run.parentSessionId && ordinal.has(run.parentSessionId) && !run.resumedFromId) {
      const list = childrenOf.get(run.parentSessionId) ?? []
      list.push(run)
      childrenOf.set(run.parentSessionId, list)
    }
  }

  const conversation: ThreadBlock[] = []
  const placed = new Set<string>()
  for (const run of runs) {
    if (placed.has(run.id)) continue
    const siblings =
      run.parentSessionId && !run.resumedFromId ? childrenOf.get(run.parentSessionId) : undefined
    const startedBy =
      run.parentSessionId && ordinal.has(run.parentSessionId)
        ? `started by Session ${ordinal.get(run.parentSessionId)}`
        : null
    if (siblings && siblings.length >= 2) {
      for (const sibling of siblings) placed.add(sibling.id)
      const states = siblings.map(threadRunState)
      conversation.push({
        kind: `parallel`,
        label: `Sessions ${joinOrdinals(siblings.map((sibling) => ordinal.get(sibling.id)!))}`,
        caption: startedBy ? [startedBy] : [],
        state: states.includes(`needs_input`)
          ? `needs_input`
          : states.includes(`live`)
            ? `live`
            : `ended`,
        runs: siblings.map((sibling) => ({
          runId: sibling.id,
          name: sibling.name,
          issueId: sibling.issueId,
          deviceLabel: sibling.deviceLabel,
          state: threadRunState(sibling),
          startedAt: sibling.startedAt,
        })),
        links: linksOf(new Set(siblings.map((sibling) => sibling.id))),
      })
      continue
    }
    placed.add(run.id)
    const state = threadRunState(run)
    const caption = [
      run.deviceLabel?.trim() || null,
      run.resumedFromId ? `resumed` : null,
      startedBy,
    ].filter(Boolean) as string[]
    conversation.push({
      kind: `run`,
      runId: run.id,
      label: `Session ${ordinal.get(run.id)}`,
      caption,
      state,
      folded: state === `ended`,
      links: linksOf(new Set([run.id])),
    })
  }

  return {
    title,
    titleDerived: !stored,
    strip,
    headerCaption: headerParts.join(` · `),
    listCaption: listParts.join(` · `),
    live: liveRuns.length > 0,
    needsInput: liveRuns.some((run) => run.needsInput),
    busy: liveRuns.some((run) => run.agentBusy),
    conversation,
    runIds: runs.map((run) => run.id),
    issueIds: issues.map((issue) => issue.id),
  }
}

export interface ThreadListEntry {
  threadId: string
  teamId: string
  updatedAt: string
  /** The viewer owns a live session in it. */
  mineLive: boolean
  /** The viewer has a session in it or created it (Recent lists only these). */
  mine: boolean
}

/** Running (a live session of mine) vs Recent (my other threads: a session of
 *  mine or created by me), newest activity first. Scope: `teamId` = the
 *  ACTIVE team (web, desktop); `null` = every team (phones, which band the
 *  rows per team). */
export function partitionThreadList<T extends ThreadListEntry>(
  entries: T[],
  scope: { teamId: string | null } = { teamId: null }
): { running: T[]; recent: T[] } {
  const byActivity = entries
    .filter((entry) => scope.teamId === null || entry.teamId === scope.teamId)
    .sort(
      (a, b) => time(b.updatedAt) - time(a.updatedAt) || a.threadId.localeCompare(b.threadId)
    )
  return {
    running: byActivity.filter((entry) => entry.mineLive),
    recent: byActivity.filter((entry) => !entry.mineLive && entry.mine),
  }
}

export interface ThreadPrCandidate {
  url: string
  /** `open` | `merged` | `closed` (any other value counts as closed). */
  state: string | null
  /** When the PR was stamped (its session's or issue's activity time). */
  at: string
}

/** The PR a thread's Guide and Merge capsule target: the newest OPEN PR,
 *  else the newest MERGED one, else none. Candidates = the PRs on the
 *  thread's sessions and issues. */
export function threadPrTarget(candidates: ThreadPrCandidate[]): string | null {
  const newest = (state: string) =>
    candidates
      .filter((candidate) => candidate.state === state && candidate.url)
      .sort((a, b) => time(b.at) - time(a.at) || a.url.localeCompare(b.url))[0]?.url ?? null
  return newest(`open`) ?? newest(`merged`)
}

/** The Guide entries a session block shows: the ones that session wrote
 *  (`sessionId`); entries filed before the stamp belong to the thread's
 *  FIRST session. The thread's Guide face shows every entry. */
export function sessionGuideEntries<T extends { sessionId?: string | null }>(
  results: readonly T[] | null | undefined,
  sessionId: string,
  firstSessionId: string | null
): T[] {
  return (results ?? []).filter((entry) =>
    entry.sessionId ? entry.sessionId === sessionId : sessionId === firstSessionId
  )
}

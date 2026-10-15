// EXP-1291 Threads v2: the session tree's view models, ONE reference ×4.
// Pure and byte-locked by the fixtures every mirror replays:
//   fixtures/session-card.json   → sessionCard()
//   fixtures/session-thread.json → buildSessionThread()
//   fixtures/threads-list.json   → partitionThreads() + listAgents()
// Mirrors: desktop `domain::session_tree`, iOS ExpCore `SessionTree.swift`,
// Android `domain/SessionTree.kt`. The fixtures' `$comment`s hold the rules;
// this file is the reference.

import cardFixture from "../fixtures/session-card.json" with { type: "json" }
import threadFixture from "../fixtures/session-thread.json" with { type: "json" }
import listFixture from "../fixtures/threads-list.json" with { type: "json" }

export interface TreeSession {
  id: string
  teamId: string
  parentId: string | null
  rootId: string
  kind: string
  userId: string
  /** The logical `devices.device_id` that runs it (null/absent = unset). */
  deviceId?: string | null
  status: string
  busy: boolean
  needsInput: boolean
  blocked: { window?: string | null; resetsAt?: string | null } | null
  shared: boolean
  title: string | null
  guide: { text?: string | null; label?: string | null }[] | null
  actionName: string | null
  name: string | null
  icon: string | null
  color: string | null
  sortOrder: number
  startedAt: string | null
  endedAt: string | null
  createdAt: string
  updatedAt: string
}

export interface TreeSessionIssue {
  sessionId: string
  issueId: string
  sortOrder: number
}

export interface TreeIssue {
  id: string
  identifier: string
  title: string
}

export interface TreeMessage {
  id: string
  sessionId: string
  authorKind: string
  authorId: string | null
  body: string
  card: Record<string, unknown> | null
  answeredAt: string | null
  consumedAt: string | null
  createdAt: string
}

export interface TreePullRequest {
  id: string
  number: number
}

export const SESSION_CARD_COPY = cardFixture.copy
export const SESSION_CARD_ICONS = cardFixture.icons
export const THREAD_COPY = threadFixture.copy
export const THREAD_TEST_IDS = threadFixture.testIds
export const THREADS_LIST_COPY = listFixture.copy

const ms = (iso: string | null | undefined): number | null => {
  if (!iso) return null
  const value = Date.parse(iso)
  return Number.isNaN(value) ? null : value
}

/** The work card's elapsed ladder: `37s`, `2m 04s`, `1h 03m`. */
export function formatElapsed(durationMs: number): string {
  const total = Math.max(0, Math.floor(durationMs / 1000))
  const hours = Math.floor(total / 3600)
  const minutes = Math.floor((total % 3600) / 60)
  const seconds = total % 60
  const pad = (value: number) => String(value).padStart(2, `0`)
  if (hours > 0) return `${hours}h ${pad(minutes)}m`
  if (minutes > 0) return `${minutes}m ${pad(seconds)}s`
  return `${seconds}s`
}

export type SessionState =
  | `queued`
  | `working`
  | `waiting`
  | `needs_you`
  | `blocked`
  | `ended`

/** One state word per session, the card's kind word and tone. */
export function sessionState(session: TreeSession): SessionState {
  if (session.status === `ended`) return `ended`
  if (session.needsInput) return `needs_you`
  if (session.blocked) return `blocked`
  if (session.status === `queued`) return `queued`
  if (session.busy) return `working`
  return `waiting`
}

/** Sessions of one tree in creation order (ties by id): `Session N` names. */
export function treeOrder(sessions: readonly TreeSession[]): TreeSession[] {
  return [...sessions].sort(
    (a, b) =>
      (ms(a.createdAt) ?? 0) - (ms(b.createdAt) ?? 0) ||
      (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  )
}

export interface SessionCard {
  sessionId: string
  icon: string
  name: string
  kindWord: string
  state: SessionState
  tone: string
  elapsed: string | null
  running: boolean
  testId: string
}

/**
 * THE session card = the Work card's header row (EXP-1240): icon, name,
 * the muted state word, trailing elapsed, a spinner while `running`. Tapping
 * it opens that session's raw view. `ordinal` = its 1-based place in the
 * tree's creation order (names an untitled session `Session N`).
 */
export function sessionCard(
  session: TreeSession,
  ordinal: number,
  now: string
): SessionCard {
  const state = sessionState(session)
  const start = ms(session.startedAt)
  const end = state === `ended` ? ms(session.endedAt) ?? ms(session.updatedAt) : ms(now)
  const elapsed =
    start !== null && end !== null && state !== `queued` ? formatElapsed(end - start) : null
  const title = session.title?.trim()
  return {
    sessionId: session.id,
    icon: SESSION_CARD_ICONS[state],
    name: title ? title : `${SESSION_CARD_COPY.session} ${ordinal}`,
    kindWord: SESSION_CARD_COPY.states[state],
    state,
    tone: cardFixture.tones[state],
    elapsed,
    running: state === `working`,
    testId: cardFixture.testIds.card,
  }
}

/** First line of a person's text, capped (a thread named by its request). */
function firstLine(body: string, max: number): string | null {
  const line = body
    .split(`\n`)
    .map((part) => part.trim())
    .find((part) => part.length > 0 && !part.startsWith(`![`))
  if (!line) return null
  return line.length > max ? `${line.slice(0, max - 1).trimEnd()}…` : line
}

/**
 * EXP-1316: a thread's title, never stored by a person (no rename ×4):
 * an agent's `name`; else `Run <action>`; else `Implement <IDENT>` (the
 * first synced issue by sortOrder, `+ <n-1>` with more); else the stored
 * title (the CLI's auto-name, written by `sessions.report`); else the first
 * line of the root's first person message (capped `titleMax`); else
 * `Untitled`. The server's `deriveSessionTitle` calls this.
 */
export function threadTitle(
  root: Pick<TreeSession, `id` | `title` | `actionName` | `name`> & { kind?: string | null },
  issues: readonly Pick<TreeIssue, `identifier`>[],
  messages: readonly Pick<TreeMessage, `sessionId` | `authorKind` | `body` | `createdAt`>[]
): string {
  const titles = threadFixture.titles
  const name = root.name?.trim()
  if (root.kind === `agent` && name) return name
  const action = root.actionName?.trim()
  if (action) return titles.action.replace(`{action}`, action)
  if (issues.length > 0) {
    const issue = issues[0].identifier
    return issues.length > 1
      ? titles.issues.replace(`{issue}`, issue).replace(`{more}`, String(issues.length - 1))
      : titles.issue.replace(`{issue}`, issue)
  }
  const stored = root.title?.trim()
  if (stored) return stored
  const first = [...messages]
    .filter((m) => m.sessionId === root.id && m.authorKind === `user`)
    .sort((a, b) => (ms(a.createdAt) ?? 0) - (ms(b.createdAt) ?? 0))[0]
  const line = first ? firstLine(first.body, threadFixture.titleMax) : null
  return line ?? THREAD_COPY.untitled
}

function rootIssues(
  rootId: string,
  sessionIssues: readonly TreeSessionIssue[],
  issues: readonly TreeIssue[]
): TreeIssue[] {
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  return sessionIssues
    .filter((link) => link.sessionId === rootId)
    .sort((a, b) => a.sortOrder - b.sortOrder)
    .map((link) => byId.get(link.issueId))
    .filter((issue): issue is TreeIssue => issue !== undefined)
}

/** A session chip's lead ×4: the thread row mark of THAT session, `ended`
 *  once it ended. */
export type SessionTabMarkState = ReturnType<typeof threadRowMarkState> | `ended`

export function sessionTabMarkState(
  session: Pick<TreeSession, `status` | `busy` | `needsInput`>
): SessionTabMarkState {
  if (session.status === `ended`) return `ended`
  return threadRowMarkState({ needsYou: session.needsInput, busy: session.busy })
}

export type ThreadTab =
  | { kind: `conversation` }
  | { kind: `issue`; issueId: string; identifier: string }
  | { kind: `add` }
  | { kind: `session`; sessionId: string; name: string; markState: SessionTabMarkState }
  | { kind: `guide`; dot: boolean }

/** EXP-1322: a question row's option, label + description like the session
 *  view's card (an undescribed card option → description null). */
export interface ThreadQuestionOption {
  label: string
  description: string | null
}

/** A synced card option (a plain label or `{label, description}`) in the
 *  row form; anything else = null (dropped). The sidecar description (see
 *  `questionOptions`) is zipped in by the caller. */
export function questionOption(raw: unknown): ThreadQuestionOption | null {
  if (typeof raw === `string`) return { label: raw, description: null }
  if (raw && typeof raw === `object` && typeof (raw as { label?: unknown }).label === `string`) {
    const { label, description } = raw as { label: string; description?: unknown }
    return { label, description: typeof description === `string` && description !== `` ? description : null }
  }
  return null
}

/** The question card's options in the row form. `card.options` carries plain
 *  label strings; the descriptions ride the parallel `card.optionDescriptions`
 *  sidecar (same length, null / "" = none; the key is omitted when no option
 *  has one): option i = `{label: options[i], description: optionDescriptions?.[i] || null}`.
 *  Cards posted before the sidecar carry `{label, description}` objects
 *  inside `options`; those keep reading (an object's own description wins
 *  over its sidecar slot). Zipped by index BEFORE dropping invalid options.
 *
 *  COMPAT (EXP-1322 sidecar): old iOS 0.14.59 / Android 0.14.61 / desktop
 *  0.14.69 only read string options. The device may post {label, description}
 *  objects again once CLIENT_MIN_VERSION_IOS >= 0.14.60 AND _ANDROID >= 0.14.62
 *  AND _DESKTOP >= 0.14.70 (or keep the sidecar as the permanent form). */
export function questionOptions(card: { options?: unknown; optionDescriptions?: unknown }): ThreadQuestionOption[] {
  if (!Array.isArray(card.options)) return []
  const descriptions = Array.isArray(card.optionDescriptions) ? card.optionDescriptions : []
  const rows: ThreadQuestionOption[] = []
  card.options.forEach((raw, index) => {
    const option = questionOption(raw)
    if (!option) return
    const sidecar: unknown = descriptions[index]
    rows.push({
      label: option.label,
      description: option.description ?? (typeof sidecar === `string` && sidecar !== `` ? sidecar : null),
    })
  })
  return rows
}

export type ThreadRow =
  | {
      type: `message`
      id: string
      author: `user` | `agent` | `system`
      authorId: string | null
      body: string
      queued: boolean
    }
  | { type: `session`; id: string; card: SessionCard }
  | {
      type: `question`
      id: string
      question: string
      options: ThreadQuestionOption[]
      custom: boolean
      answer: string | null
      answered: boolean
    }
  | { type: `plan`; id: string; plan: string; decision: string | null }
  | { type: `subagent`; id: string; title: string; status: string }
  | {
      type: `picture`
      id: string
      attachmentId: string
      width: number | null
      height: number | null
      caption: string | null
    }
  | { type: `event`; id: string; event: string; text: string }

export interface SessionThreadView {
  rootId: string
  title: string
  private: boolean
  focusId: string | null
  tabs: ThreadTab[]
  rows: ThreadRow[]
  composer: { sessionId: string; placeholder: string }
  live: boolean
  busy: boolean
  needsYou: boolean
}

function eventText(
  card: Record<string, unknown>,
  issues: ReadonlyMap<string, TreeIssue>,
  prs: ReadonlyMap<string, TreePullRequest>
): string | null {
  const event = String(card.event ?? ``)
  const template = (THREAD_COPY.events as Record<string, string>)[event]
  if (!template) return null
  const issue = typeof card.issueId === `string` ? issues.get(card.issueId) : undefined
  const pr = typeof card.pullRequestId === `string` ? prs.get(card.pullRequestId) : undefined
  if (template.includes(`{issue}`)) {
    if (!issue) return null
    return template.replace(`{issue}`, issue.identifier)
  }
  if (template.includes(`{pr}`)) {
    if (!pr) return null
    return template.replace(`{pr}`, `#${pr.number}`)
  }
  return template
}

export interface GuidePullRequestLike {
  id: string
  number: number
  state: string
  createdAt: string
}

/** EXP-1306: the ONE pull request the thread's Guide speaks for (its stack
 *  rail + the bottom Merge capsule): the newest open one, else the newest
 *  merged one (createdAt, ties by the higher number); a closed PR never counts. */
export function guidePullRequest<T extends GuidePullRequestLike>(
  pullRequests: readonly T[]
): T | null {
  const newest = (state: string) =>
    pullRequests
      .filter((pr) => pr.state === state)
      .reduce<T | null>((best, pr) => {
        if (!best) return pr
        const diff = (ms(pr.createdAt) ?? 0) - (ms(best.createdAt) ?? 0)
        return diff > 0 || (diff === 0 && pr.number > best.number) ? pr : best
      }, null)
  return newest(`open`) ?? newest(`merged`)
}

export type GuideMenuEntry = `open-github` | `close-pr`

/** EXP-1307: the thread header's `…` on the Guide: Open on GitHub whenever
 *  the Guide has a PR, Close PR while it is open and the viewer is a member;
 *  empty = no trigger. */
export function guideMenuEntries(input: {
  state: string | null
  member: boolean
}): GuideMenuEntry[] {
  if (input.state === null) return []
  return input.state === `open` && input.member ? [`open-github`, `close-pr`] : [`open-github`]
}

/** EXP-1306 (EXP-820 ×4): a question or plan card waiting on the viewer
 *  takes the session composer's place. */
export function composerYieldsToCard(input: { live: boolean; pendingAsk: boolean }): boolean {
  return input.live && input.pendingAsk
}

/**
 * The thread screen: the header tabs (Conversation · one chip per issue ·
 * + · session chips · Guide; the session chips = the viewed level: the
 * root's own run + its direct children, or with `focus` that session + ITS
 * direct children), the root's conversation as rows (the root's own session card
 * first, then its messages oldest first; a `session`/`thread_started` card
 * becomes that session's card) and the composer target (the root: a message
 * to an ended thread continues it server-side).
 */
export function buildSessionThread(input: {
  root: TreeSession
  sessions: readonly TreeSession[]
  sessionIssues: readonly TreeSessionIssue[]
  issues: readonly TreeIssue[]
  messages: readonly TreeMessage[]
  pullRequests: readonly TreePullRequest[]
  now: string
}, options: { focus?: string | null } = {}): SessionThreadView {
  const { root, now } = input
  const tree = treeOrder(input.sessions.filter((s) => s.rootId === root.id || s.id === root.id))
  if (!tree.some((s) => s.id === root.id)) tree.unshift(root)
  const ordinals = new Map(tree.map((s, index) => [s.id, index + 1]))
  const byId = new Map(input.sessions.map((s) => [s.id, s]))
  byId.set(root.id, root)
  const issues = rootIssues(root.id, input.sessionIssues, input.issues)
  const issueMap = new Map(input.issues.map((issue) => [issue.id, issue]))
  const prMap = new Map(input.pullRequests.map((pr) => [pr.id, pr]))
  const own = input.messages
    .filter((m) => m.sessionId === root.id)
    .sort(
      (a, b) =>
        (ms(a.createdAt) ?? 0) - (ms(b.createdAt) ?? 0) ||
        (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
    )
  const title = threadTitle(root, issues, own)
  const focus = options.focus ? tree.find((s) => s.id === options.focus) ?? null : null
  const level = focus ?? root
  const sessionTabs: ThreadTab[] = [
    level,
    ...tree.filter((s) => s.parentId === level.id && s.id !== level.id),
  ].map((s) => ({
    kind: `session` as const,
    sessionId: s.id,
    name: sessionCard(s, ordinals.get(s.id) ?? 1, now).name,
    markState: sessionTabMarkState(s),
  }))

  const rows: ThreadRow[] = [
    { type: `session`, id: root.id, card: sessionCard(root, 1, now) },
  ]
  for (const message of own) {
    const card = message.card
    if (!card) {
      const author =
        message.authorKind === `user` || message.authorKind === `agent`
          ? message.authorKind
          : `system`
      if (!message.body.trim()) continue
      rows.push({
        type: `message`,
        id: message.id,
        author,
        authorId: message.authorId,
        body: message.body,
        queued: author === `user` && message.consumedAt === null,
      })
      continue
    }
    switch (card.kind) {
      case `session`:
      case `thread_started`: {
        const target = typeof card.sessionId === `string` ? byId.get(card.sessionId) : undefined
        if (!target) break
        rows.push({
          type: `session`,
          id: message.id,
          card: sessionCard(target, ordinals.get(target.id) ?? 1, now),
        })
        break
      }
      case `question`:
        rows.push({
          type: `question`,
          id: message.id,
          question: String(card.question ?? ``),
          options: questionOptions(card),
          custom: true,
          answer: typeof card.answer === `string` ? card.answer : null,
          answered: message.answeredAt !== null,
        })
        break
      case `plan`:
        rows.push({
          type: `plan`,
          id: message.id,
          plan: String(card.plan ?? ``),
          decision: typeof card.decision === `string` ? card.decision : null,
        })
        break
      case `subagent`:
        rows.push({
          type: `subagent`,
          id: message.id,
          title: String(card.title ?? ``),
          status: String(card.status ?? ``),
        })
        break
      case `picture`:
        if (typeof card.attachmentId !== `string`) break
        rows.push({
          type: `picture`,
          id: message.id,
          attachmentId: card.attachmentId,
          width: typeof card.width === `number` ? card.width : null,
          height: typeof card.height === `number` ? card.height : null,
          caption: typeof card.caption === `string` ? card.caption : null,
        })
        break
      case `event`: {
        const text = eventText(card, issueMap, prMap)
        if (text) rows.push({ type: `event`, id: message.id, event: String(card.event), text })
        break
      }
      default:
        break
    }
  }

  const guideDot = (root.guide ?? []).some(
    (entry) => (entry.text ?? ``).trim().length > 0 || (entry.label ?? null) !== null
  )
  const live = tree.filter((s) => s.status !== `ended`)
  return {
    rootId: root.id,
    title,
    private: !root.shared,
    focusId: focus?.id ?? null,
    tabs: [
      { kind: `conversation` },
      ...issues.map((issue) => ({
        kind: `issue` as const,
        issueId: issue.id,
        identifier: issue.identifier,
      })),
      { kind: `add` },
      ...sessionTabs,
      { kind: `guide`, dot: guideDot },
    ],
    rows,
    composer: { sessionId: root.id, placeholder: THREAD_COPY.placeholder },
    live: live.length > 0,
    busy: live.some((s) => s.busy),
    needsYou: live.some((s) => s.needsInput),
  }
}

export interface ThreadsListRow {
  sessionId: string
  teamId: string
  title: string
  private: boolean
  /** EXP-1318: the ROOT's device (the row's trailing device icon);
   *  `partitionThreads` always sets it, hand-built rows may omit it (= null). */
  deviceId?: string | null
  busy: boolean
  needsYou: boolean
  activityAt: string
}

/** EXP-1312: a thread row's lead outside the Threads tab (home Running band,
 *  rail Running rows, work tabs) ×4: needs you beats working, else the bare
 *  mark (`threads-list.json` `markCases`). */
export function threadRowMarkState(row: {
  needsYou: boolean
  busy: boolean
}): `needs_you` | `working` | `waiting` {
  if (row.needsYou) return `needs_you`
  if (row.busy) return `working`
  return `waiting`
}

export interface ThreadsListGroup {
  key: `needs_you` | `running` | `recent`
  title: string
  rows: ThreadsListRow[]
}

/**
 * The Threads page/rail list: roots of kind `thread` in scope (`teamId` =
 * the ACTIVE team; null = every team, phones band rows per team), grouped
 * Needs you (a live session of the tree waits on a person) · Running (any
 * session queued or running) · Recent (the rest), each by the tree's newest
 * `updatedAt` desc. Empty groups are omitted.
 */
export function partitionThreads(input: {
  sessions: readonly TreeSession[]
  sessionIssues: readonly TreeSessionIssue[]
  issues: readonly TreeIssue[]
  messages: readonly TreeMessage[]
  teamId: string | null
}): ThreadsListGroup[] {
  const roots = input.sessions.filter(
    (s) =>
      s.parentId === null &&
      s.kind === `thread` &&
      (input.teamId === null || s.teamId === input.teamId)
  )
  const byRoot = new Map<string, TreeSession[]>()
  for (const session of input.sessions) {
    const list = byRoot.get(session.rootId) ?? []
    list.push(session)
    byRoot.set(session.rootId, list)
  }
  const buckets: Record<ThreadsListGroup[`key`], ThreadsListRow[]> = {
    needs_you: [],
    running: [],
    recent: [],
  }
  for (const root of roots) {
    const tree = byRoot.get(root.id) ?? [root]
    const live = tree.filter((s) => s.status !== `ended`)
    const needsYou = live.some((s) => s.needsInput)
    const running = live.some((s) => s.status === `queued` || s.status === `running`)
    const activity = tree.reduce(
      (latest, s) => ((ms(s.updatedAt) ?? 0) > (ms(latest) ?? 0) ? s.updatedAt : latest),
      root.updatedAt
    )
    const title = threadTitle(
      root,
      rootIssues(root.id, input.sessionIssues, input.issues),
      input.messages
    )
    const row: ThreadsListRow = {
      sessionId: root.id,
      teamId: root.teamId,
      title,
      private: !root.shared,
      deviceId: root.deviceId ?? null,
      busy: live.some((s) => s.busy),
      needsYou,
      activityAt: activity,
    }
    buckets[needsYou ? `needs_you` : running ? `running` : `recent`].push(row)
  }
  const order = (rows: ThreadsListRow[]) =>
    rows.sort(
      (a, b) =>
        (ms(b.activityAt) ?? 0) - (ms(a.activityAt) ?? 0) ||
        (a.sessionId < b.sessionId ? -1 : 1)
    )
  return (Object.keys(buckets) as ThreadsListGroup[`key`][])
    .map((key) => ({ key, title: THREADS_LIST_COPY.groups[key], rows: order(buckets[key]) }))
    .filter((group) => group.rows.length > 0)
}

export interface AgentRow {
  sessionId: string
  teamId: string
  name: string
  icon: string
  color: string
  busy: boolean
  needsYou: boolean
}

/**
 * The rail's Agents section (above Boards): one row per agent root of the
 * team, icon + color like boards, by `sortOrder` then name. An agent's
 * spinner/needs-you dot covers its own row only (the threads it spawns are
 * their own roots).
 */
export function listAgents(
  sessions: readonly TreeSession[],
  teamId: string | null
): AgentRow[] {
  return sessions
    .filter(
      (s) =>
        s.parentId === null &&
        s.kind === `agent` &&
        (teamId === null || s.teamId === teamId)
    )
    .sort(
      (a, b) =>
        a.sortOrder - b.sortOrder ||
        (a.name ?? ``).localeCompare(b.name ?? ``) ||
        (a.id < b.id ? -1 : 1)
    )
    .map((s) => ({
      sessionId: s.id,
      teamId: s.teamId,
      name: s.name ?? ``,
      icon: s.icon ?? listFixture.agentDefaults.icon,
      color: s.color ?? listFixture.agentDefaults.color,
      busy: s.busy,
      needsYou: s.needsInput,
    }))
}

/** EXP-1309/1318: the ONE quiet trailing word of a thread row ×4 (the lock
 *  rides beside it when private). Keys; `THREAD_ROW_WORDS` = the copy. */
export type ThreadRowWord = `stack` | `readyToMerge` | `merged` | `question`

export const THREAD_ROW_WORDS = listFixture.copy.words

export interface StackPr {
  id: string
  state: string
  basePullRequestId: string | null
}

/** An open PR of the thread sits in an open stack: based on another open
 *  PR, or another open PR is based on it. `teamPrs` = the team's PRs (a
 *  stack edge may cross threads). */
export function threadInStack(
  pullRequests: readonly StackPr[],
  teamPrs: readonly StackPr[] = pullRequests
): boolean {
  const openIds = new Set(teamPrs.filter((pr) => pr.state === `open`).map((pr) => pr.id))
  return pullRequests
    .filter((pr) => pr.state === `open`)
    .some(
      (pr) =>
        (pr.basePullRequestId !== null && openIds.has(pr.basePullRequestId)) ||
        teamPrs.some((other) => other.state === `open` && other.basePullRequestId === pr.id)
    )
}

/** Word order: an open PR in a stack → `stack`; an open PR while nothing
 *  works → `readyToMerge` (working = no word); no open PR but a merged one
 *  → `merged`; else waiting on a person → `question`; else none. */
export function threadRowWord(input: {
  pullRequests: readonly { state: string }[]
  busy: boolean
  needsYou: boolean
  inStack: boolean
}): ThreadRowWord | null {
  if (input.pullRequests.some((pr) => pr.state === `open`)) {
    if (input.inStack) return `stack`
    return input.busy ? null : `readyToMerge`
  }
  if (input.pullRequests.some((pr) => pr.state === `merged`)) return `merged`
  if (input.needsYou) return `question`
  return null
}

/** The rail's Active band ×web+IDE (`copy.rail`): every Needs you row, every
 *  Running row, then the Recent rows whose word still asks for a merge
 *  (`readyToMerge` or `stack`), in group order. */
export function railThreads(
  groups: readonly ThreadsListGroup[],
  wordOf: (row: ThreadsListRow) => ThreadRowWord | null
): ThreadsListRow[] {
  return groups.flatMap((group) =>
    group.key === `recent`
      ? group.rows.filter((row) => {
          const word = wordOf(row)
          return word === `readyToMerge` || word === `stack`
        })
      : group.rows
  )
}

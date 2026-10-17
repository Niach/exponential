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
import { sessionTitle, sessionTitles } from "./session-title"
import { pictureFor, type CardSurface, type PlanCardPicture } from "./plan-card"

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
  /** EXP-1323: the live status line (absent = null). */
  caption?: string | null
  /** EXP-1323: a `/btw` aside child (absent = false). */
  aside?: boolean
}

/** EXP-1323: the viewer's read marks (`session_reads`, already user-scoped). */
export interface TreeSessionRead {
  sessionId: string
  readAt: string
}

export interface TreeSessionIssue {
  sessionId: string
  issueId: string
  sortOrder: number
  /** EXP-1341: the tree session working on it (absent/null = the root's). */
  assignedSessionId?: string | null
}

export interface TreeIssue {
  id: string
  identifier: string
  title: string
  /** EXP-1334: the issue's parent (`issue_relations` kind parent; absent =
   *  null): a linked sub-issue folds under its nearest LINKED ancestor. */
  parentId?: string | null
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
  /** EXP-1323: the session that posted it (absent = null). */
  authorSessionId?: string | null
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

/** Sessions of one tree in creation order (ties by id). */
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
  /** EXP-1323: the live status line under the name; an ended session's is
   *  dropped unless it is an aside (its answer's first line stays). */
  caption: string | null
  testId: string
}

/**
 * THE session card = the Work card's header row (EXP-1240): icon, name,
 * the muted state word, trailing elapsed, a spinner while `running`. Tapping
 * it opens that session's raw view. `title` = its `sessionTitle` (EXP-1341,
 * fixtures/session-title.json; never `Session N`).
 */
export function sessionCard(
  session: TreeSession,
  title: string,
  now: string
): SessionCard {
  const state = sessionState(session)
  const start = ms(session.startedAt)
  const end = state === `ended` ? ms(session.endedAt) ?? ms(session.updatedAt) : ms(now)
  const elapsed =
    start !== null && end !== null && state !== `queued` ? formatElapsed(end - start) : null
  const caption = session.caption?.trim()
  return {
    sessionId: session.id,
    icon: SESSION_CARD_ICONS[state],
    name: title,
    kindWord: SESSION_CARD_COPY.states[state],
    state,
    tone: cardFixture.tones[state],
    elapsed,
    running: state === `working`,
    caption: caption && (state !== `ended` || session.aside === true) ? caption : null,
    testId: cardFixture.testIds.card,
  }
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

/** EXP-1334: one row of a folded chip's subtree (the chip's dropdown =
 *  the chip itself, then these, THE tree rows with their guides). `depth`
 *  is relative to the chip (its children = 1). */
export interface ThreadTabNestedIssue {
  issueId: string
  identifier: string
  depth: number
}
export interface ThreadTabNestedSession {
  sessionId: string
  name: string
  markState: SessionTabMarkState
  depth: number
}

export type ThreadTab =
  | { kind: `add` }
  | { kind: `conversation` }
  | { kind: `issue`; issueId: string; identifier: string; nested: ThreadTabNestedIssue[] }
  | {
      kind: `session`
      sessionId: string
      name: string
      markState: SessionTabMarkState
      nested: ThreadTabNestedSession[]
    }
  | { kind: `guide`; dot: boolean }

/** EXP-1322: a question row's option, label + description like the session
 *  view's card (an undescribed card option → description null). */
export interface ThreadQuestionOption {
  label: string
  description: string | null
  /** EXP-1280: the option's picture (plan-card.json `pictureFor`); null = a text option. */
  attachmentId: string | null
}

/** EXP-1280: one choice question of a plan card in the row form. */
export interface ThreadPlanQuestion {
  id: string
  prompt: string
  multi: boolean
  custom: boolean
  options: ThreadQuestionOption[]
}

/** A synced card option (a plain label) in the row form; anything else =
 *  null (dropped). The sidecar description (see `questionOptions`) is zipped
 *  in by the caller. */
export function questionOption(raw: unknown): ThreadQuestionOption | null {
  return typeof raw === `string` ? { label: raw, description: null } : null
}

/** The question card's options in the row form. `card.options` carries plain
 *  label strings; the descriptions ride the parallel `card.optionDescriptions`
 *  sidecar (same length, null / "" = none; the key is omitted when no option
 *  has one): option i = `{label: options[i], description: optionDescriptions?.[i] || null}`.
 *  Zipped by index BEFORE dropping non-string options. */
export function questionOptions(card: {
  options?: unknown
  optionDescriptions?: unknown
  optionPictures?: unknown
  pictures?: unknown
}): ThreadQuestionOption[] {
  if (!Array.isArray(card.options)) return []
  const descriptions = Array.isArray(card.optionDescriptions) ? card.optionDescriptions : []
  const pictureLabels = Array.isArray(card.optionPictures) ? card.optionPictures : []
  const rows: ThreadQuestionOption[] = []
  card.options.forEach((raw, index) => {
    const option = questionOption(raw)
    if (!option) return
    const sidecar: unknown = descriptions[index]
    const picture: unknown = pictureLabels[index]
    rows.push({
      label: option.label,
      description: typeof sidecar === `string` && sidecar !== `` ? sidecar : null,
      attachmentId: pictureFor(cardPictures(card.pictures), typeof picture === `string` ? picture : null),
    })
  })
  return rows
}

/** EXP-1280: a card's `pictures` (the upload namespace) in a tolerant read. */
export function cardPictures(raw: unknown): PlanCardPicture[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry) => {
    if (!entry || typeof entry !== `object`) return []
    const picture = entry as Record<string, unknown>
    if (typeof picture.label !== `string`) return []
    return [
      {
        label: picture.label,
        attachmentId: typeof picture.attachmentId === `string` ? picture.attachmentId : null,
        width: typeof picture.width === `number` ? picture.width : null,
        height: typeof picture.height === `number` ? picture.height : null,
      },
    ]
  })
}

/** EXP-1280: a plan card's `questions` in the row form; a malformed
 *  question (no string id/prompt) or option drops. */
export function planQuestions(card: { questions?: unknown; pictures?: unknown }): ThreadPlanQuestion[] {
  if (!Array.isArray(card.questions)) return []
  const pictures = cardPictures(card.pictures)
  const rows: ThreadPlanQuestion[] = []
  for (const raw of card.questions) {
    if (!raw || typeof raw !== `object`) continue
    const question = raw as Record<string, unknown>
    if (typeof question.id !== `string` || typeof question.prompt !== `string`) continue
    const options: ThreadQuestionOption[] = []
    for (const entry of Array.isArray(question.options) ? question.options : []) {
      if (!entry || typeof entry !== `object`) continue
      const option = entry as Record<string, unknown>
      if (typeof option.label !== `string`) continue
      options.push({
        label: option.label,
        description: typeof option.description === `string` && option.description !== `` ? option.description : null,
        attachmentId: pictureFor(pictures, typeof option.picture === `string` ? option.picture : null),
      })
    }
    rows.push({
      id: question.id,
      prompt: question.prompt,
      multi: question.multi === true,
      custom: question.custom !== false,
      options,
    })
  }
  return rows
}

/** EXP-1280: a card's inline surface (tolerant read; an object with a `root` object, else null). */
export function cardSurface(raw: unknown): CardSurface | null {
  if (!raw || typeof raw !== `object`) return null
  const surface = raw as Record<string, unknown>
  if (!surface.root || typeof surface.root !== `object`) return null
  const data = surface.data && typeof surface.data === `object` && !Array.isArray(surface.data) ? (surface.data as Record<string, unknown>) : null
  return { root: surface.root, data }
}

/** EXP-1280: a plan card's `answers` (question id → labels), strings only. */
export function planAnswers(raw: unknown): Record<string, string[]> | null {
  if (!raw || typeof raw !== `object` || Array.isArray(raw)) return null
  const answers: Record<string, string[]> = {}
  for (const [id, labels] of Object.entries(raw as Record<string, unknown>)) {
    if (!Array.isArray(labels)) continue
    answers[id] = labels.filter((label): label is string => typeof label === `string`)
  }
  return answers
}

function stringList(raw: unknown): string[] | null {
  if (!Array.isArray(raw)) return null
  return raw.filter((entry): entry is string => typeof entry === `string`)
}

export type ThreadRow =
  | {
      type: `message`
      id: string
      author: `user` | `agent` | `system`
      authorId: string | null
      body: string
      queued: boolean
      /** EXP-1323: the session that posted it (a child asking its parent). */
      authorSessionId: string | null
    }
  | { type: `session`; id: string; card: SessionCard }
  | { type: `report`; id: string; sessionId: string; text: string; summary: string }
  | {
      type: `suggestions`
      id: string
      /** The session that posted the card (Do it here messages it). */
      sessionId: string
      header: string
      startAll: boolean
      items: SuggestionRow[]
    }
  | {
      type: `question`
      id: string
      /** EXP-1334: the session whose card it is (the root, or a child whose
       *  own question surfaces in the Conversation). */
      sessionId: string
      question: string
      options: ThreadQuestionOption[]
      custom: boolean
      /** EXP-1280: several picks allowed. */
      multi: boolean
      /** EXP-1280: the inline Exponential UI surface, if any. */
      surface: CardSurface | null
      answer: string | null
      /** EXP-1280: the picked labels (null on a legacy card = `answer` alone). */
      answers: string[] | null
      answered: boolean
    }
  | {
      type: `plan`
      id: string
      sessionId: string
      plan: string
      /** EXP-1280: the choice questions, the inline surface. */
      questions: ThreadPlanQuestion[]
      surface: CardSurface | null
      decision: string | null
      /** An approval that started in a fresh context (ask-dock.json). */
      fresh: boolean
      /** EXP-1280: the picked labels per question, the note. */
      answers: Record<string, string[]> | null
      feedback: string | null
    }
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

/** EXP-1323: one suggested task: `open` (× · Options · Do it here ·
 *  Start), `started` (the child's card once synced), `here` (sent to the
 *  posting session), `filed` (the issue it became). Dismissed items drop. */
export interface SuggestionRow {
  id: string
  title: string
  text: string
  state: `open` | `started` | `here` | `filed`
  card: SessionCard | null
  issueId: string | null
  identifier: string | null
}

export const SUGGESTIONS_COPY = threadFixture.copy.suggestions

function suggestionsRow(
  message: TreeMessage,
  card: Record<string, unknown>,
  byId: ReadonlyMap<string, TreeSession>,
  titles: ReadonlyMap<string, string>,
  issues: ReadonlyMap<string, TreeIssue>,
  now: string
): ThreadRow | null {
  if (!Array.isArray(card.items)) return null
  const items: SuggestionRow[] = []
  for (const raw of card.items as Record<string, unknown>[]) {
    if (!raw || typeof raw !== `object` || typeof raw.id !== `string`) continue
    if (raw.dismissed === true) continue
    const sessionId = typeof raw.sessionId === `string` ? raw.sessionId : null
    const issueId = typeof raw.issueId === `string` ? raw.issueId : null
    const target = sessionId ? byId.get(sessionId) : undefined
    const state: SuggestionRow[`state`] = sessionId
      ? `started`
      : raw.here === true
        ? `here`
        : issueId
          ? `filed`
          : `open`
    items.push({
      id: raw.id,
      title: String(raw.title ?? ``),
      text: String(raw.text ?? ``),
      state,
      card: target ? sessionCard(target, titles.get(target.id) ?? ``, now) : null,
      issueId,
      identifier: issueId ? issues.get(issueId)?.identifier ?? null : null,
    })
  }
  if (items.length === 0) return null
  const started = items.filter((item) => item.state === `started`).length
  const count =
    items.length === 1
      ? SUGGESTIONS_COPY.one
      : SUGGESTIONS_COPY.many.replace(`{count}`, String(items.length))
  return {
    type: `suggestions`,
    id: message.id,
    sessionId: message.sessionId,
    header:
      started > 0
        ? `${count} · ${SUGGESTIONS_COPY.started.replace(`{count}`, String(started))}`
        : count,
    startAll: items.filter((item) => item.state === `open`).length > 1,
    items,
  }
}

export interface SessionThreadView {
  rootId: string
  title: string
  private: boolean
  focusId: string | null
  tabs: ThreadTab[]
  rows: ThreadRow[]
  composer: { sessionId: string; placeholder: string }
  live: boolean
  /** EXP-1323: the live tail (THE Queue, task-queue.json, + the status line) reads this
   *  session's relay state; null once the root ended. */
  liveSessionId: string | null
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

export type GuideMenuEntry = `close-pr`

/** EXP-1307: the thread header's `…` on the Guide: Close PR while the Guide's
 *  PR is open and the viewer is a member; empty = no trigger. EXP-1338: no
 *  GitHub entry, only the agent opens PRs. */
export function guideMenuEntries(input: {
  state: string | null
  member: boolean
}): GuideMenuEntry[] {
  return input.state === `open` && input.member ? [`close-pr`] : []
}

/** EXP-1306 (EXP-820 ×4): a question or plan card waiting on the viewer
 *  takes the session composer's place. */
export function composerYieldsToCard(input: { live: boolean; pendingAsk: boolean }): boolean {
  return input.live && input.pendingAsk
}

/** Depth-first walk of a forest given each node's FOLD parent (null = a
 *  top-level node), children in the given order. */
function foldForest<T>(
  items: readonly T[],
  id: (item: T) => string,
  foldParent: (item: T) => string | null
): { top: T; nested: { item: T; depth: number }[] }[] {
  const children = new Map<string | null, T[]>()
  for (const item of items) {
    const parent = foldParent(item)
    const list = children.get(parent) ?? []
    list.push(item)
    children.set(parent, list)
  }
  const seen = new Set<string>()
  const fold = (top: T) => {
    const nested: { item: T; depth: number }[] = []
    seen.add(id(top))
    const walk = (parentId: string, depth: number) => {
      for (const child of children.get(parentId) ?? []) {
        if (seen.has(id(child))) continue
        seen.add(id(child))
        nested.push({ item: child, depth })
        walk(id(child), depth + 1)
      }
    }
    walk(id(top), 1)
    return { top, nested }
  }
  const out = (children.get(null) ?? []).map(fold)
  // A fold-parent cycle (a ↔ b) reaches no top: its first unreached item
  // (input order) becomes a top-level chip, the rest fold under it.
  for (const item of items) if (!seen.has(id(item))) out.push(fold(item))
  return out
}

/** EXP-1334: a linked issue's fold parent = its nearest LINKED ancestor
 *  (an unlinked parent is skipped; a cycle or none = top-level). */
function issueFoldParent(
  issue: TreeIssue,
  linked: ReadonlySet<string>,
  all: ReadonlyMap<string, TreeIssue>
): string | null {
  const seen = new Set([issue.id])
  let parent = issue.parentId ?? null
  while (parent && !seen.has(parent)) {
    if (linked.has(parent)) return parent
    seen.add(parent)
    parent = all.get(parent)?.parentId ?? null
  }
  return null
}

/**
 * The thread screen (EXP-1334): the header strip (`+` · Conversation · one
 * chip per TOP-LEVEL linked issue, its linked sub-issues folded under it ·
 * one chip per DIRECT child of the dispatcher, every deeper session folded
 * under its top-level ancestor · Guide), the dispatcher's conversation as
 * rows (no card of its own: its messages oldest first, a
 * `session`/`thread_started` card becoming that session's card, a child's
 * open question/plan card surfacing here) and the composer target (the root:
 * a message to an ended thread continues it server-side). `focus` = the
 * session open in place (`focusId`); the chips stay the dispatcher's.
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
  const byId = new Map(input.sessions.map((s) => [s.id, s]))
  byId.set(root.id, root)
  const titles = sessionTitles([...byId.values()], input.sessionIssues, input.issues, input.messages)
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
  const title = titles.get(root.id)!
  const focus = options.focus ? tree.find((s) => s.id === options.focus) ?? null : null
  // EXP-1334: the conversation IS the dispatcher (no chip of its own); the
  // chips = its DIRECT children, every deeper session folded under its
  // top-level ancestor (the chip's dropdown).
  const nameOf = (s: TreeSession) => sessionCard(s, titles.get(s.id) ?? ``, now).name
  const treeIds = new Set(tree.map((s) => s.id))
  const sessionTabs: ThreadTab[] = foldForest(
    tree.filter((s) => s.id !== root.id),
    (s) => s.id,
    (s) => (s.parentId === root.id || !s.parentId || !treeIds.has(s.parentId) ? null : s.parentId)
  ).map(({ top, nested }) => ({
    kind: `session` as const,
    sessionId: top.id,
    name: nameOf(top),
    markState: sessionTabMarkState(top),
    nested: nested.map(({ item, depth }) => ({
      sessionId: item.id,
      name: nameOf(item),
      markState: sessionTabMarkState(item),
      depth,
    })),
  }))
  const linkedIds = new Set(issues.map((issue) => issue.id))
  const issueTabs: ThreadTab[] = foldForest(
    issues,
    (issue) => issue.id,
    (issue) => issueFoldParent(issue, linkedIds, issueMap)
  ).map(({ top, nested }) => ({
    kind: `issue` as const,
    issueId: top.id,
    identifier: top.identifier,
    nested: nested.map(({ item, depth }) => ({
      issueId: item.id,
      identifier: item.identifier,
      depth,
    })),
  }))

  // EXP-1334: no root card; a CHILD's own question / plan card surfaces in
  // the Conversation (answered here, straight back to that child).
  const asks = input.messages.filter(
    (m) =>
      m.sessionId !== root.id &&
      treeIds.has(m.sessionId) &&
      (m.card?.kind === `question` || m.card?.kind === `plan`)
  )
  const conversation = [...own, ...asks].sort(
    (a, b) =>
      (ms(a.createdAt) ?? 0) - (ms(b.createdAt) ?? 0) ||
      (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  )
  const rows: ThreadRow[] = []
  for (const message of conversation) {
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
        authorSessionId: message.authorSessionId ?? null,
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
          card: sessionCard(target, titles.get(target.id) ?? ``, now),
        })
        break
      }
      case `question`:
        rows.push({
          type: `question`,
          id: message.id,
          sessionId: message.sessionId,
          question: String(card.question ?? ``),
          options: questionOptions(card),
          custom: card.custom !== false,
          multi: card.multi === true,
          surface: cardSurface(card.surface),
          answer: typeof card.answer === `string` ? card.answer : null,
          answers: stringList(card.answers),
          answered: message.answeredAt !== null,
        })
        break
      case `plan`:
        rows.push({
          type: `plan`,
          id: message.id,
          sessionId: message.sessionId,
          plan: String(card.plan ?? ``),
          questions: planQuestions(card),
          surface: cardSurface(card.surface),
          decision: typeof card.decision === `string` ? card.decision : null,
          fresh: card.fresh === true,
          answers: planAnswers(card.answers),
          feedback: typeof card.feedback === `string` && card.feedback !== `` ? card.feedback : null,
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
      case `report`: {
        const target = typeof card.sessionId === `string` ? byId.get(card.sessionId) : undefined
        if (!target) break
        rows.push({
          type: `report`,
          id: message.id,
          sessionId: target.id,
          text: THREAD_COPY.report.replace(
            `{session}`,
            sessionCard(target, titles.get(target.id) ?? ``, now).name
          ),
          summary: typeof card.summary === `string` ? card.summary : message.body,
        })
        break
      }
      case `suggestions`: {
        const row = suggestionsRow(message, card, byId, titles, issueMap, now)
        if (row) rows.push(row)
        break
      }
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
      { kind: `add` },
      { kind: `conversation` },
      ...issueTabs,
      ...sessionTabs,
      { kind: `guide`, dot: guideDot },
    ],
    rows,
    composer: { sessionId: root.id, placeholder: THREAD_COPY.placeholder },
    live: live.length > 0,
    liveSessionId: root.status === `ended` ? null : root.id,
    busy: live.some((s) => s.busy),
    needsYou: live.some((s) => s.needsInput),
  }
}

export interface ThreadsListRow {
  sessionId: string
  teamId: string
  /** EXP-1323: 0 = the thread (opens the thread), >0 = a nested child
   *  (opens that session); `parentId` null on a root. */
  depth: number
  parentId: string | null
  rootId: string
  title: string
  /** EXP-1318: the ROOT's device (the row's trailing device icon);
   *  `partitionThreads` always sets it, hand-built rows may omit it (= null). */
  deviceId?: string | null
  busy: boolean
  needsYou: boolean
  /** EXP-1323: bold title (a root: the tree's newest agent message is
   *  newer than the viewer's read of the root; a child: its own). */
  unread: boolean
  /** EXP-1323: the live caption (the session card rule). */
  caption: string | null
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
  /** EXP-1323: the viewer's `session_reads` (absent = none read). */
  reads?: readonly TreeSessionRead[]
}): ThreadsListGroup[] {
  const readAt = new Map((input.reads ?? []).map((read) => [read.sessionId, ms(read.readAt) ?? 0]))
  const rootOf = new Map(input.sessions.map((s) => [s.id, s.rootId]))
  const newestAgent = new Map<string, number>()
  const newestAgentInTree = new Map<string, number>()
  for (const message of input.messages) {
    if (message.authorKind !== `agent`) continue
    const at = ms(message.createdAt) ?? 0
    newestAgent.set(message.sessionId, Math.max(newestAgent.get(message.sessionId) ?? 0, at))
    const rootId = rootOf.get(message.sessionId)
    if (rootId) newestAgentInTree.set(rootId, Math.max(newestAgentInTree.get(rootId) ?? 0, at))
  }
  const unreadAt = (newest: number | undefined, sessionId: string) =>
    newest !== undefined && newest > (readAt.get(sessionId) ?? -1)
  const now = ``
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
  const buckets: Record<
    ThreadsListGroup[`key`],
    { row: ThreadsListRow; children: ThreadsListRow[] }[]
  > = {
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
    const title = sessionTitle(root, input.sessionIssues, input.issues, input.messages)
    const row: ThreadsListRow = {
      sessionId: root.id,
      teamId: root.teamId,
      depth: 0,
      parentId: null,
      rootId: root.id,
      title,
      deviceId: root.deviceId ?? null,
      busy: live.some((s) => s.busy),
      needsYou,
      unread: unreadAt(newestAgentInTree.get(root.id), root.id),
      caption: sessionCard(root, title, now).caption,
      activityAt: activity,
    }
    const key = needsYou ? `needs_you` : running ? `running` : `recent`
    // A live tree nests its children (depth-first, creation order; asides
    // stay in the Conversation); Recent collapses to the root.
    const children: ThreadsListRow[] = []
    if (key !== `recent`) {
      const ordered = treeOrder(tree)
      const walk = (parentId: string, depth: number) => {
        for (const child of ordered) {
          if (child.parentId !== parentId || child.aside === true) continue
          const card = sessionCard(
            child,
            sessionTitle(child, input.sessionIssues, input.issues, input.messages),
            now
          )
          children.push({
            sessionId: child.id,
            teamId: child.teamId,
            depth,
            parentId,
            rootId: root.id,
            title: card.name,
            deviceId: child.deviceId ?? null,
            busy: child.status !== `ended` && child.busy,
            needsYou: child.status !== `ended` && child.needsInput,
            unread: unreadAt(newestAgent.get(child.id), child.id),
            caption: card.caption,
            activityAt: child.updatedAt,
          })
          walk(child.id, depth + 1)
        }
      }
      walk(root.id, 1)
    }
    buckets[key].push({ row, children })
  }
  const order = (entries: { row: ThreadsListRow; children: ThreadsListRow[] }[]) =>
    entries
      .sort(
        (a, b) =>
          (ms(b.row.activityAt) ?? 0) - (ms(a.row.activityAt) ?? 0) ||
          (a.row.sessionId < b.row.sessionId ? -1 : 1)
      )
      .flatMap((entry) => [entry.row, ...entry.children])
  return (Object.keys(buckets) as ThreadsListGroup[`key`][])
    .map((key) => ({ key, title: THREADS_LIST_COPY.groups[key], rows: order(buckets[key]) }))
    .filter((group) => group.rows.length > 0)
}

/** EXP-1323: the Threads nav dot = any thread row is unread. */
export function threadsUnread(groups: readonly ThreadsListGroup[]): boolean {
  return groups.some((group) => group.rows.some((row) => row.depth === 0 && row.unread))
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

/** EXP-1309/1318: the ONE quiet trailing word of a thread row ×4. Keys;
 *  `THREAD_ROW_WORDS` = the copy (EXP-1339: `merged` has none, it only
 *  turns the lead violet). */
export type ThreadRowWord = `stack` | `readyToMerge` | `merged` | `question`

export const THREAD_ROW_WORDS: Partial<Record<ThreadRowWord | `needsInput`, string>> =
  listFixture.copy.words

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

// ── EXP-1334: the thread strip never scrolls ─────────────────────────────────

/**
 * Which chips get drawn, in order; the rest collapse into one "+N". A port
 * of the desktop's `partition_tabs` (EXP-288/326/343), same vectors: chips
 * pack in order until the next would not fit; the overflow chip's width is
 * reserved only once something overflows; the ACTIVE chip's width is
 * committed first so it is always visible; one chip always survives.
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
  const total = widths.reduce((sum, width) => sum + width, 0) + gap * Math.max(count - 1, 0)
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
  if (activeIndex !== null && !visible.includes(activeIndex)) visible.push(activeIndex)
  if (visible.length === 0 && count > 0) visible.push(0)
  return visible
}

export interface ThreadStripLayout {
  /** `full` = identifier + title; `short` = the identifier alone; `fold` =
   *  short and some chips behind a "+N". */
  tier: `full` | `short` | `fold`
  /** Indexes of the issue chips drawn (strip order). */
  issues: number[]
  /** Indexes of the session chips drawn. */
  sessions: number[]
}

/**
 * EXP-1334: the strip between Conversation and Guide (both pinned, never
 * scrolled) holds two zones, issues then sessions, one `gap` between them.
 * Every issue chip has a `full` and a `short` width (the caller measures).
 * Tiers: everything full → every issue short → fold: the sessions keep
 * their OWN space (what the short issues leave, at least half the room),
 * each zone packs with `partitionTabs` and its own "+N" (`overflowW`).
 */
export function partitionThreadStrip(input: {
  issues: readonly { full: number; short: number }[]
  sessions: readonly number[]
  available: number
  gap: number
  overflowW: number
  activeIssue: number | null
  activeSession: number | null
}): ThreadStripLayout {
  const { issues, sessions, gap, overflowW } = input
  const room = Math.max(input.available, 0)
  const span = (widths: readonly number[]) =>
    widths.length === 0 ? 0 : widths.reduce((sum, w) => sum + w, 0) + gap * (widths.length - 1)
  const sep = issues.length > 0 && sessions.length > 0 ? gap : 0
  const all = (n: number) => Array.from({ length: n }, (_, index) => index)
  const sessionSpan = span(sessions)
  const full = issues.map((issue) => issue.full)
  const short = issues.map((issue) => issue.short)
  if (span(full) + sep + sessionSpan <= room) {
    return { tier: `full`, issues: all(issues.length), sessions: all(sessions.length) }
  }
  if (span(short) + sep + sessionSpan <= room) {
    return { tier: `short`, issues: all(issues.length), sessions: all(sessions.length) }
  }
  const zone = (widths: readonly number[], budget: number, active: number | null) => {
    const shown = partitionTabs(widths, budget, gap, overflowW, active)
    const used =
      span(shown.map((index) => widths[index])) +
      (shown.length < widths.length ? gap + overflowW : 0)
    return { shown, used }
  }
  const sessionBudget =
    issues.length === 0
      ? room
      : Math.min(sessionSpan, Math.max(room / 2, room - span(short) - sep))
  const placed = zone(sessions, sessionBudget, input.activeSession)
  const issueBudget = room - (sessions.length > 0 ? placed.used + sep : 0)
  const issueZone = zone(short, issueBudget, input.activeIssue)
  return { tier: `fold`, issues: issueZone.shown, sessions: placed.shown }
}

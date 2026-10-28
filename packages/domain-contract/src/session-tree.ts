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
import { commandDivider } from "./chats"

export interface TreeSession {
  id: string
  teamId: string
  parentId: string | null
  rootId: string
  userId: string
  /** The logical `devices.device_id` that runs it (null/absent = unset). */
  deviceId?: string | null
  status: string
  busy: boolean
  /** EXP-1374: busy only because background tasks still run after the
   *  turn (absent = false). */
  background?: boolean | null
  needsInput: boolean
  blocked: {
    kind?: string | null
    window?: string | null
    resetsAt?: string | null
    resume?: boolean | null
  } | null
  shared: boolean
  title: string | null
  guide: { text?: string | null; label?: string | null }[] | null
  /** NEW-2: the agent this root is a conversation of (absent = null). */
  agentId?: string | null
  /** VAPP-110: the app this root is a member's editor session of (absent = null). */
  appId?: string | null
  startedAt: string | null
  endedAt: string | null
  createdAt: string
  updatedAt: string
}

/** EXP-1323: the viewer's read marks (`session_reads`, already user-scoped). */
export interface TreeSessionRead {
  sessionId: string
  readAt: string
  /** #EXP-1388: the viewer archived the chat (absent / null = not). */
  archivedAt?: string | null
}

export interface TreeSessionIssue {
  /** NEW-1: the session that HOLDS the issue (the row sits on it). */
  sessionId: string
  issueId: string
  sortOrder: number
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
  /** NEW-8: the session its conversation belongs to, keyed through the
   *  `conversations` shape (`keyMessagesBySession`), never the row's own
   *  legacy `session_id`. */
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
  /** NEW-1: `treeState` reads it (`merged` = landed); absent = unknown. */
  state?: string
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

/** S-3 (owner 2026-10-28): the FOUR display states ×4, nothing else (no
 *  running, paused, interrupted or stale in the UI). `working` = queued or
 *  busy (background tasks included); `needs_you` = needsInput (a card, a
 *  usage wall, an unclaimed/stuck flag); `done` = ended; `idle` = the rest
 *  (a started session between turns, an interruption: device_offline and
 *  restarted are invisible). */
export type SessionState = `working` | `needs_you` | `idle` | `done`

/** One state per session, the card's kind word and tone. */
export function sessionState(
  session: Pick<TreeSession, `status` | `needsInput` | `busy`>
): SessionState {
  if (session.status === `ended`) return `done`
  if (session.needsInput) return `needs_you`
  if (session.status === `queued` || session.busy) return `working`
  return `idle`
}

/** S-3: a NESTED session (a child row, a strip chip) shows while it works or
 *  needs you, or while its last activity (ended: endedAt, else updatedAt) is
 *  within `threads-list.json` `recentChildMinutes` of `now` (null = never). */
export function visibleChild(
  session: Pick<TreeSession, `status` | `needsInput` | `busy` | `endedAt` | `updatedAt`>,
  now: string | null | undefined
): boolean {
  const state = sessionState(session)
  if (state === `working` || state === `needs_you`) return true
  const at = ms(now)
  const last = state === `done` ? ms(session.endedAt) ?? ms(session.updatedAt) : ms(session.updatedAt)
  return at !== null && last !== null && at - last <= listFixture.recentChildMinutes * 60 * 1000
}

/** S-3: the ids of a tree's sessions that show nested: `visibleChild` or an
 *  ancestor of one (a working grandchild keeps its idle parent). */
export function visibleTree(tree: readonly TreeSession[], now: string | null | undefined): Set<string> {
  const byId = new Map(tree.map((s) => [s.id, s]))
  const out = new Set<string>()
  for (const session of tree) {
    if (!visibleChild(session, now)) continue
    let at: TreeSession | undefined = session
    const seen = new Set<string>()
    while (at && !seen.has(at.id)) {
      seen.add(at.id)
      out.add(at.id)
      at = at.parentId ? byId.get(at.parentId) : undefined
    }
  }
  return out
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
  testId: string
}

/** NEW-1: the card's state word; a session that needs you because of a
 *  flag names WHY (`copy.blockedKinds[blocked.kind]`). */
export function sessionKindWord(session: Pick<TreeSession, `blocked`>, state: SessionState): string {
  if (state === `needs_you`) {
    const kind = session.blocked?.kind
    const word = kind ? (SESSION_CARD_COPY.blockedKinds as Record<string, string>)[kind] : undefined
    if (word) return word
  }
  return SESSION_CARD_COPY.states[state]
}

/**
 * THE session card = the Work card's header row (EXP-1240): icon, name,
 * the muted state word, trailing elapsed (working and done only), a spinner
 * while `running` (= working). Tapping it opens that session's raw view.
 * `title` = its `sessionTitle` (EXP-1341, fixtures/session-title.json).
 */
export function sessionCard(
  session: TreeSession,
  title: string,
  now: string
): SessionCard {
  const state = sessionState(session)
  const start = ms(session.startedAt)
  const end =
    state === `done`
      ? ms(session.endedAt) ?? ms(session.updatedAt)
      : state === `working` && session.status !== `queued`
        ? ms(now)
        : null
  const elapsed = start !== null && end !== null ? formatElapsed(end - start) : null
  return {
    sessionId: session.id,
    icon: SESSION_CARD_ICONS[state],
    name: title,
    kindWord: sessionKindWord(session, state),
    state,
    tone: cardFixture.tones[state],
    elapsed,
    running: state === `working`,
    testId: cardFixture.testIds.card,
  }
}

/** EXP-1374: the Dock card of a background session, and (#EXP-1405) of a
 *  queued one waiting for its device (a dock.json `banner` source without
 *  its `kind`), null otherwise. S-3: an interruption has no card. */
export interface SessionBanner {
  id: `background` | `queued`
  icon: string
  label: string
  text: string
  tone: `muted`
  action: null
}

export function sessionBanner(
  session: Pick<TreeSession, `status` | `needsInput` | `busy` | `background`>
): SessionBanner | null {
  if (session.status === `ended` || session.needsInput) return null
  const id = session.status === `queued` ? `queued` : session.busy && session.background ? `background` : null
  if (!id) return null
  return {
    id,
    icon: cardFixture.bannerIcons[id],
    label: cardFixture.bannerLabels[id],
    text: cardFixture.banner[id],
    tone: `muted`,
    action: null,
  }
}

/** NEW-1: a thread's issues = the UNION over its tree's rows: the sessions
 *  in tree order (the root first, then creation order), each one's rows by
 *  sortOrder, every issue once. */
export function treeIssues(
  tree: readonly Pick<TreeSession, `id`>[],
  sessionIssues: readonly TreeSessionIssue[],
  issues: readonly TreeIssue[]
): TreeIssue[] {
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  const seen = new Set<string>()
  const out: TreeIssue[] = []
  for (const session of tree) {
    const rows = sessionIssues
      .filter((link) => link.sessionId === session.id)
      .sort(
        (a, b) =>
          a.sortOrder - b.sortOrder || (a.issueId < b.issueId ? -1 : a.issueId > b.issueId ? 1 : 0)
      )
    for (const link of rows) {
      const issue = byId.get(link.issueId)
      if (!issue || seen.has(issue.id)) continue
      seen.add(issue.id)
      out.push(issue)
    }
  }
  return out
}

/** A session chip's lead ×4: THAT session's display state (S-3). */
export type SessionTabMarkState = SessionState

export function sessionTabMarkState(
  session: Pick<TreeSession, `status` | `busy` | `needsInput`>
): SessionTabMarkState {
  return sessionState(session)
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
  return typeof raw === `string` ? { label: raw, description: null, attachmentId: null } : null
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

/** EXP-1280: a JSON object (the surface's data model), else null. */
function dataRecord(raw: unknown): Record<string, unknown> | null {
  return raw && typeof raw === `object` && !Array.isArray(raw) ? (raw as Record<string, unknown>) : null
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
      /** EXP-1280: the surface's data model as the person answered it (null = unanswered / none). */
      surfaceData: Record<string, unknown> | null
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
      surfaceData: Record<string, unknown> | null
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
  /** Thread 58cb32df (chats.json): a person's `/new` or `/compact` line. */
  | { type: `divider`; id: string; kind: `new` | `compact`; at: string }
  /** Thread 58cb32df (chats.json): an agent's inline Exponential UI preview. */
  | { type: `surface`; id: string; sessionId: string; title: string | null; surface: CardSurface }

/** A `question` / `plan` card's Conversation row (its own thread, or an
 *  escalated card read from its origin message). */
function askRow(
  message: TreeMessage,
  card: NonNullable<TreeMessage[`card`]>
): Extract<ThreadRow, { type: `question` | `plan` }> {
  if (card.kind === `question`) {
    return {
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
      surfaceData: dataRecord(card.surfaceData),
      answered: message.answeredAt !== null,
    }
  }
  return {
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
    surfaceData: dataRecord(card.surfaceData),
  }
}

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
  /** NEW-6 (N-1): a thread with ONE session (the root alone, no child yet)
   *  shows that session's raw work as the main thing: its id, and the
   *  strip carries no Conversation tab. Null once a second session exists
   *  (the Conversation tab appears, the chips name the children). */
  mainSessionId: string | null
}

function eventText(
  card: Record<string, unknown>,
  issues: ReadonlyMap<string, TreeIssue>,
  prs: ReadonlyMap<string, TreePullRequest>,
  sessionName: (id: string) => string | null = () => null
): string | null {
  const event = String(card.event ?? ``)
  const template = (THREAD_COPY.events as Record<string, string>)[event]
  if (!template) return null
  const issue = typeof card.issueId === `string` ? issues.get(card.issueId) : undefined
  const pr = typeof card.pullRequestId === `string` ? prs.get(card.pullRequestId) : undefined
  let text = template
  if (text.includes(`{issue}`)) {
    if (!issue) return null
    text = text.replace(`{issue}`, () => issue.identifier)
  }
  if (text.includes(`{pr}`)) {
    if (!pr) return null
    text = text.replace(`{pr}`, () => `#${pr.number}`)
  }
  if (text.includes(`{session}`)) {
    // NEW-1: a bus line about a session of the tree; an unsynced one drops.
    const name = typeof card.sessionId === `string` ? sessionName(card.sessionId) : null
    if (!name) return null
    text = text.replace(`{session}`, () => name)
  }
  if (text.includes(`{name}`)) {
    // NEW-2: a trigger line names its event (an event trigger) and nothing
    // else; a schedule firing has no name and draws no line (its body
    // speaks).
    const name = typeof card.name === `string` && card.name.trim() ? card.name : null
    if (!name) return null
    text = text.replace(`{name}`, () => name)
  }
  return text
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

export const REPLIES_COPY = threadFixture.copy.replies

export interface ComposerReplies {
  messageId: string
  items: { id: string; text: string }[]
}

/** EXP-1211: the quick replies above a session's composer = its NEWEST
 *  `replies` card (createdAt, then id) minus the sent items; null = none. */
export function composerReplies(input: {
  sessionId: string
  messages: readonly TreeMessage[]
}): ComposerReplies | null {
  let newest: TreeMessage | null = null
  for (const message of input.messages) {
    if (message.sessionId !== input.sessionId || message.card?.kind !== `replies`) continue
    if (
      !newest ||
      (ms(message.createdAt) ?? 0) > (ms(newest.createdAt) ?? 0) ||
      ((ms(message.createdAt) ?? 0) === (ms(newest.createdAt) ?? 0) && message.id > newest.id)
    ) {
      newest = message
    }
  }
  if (!newest || !Array.isArray(newest.card?.items)) return null
  const items: ComposerReplies[`items`] = []
  for (const raw of newest.card.items as Record<string, unknown>[]) {
    if (!raw || typeof raw !== `object` || typeof raw.id !== `string`) continue
    if (raw.sent === true || typeof raw.text !== `string` || !raw.text.trim()) continue
    items.push({ id: raw.id, text: raw.text })
  }
  return items.length > 0 ? { messageId: newest.id, items } : null
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
}, options: {
  focus?: string | null
  /** #EXP-1380 (web): the sessions' titles when `messages` is only the
   *  newest of the tree (a first prompt may be older); absent = from them. */
  titles?: ReadonlyMap<string, string>
} = {}): SessionThreadView {
  const { root, now } = input
  const tree = treeOrder(input.sessions.filter((s) => s.rootId === root.id || s.id === root.id))
  if (!tree.some((s) => s.id === root.id)) tree.unshift(root)
  const byId = new Map(input.sessions.map((s) => [s.id, s]))
  byId.set(root.id, root)
  const titles =
    options.titles ?? sessionTitles([...byId.values()], input.sessionIssues, input.issues, input.messages)
  const issues = treeIssues(tree, input.sessionIssues, input.issues)
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
  // S-3: only the VISIBLE sessions get chips (`visibleTree`), the focused
  // one always; the rest stay reachable through their cards.
  const shown = visibleTree(tree.filter((s) => s.id !== root.id), now)
  const sessionTabs: ThreadTab[] = foldForest(
    tree.filter((s) => s.id !== root.id && (shown.has(s.id) || s.id === focus?.id)),
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
  const messagesById = new Map(input.messages.map((m) => [m.id, m]))
  const escalated = new Set<string>()
  for (const message of conversation) {
    const card = message.card
    if (!card) {
      const author =
        message.authorKind === `user` || message.authorKind === `agent`
          ? message.authorKind
          : `system`
      if (!message.body.trim()) continue
      const divider = author === `user` ? commandDivider(message.body) : null
      if (divider) {
        rows.push({ type: `divider`, id: message.id, kind: divider, at: message.createdAt })
        continue
      }
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
      case `plan`:
        rows.push(askRow(message, card))
        break
      case `surface`: {
        const surface = cardSurface(card.surface)
        if (!surface) break
        rows.push({
          type: `surface`,
          id: message.id,
          sessionId: message.sessionId,
          title: typeof card.title === `string` && card.title.trim() ? card.title.trim() : null,
          surface,
        })
        break
      }
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
        const text = eventText(card, issueMap, prMap, (id) => {
          const target = byId.get(id)
          return target ? sessionCard(target, titles.get(target.id) ?? ``, now).name : null
        })
        if (text) rows.push({ type: `event`, id: message.id, event: String(card.event), text })
        break
      }
      // NEW-1: a child's ask mirrored into its parent (`ask`): the child's
      // own card already surfaces in the Conversation (`asks`), so the
      // mirror draws nothing here. #EXP-1399: a mirror from ANOTHER tree
      // (an agent's conversation watching a thread, `escalate`) = the
      // origin's session card + THE origin card, read from its message.
      case `ask`: {
        const target = typeof card.sessionId === `string` ? byId.get(card.sessionId) : undefined
        const origin = typeof card.messageId === `string` ? messagesById.get(card.messageId) : undefined
        if (!target || treeIds.has(target.id) || !origin || origin.sessionId !== target.id) break
        // #EXP-1399 review: only the origin session's own mirror draws it.
        if (message.authorSessionId !== target.id) break
        if (origin.card?.kind !== `question` && origin.card?.kind !== `plan`) break
        if (escalated.has(origin.id)) break
        escalated.add(origin.id)
        rows.push({ type: `session`, id: message.id, card: sessionCard(target, titles.get(target.id) ?? ``, now) })
        rows.push(askRow(origin, origin.card))
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
  // NEW-6 (N-1): the root alone = the thread IS that session: the
  // Conversation tab opens its raw work in place (`mainSessionId`); a
  // second session turns the tab into the dispatcher's conversation and
  // brings the chips. NEW-46: the tab itself is ALWAYS in the strip.
  const single = tree.every((s) => s.id === root.id)
  return {
    rootId: root.id,
    title,
    private: !root.shared,
    focusId: focus?.id ?? null,
    tabs: [
      { kind: `add` },
      { kind: `conversation` as const },
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
    mainSessionId: single ? root.id : null,
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
  activityAt: string
  /** S-3: set ONLY on a tree's fold row (`<rootId>:earlier`, depth 1, the
   *  muted list-item row; opens the thread) = how many sub-sessions it hides. */
  earlier?: number
}

/** S-3: the fold row's id suffix and its label (`copy.earlier`). */
export const EARLIER_SUFFIX = `:earlier`
export function earlierLabel(count: number): string {
  return (count === 1 ? THREADS_LIST_COPY.earlier.one : THREADS_LIST_COPY.earlier.other).replace(
    `{n}`,
    String(count)
  )
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

/** NEW-41 ×4: the PR glyph a thread row leads with once threadRowMarkState
 *  is `waiting`, off the ROOT's PR states (never the word, so a stacked open
 *  PR still leads emerald): any open → `open`, else any merged → `merged`,
 *  else none (`threads-list.json` `leadCases`). */
export function threadRowLeadPr(prStates: readonly string[]): `open` | `merged` | null {
  if (prStates.includes(`open`)) return `open`
  if (prStates.includes(`merged`)) return `merged`
  return null
}

export interface ThreadsListGroup {
  key: `needs_you` | `running` | `recent`
  title: string
  rows: ThreadsListRow[]
}

/** S-3: a tree's display state = the session's four (`SessionState`). */
export type TreeState = SessionState

/**
 * NEW-1 / S-3: a tree's derived state. `needs_you` = a live session waits on
 * a person (`needsInput`, a usage wall or a watchdog flag included);
 * `working` = any live session queued or busy; `done` = every session ended,
 * or every PR of the thread merged (none = never done this way); else
 * `idle`. `now` is kept for the signature (no stale state any more).
 */
export function treeState(
  tree: readonly TreeSession[],
  pullRequests: readonly Pick<TreePullRequest, `id` | `state`>[],
  _now: string
): TreeState {
  const live = tree.filter((s) => s.status !== `ended`)
  if (live.some((s) => s.needsInput)) return `needs_you`
  if (live.some((s) => s.status === `queued` || s.busy)) return `working`
  if (live.length === 0) return `done`
  if (pullRequests.length > 0 && pullRequests.every((pr) => pr.state === `merged`)) return `done`
  return `idle`
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
  /** NEW-1: the clock for Recent's window (absent = no window: every
   *  recent tree, up to `recent.max`). */
  now?: string | null
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
      // NEW-2: an agent's conversation (a root with `agentId`) is no thread;
      // VAPP-110: neither is an app's editor session (`appId`).
      !s.agentId &&
      !s.appId &&
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
    const running = live.some((s) => s.status === `queued` || s.busy)
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
      activityAt: activity,
    }
    const key = needsYou ? `needs_you` : running ? `running` : `recent`
    // A live tree nests its VISIBLE children (S-3 `visibleTree`: working,
    // needs you or active within `recentChildMinutes`; depth-first, creation
    // order), the rest fold into ONE `earlier` row; Recent collapses to the
    // root.
    const children: ThreadsListRow[] = []
    if (key !== `recent`) {
      const ordered = treeOrder(tree)
      const shown = visibleTree(ordered.filter((s) => s.id !== root.id), input.now)
      const walk = (parentId: string, depth: number) => {
        for (const child of ordered) {
          if (child.parentId !== parentId || !shown.has(child.id)) continue
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
            activityAt: child.updatedAt,
          })
          walk(child.id, depth + 1)
        }
      }
      walk(root.id, 1)
      const hidden = ordered.filter((s) => s.id !== root.id && !shown.has(s.id))
      if (hidden.length > 0) {
        children.push({
          sessionId: `${root.id}${EARLIER_SUFFIX}`,
          teamId: root.teamId,
          depth: 1,
          parentId: root.id,
          rootId: root.id,
          title: earlierLabel(hidden.length),
          deviceId: null,
          busy: false,
          needsYou: false,
          unread: false,
          activityAt: hidden.reduce(
            (latest, s) => ((ms(s.updatedAt) ?? 0) > (ms(latest) ?? 0) ? s.updatedAt : latest),
            hidden[0].updatedAt
          ),
          earlier: hidden.length,
        })
      }
    }
    buckets[key].push({ row, children })
  }
  const sorted = (entries: { row: ThreadsListRow; children: ThreadsListRow[] }[]) =>
    entries.sort(
      (a, b) =>
        (ms(b.row.activityAt) ?? 0) - (ms(a.row.activityAt) ?? 0) ||
        (a.row.sessionId < b.row.sessionId ? -1 : 1)
    )
  const flat = (entries: { row: ThreadsListRow; children: ThreadsListRow[] }[]) =>
    entries.flatMap((entry) => [entry.row, ...entry.children])
  // NEW-1: Recent = every tree active in the last `recent.hours`, filled
  // up to the last `recent.max` by activity. Ended trees stay openable.
  const recentRules = listFixture.recent
  const at = ms(input.now ?? null)
  const recent = sorted(buckets.recent).filter(
    (entry, index) =>
      index < recentRules.max ||
      (at !== null && at - (ms(entry.row.activityAt) ?? 0) <= recentRules.hours * 60 * 60 * 1000)
  )
  return (Object.keys(buckets) as ThreadsListGroup[`key`][])
    .map((key) => ({
      key,
      title: THREADS_LIST_COPY.groups[key],
      rows: flat(key === `recent` ? recent : sorted(buckets[key])),
    }))
    .filter((group) => group.rows.length > 0)
}

/** EXP-1323: the Threads nav dot = any thread row is unread. */
export function threadsUnread(groups: readonly ThreadsListGroup[]): boolean {
  return groups.some((group) => group.rows.some((row) => row.depth === 0 && row.unread))
}

export interface AgentRow {
  agentId: string
  teamId: string
  name: string
  icon: string
  color: string
  busy: boolean
  needsYou: boolean
  /** S-3: the viewer's own Assistant (agents.role assistant): the brand
   *  mark instead of icon/color, always first. */
  assistant: boolean
}

/** NEW-2: an `agents` definition row as the list needs it. */
export interface AgentLike {
  id: string
  teamId: string
  name: string
  icon: string | null
  color: string | null
  sortOrder: number
  /** S-3: contract `agentRole` (absent = `agent`). */
  role?: string | null
  /** S-3: who owns it (an Assistant shows to its owner only). */
  ownerId?: string | null
}

/**
 * The rail's Agents section (above Boards): one row per `agents` definition
 * of the team (NEW-2), icon + color like boards, by `sortOrder` then name
 * then id. The spinner / needs-you dot read the VIEWER's own conversation
 * session with the agent (a root with `agentId` and `userId == me`); a
 * member without one sees a quiet row.
 */
export function listAgents(
  agents: readonly AgentLike[],
  sessions: readonly TreeSession[],
  teamId: string | null,
  me: string | null
): AgentRow[] {
  const mine = new Map<string, TreeSession>()
  for (const s of sessions) {
    if (s.parentId === null && s.agentId && s.userId === me) mine.set(s.agentId, s)
  }
  const assistant = (a: AgentLike) => a.role === `assistant`
  return agents
    .filter((a) => teamId === null || a.teamId === teamId)
    // S-3: another member's Assistant is never listed (agent-settings.json
    // `assistant.visibleCases`); the viewer's own comes first.
    .filter((a) => !assistant(a) || a.ownerId === me)
    .slice()
    .sort(
      (a, b) =>
        Number(!assistant(a)) - Number(!assistant(b)) ||
        a.sortOrder - b.sortOrder ||
        a.name.localeCompare(b.name) ||
        (a.id < b.id ? -1 : 1)
    )
    .map((a) => {
      const conversation = mine.get(a.id)
      return {
        agentId: a.id,
        teamId: a.teamId,
        name: a.name,
        icon: a.icon ?? listFixture.agentDefaults.icon,
        color: a.color ?? listFixture.agentDefaults.color,
        busy: conversation?.busy === true && conversation.status !== `ended`,
        needsYou: conversation?.needsInput === true && conversation.status !== `ended`,
        assistant: assistant(a),
      }
    })
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

/** S-3 (session-thread.json `chatModeCases`): THE thread screen's mode: an
 *  agent conversation paints chat bubbles, a thread its transcript. */
export type ThreadChatMode = `bubbles` | `transcript`
export function threadChatMode(root: { agentId?: string | null }): ThreadChatMode {
  return root.agentId ? `bubbles` : `transcript`
}

// Thread 58cb32df (owner 2026-10-23, #EXP-1373): Chats, the phone app's
// WhatsApp-like list and the context menus ×4. Pure and byte-locked by
// `fixtures/chats.json` (its `$comment` holds the rules); mirrors: iOS ExpCore
// `Chats.swift`, Android `domain/Chats.kt`, desktop `domain::chats` (every
// group: the list is on every form factor since #EXP-1380).

import fixture from "../fixtures/chats.json" with { type: "json" }
import {
  threadRowLeadPr,
  threadRowMarkState,
  type AgentRow,
  type ThreadRowWord,
  type ThreadsListGroup,
  type TreeMessage,
  type TreeSession,
  type TreeSessionRead,
} from "./session-tree"

export const CHATS_COPY = fixture.copy
export const CHATS_TEST_IDS = fixture.testIds
export const CHATS_SHELL = fixture.shell
/** #EXP-1380: the transcript window, its step and the follow slack (px). */
export const CHATS_TRANSCRIPT = fixture.transcript

/** The window's first painted row (`windowCases`, the WINDOW rule):
 *  `pinnedStart` = the start the chat's FIRST paint fixed (null before it /
 *  on reopen), `loadedSteps` = Load earlier presses since. A waiting
 *  plan / question / ask card is never cut off; arrivals never re-slide the
 *  start; one press = one `windowStep`. */
export function transcriptWindowStart(input: {
  rowCount: number
  waitingIndexes: readonly number[]
  pinnedStart: number | null
  loadedSteps: number
}): number {
  const tail = Math.max(0, input.rowCount - CHATS_TRANSCRIPT.window)
  let base = input.pinnedStart === null ? tail : Math.min(Math.max(0, input.pinnedStart), tail)
  for (const index of input.waitingIndexes) if (index >= 0 && index < input.rowCount && index < base) base = index
  return Math.max(0, base - Math.max(0, input.loadedSteps) * CHATS_TRANSCRIPT.windowStep)
}

/** The indexes of a transcript's WAITING cards (buildSessionThread rows):
 *  a plan without a decision, an unanswered question (an `ask` draws no
 *  row of its own). `transcriptWindowStart`'s `waitingIndexes`. */
export function transcriptWaitingIndexes(
  rows: readonly { type: string; decision?: string | null; answered?: boolean }[]
): number[] {
  const out: number[] = []
  rows.forEach((row, index) => {
    if ((row.type === `plan` && row.decision === null) || (row.type === `question` && row.answered === false)) out.push(index)
  })
  return out
}

export type ChatFilter = `all` | `needs_you` | `running` | `unread`
export const CHAT_FILTERS = fixture.filters as readonly ChatFilter[]

export type ChatLead = `needs_you` | `working` | `open` | `merged` | `waiting`

export type ChatPreviewKind =
  | `working`
  | `question`
  | `plan`
  | `you`
  | `agent`
  | `picture`
  | `surface`
  | `new`
  | `compact`
  | `empty`

export interface ChatPreview {
  kind: ChatPreviewKind
  /** The whole line as painted (the prefix included); clients truncate. */
  text: string
}

export interface ChatRow {
  sessionId: string
  teamId: string
  title: string
  deviceId: string | null
  group: ThreadsListGroup[`key`]
  lead: ChatLead
  preview: ChatPreview
  activityAt: string
  time: string
  /** Agent messages in the ROOT conversation newer than the root's read. */
  unreadCount: number
  needsYou: boolean
  busy: boolean
  word: ThreadRowWord | null
  /** S-3: the chat is DONE (every PR of the root merged and nothing works
   *  or needs you): it stays in the list, dimmed (`shell.dimOpacity`). */
  dim: boolean
}

export interface ChatList {
  rows: ChatRow[]
  /** Chip counts (All has none). */
  counts: Record<Exclude<ChatFilter, `all`>, number>
  /** #EXP-1388: archived chats of the active team (the Archived entry). */
  archived: number
}

/** #EXP-1388: the list's view: the chats, or the Archived page. */
export type ChatListView = `list` | `archived`

const ms = (iso: string | null | undefined): number | null => {
  if (!iso) return null
  const t = Date.parse(iso)
  return Number.isNaN(t) ? null : t
}

const PREVIEW_CARDS = new Set([`question`, `plan`, `ask`, `picture`, `surface`])

/** A builtin command line a person sent (`/new`, `/compact …`): the
 *  transcript paints it as a divider, never a bubble. */
export function commandDivider(body: string): `new` | `compact` | null {
  const text = body.trim()
  if (text === `/new`) return `new`
  if (text === `/compact` || text.startsWith(`/compact `)) return `compact`
  return null
}

/** Markdown → one plain line (the list preview's text). */
export function plainLine(markdown: string): string {
  return markdown
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ` `)
    .replace(/\[([^\]]*)\]\([^)]*\)/g, `$1`)
    .split(`\n`)
    .map((line) => line.replace(/^\s*(?:#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+)/, ``))
    .join(` `)
    .replace(/\*\*|__|~~|`/g, ``)
    .replace(/(^|\s)[*_](\S)/g, `$1$2`)
    .replace(/(\S)[*_](?=\s|$)/g, `$1`)
    .replace(/\s+/g, ` `)
    .trim()
}

/** A plan / question card's preview line. #EXP-1399: an `ask` mirror
 *  reads its body, the bus's `askText` (`Asks: <question>` on its first
 *  line with text | `Proposes a plan:` then the plan's first block), which
 *  the list digest keeps too. A PRIVATE origin (the server's neutral body,
 *  or a session outside `visible`) shows the private copy, never its text. */
export function askPreview(
  message: Pick<TreeMessage, `card` | `body` | `authorSessionId`>,
  visible?: ReadonlySet<string> | null
): { kind: `plan` | `question`; text: string } {
  const copy = fixture.copy.preview
  const card = message.card!
  if (card.kind === `plan`) return { kind: `plan`, text: copy.plan.replace(`{text}`, plainLine(String(card.plan ?? ``))) }
  if (card.kind !== `ask`) {
    return { kind: `question`, text: copy.question.replace(`{text}`, plainLine(String(card.question ?? ``))) }
  }
  const body = message.body.trim()
  const plan = body.startsWith(ASK_PLAN_LEAD) || body.startsWith(ASK_PRIVATE_PLAN_LEAD)
  const hidden =
    body.startsWith(ASK_PRIVATE_PLAN_LEAD) ||
    body.startsWith(ASK_PRIVATE_QUESTION_LEAD) ||
    (visible != null && !visible.has(askOrigin(message)))
  if (hidden) return plan ? { kind: `plan`, text: copy.planPrivate } : { kind: `question`, text: copy.questionPrivate }
  if (plan) {
    const block = plainLine(body.slice(ASK_PLAN_LEAD.length).trim().split(/\n\s*\n/)[0] ?? ``)
    return { kind: `plan`, text: copy.plan.replace(`{text}`, block) }
  }
  const first = body.split(`\n`).find((line) => plainLine(line) !== ``) ?? ``
  const question = plainLine(first.startsWith(ASK_QUESTION_LEAD) ? first.slice(ASK_QUESTION_LEAD.length) : first)
  return { kind: `question`, text: copy.question.replace(`{text}`, question) }
}

/** An `ask` mirror's origin session: its card's, else its author (the
 *  digest keeps only the author). */
function askOrigin(message: Pick<TreeMessage, `card` | `authorSessionId`>): string {
  const id = message.card?.sessionId
  return typeof id === `string` ? id : (message.authorSessionId ?? ``)
}

export const ASK_QUESTION_LEAD = fixture.copy.preview.askQuestionLead
export const ASK_PLAN_LEAD = fixture.copy.preview.askPlanLead
/** #EXP-1399 review: the server's NEUTRAL mirror body (a reader may not see
 *  the origin): no card text. */
export const ASK_PRIVATE_QUESTION_LEAD = fixture.copy.preview.askPrivateQuestionLead
export const ASK_PRIVATE_PLAN_LEAD = fixture.copy.preview.askPrivatePlanLead
/** #EXP-1408: the bus's NEUTRAL lines for a private chat (its reply, its
 *  end report, its pull request) where a reader may not see it. */
export const PRIVATE_REPLY_LEAD = fixture.copy.preview.privateReplyLead
export const PRIVATE_REPORT_LEAD = fixture.copy.preview.privateReportLead
export const PRIVATE_PR_LEAD = fixture.copy.preview.privatePrLead

/** The root conversation's preview line (`previewCases`). */
export function chatPreview(input: {
  rootId: string
  messages: readonly TreeMessage[]
  needsYou: boolean
  busy: boolean
  /** #EXP-1399 review: the viewer's synced session ids (absent = no check). */
  visibleSessionIds?: readonly string[] | null
}): ChatPreview {
  const copy = fixture.copy.preview
  const visible = input.visibleSessionIds ? new Set(input.visibleSessionIds) : null
  const own = input.messages
    .filter((m) => m.sessionId === input.rootId && (m.authorKind === `user` || m.authorKind === `agent`))
    .filter((m) => (m.card ? PREVIEW_CARDS.has(String(m.card.kind)) : m.body.trim() !== ``))
    .sort((a, b) => (ms(b.createdAt) ?? 0) - (ms(a.createdAt) ?? 0) || (a.id < b.id ? 1 : -1))
  if (input.needsYou) {
    const waiting = own.find(
      (m) => m.answeredAt === null && (m.card?.kind === `question` || m.card?.kind === `plan` || m.card?.kind === `ask`)
    )
    if (waiting) return askPreview(waiting, visible)
  }
  if (input.busy) return { kind: `working`, text: copy.working }
  const newest = own[0]
  if (!newest) return { kind: `empty`, text: copy.empty }
  if (newest.authorKind === `user`) {
    const divider = commandDivider(newest.body)
    if (divider) return { kind: divider, text: fixture.copy.dividers[divider] }
    return { kind: `you`, text: copy.you.replace(`{text}`, plainLine(newest.body)) }
  }
  switch (newest.card?.kind) {
    case `picture`:
      return { kind: `picture`, text: copy.picture }
    case `surface`:
      return {
        kind: `surface`,
        text: typeof newest.card.title === `string` && newest.card.title.trim() ? newest.card.title.trim() : copy.surface,
      }
    case `question`:
    case `ask`:
    case `plan`:
      return askPreview(newest, visible)
    default:
      // #EXP-1408: the bus's neutral relay of a private chat: its lead only.
      if (newest.body.startsWith(PRIVATE_REPLY_LEAD)) return { kind: `agent`, text: PRIVATE_REPLY_LEAD }
      return { kind: `agent`, text: plainLine(newest.body) }
  }
}

const pad2 = (n: number) => String(n).padStart(2, `0`)

/** The trailing time label (`timeCases`): the viewer's local day by
 *  `utcOffsetMinutes`. */
export function chatTimeLabel(at: string, now: string, utcOffsetMinutes: number): string {
  const copy = fixture.copy.time
  const shift = utcOffsetMinutes * 60 * 1000
  const local = new Date((ms(at) ?? 0) + shift)
  const today = new Date((ms(now) ?? 0) + shift)
  const day = (d: Date) => Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()) / 86_400_000
  const diff = day(today) - day(local)
  if (diff <= 0) return `${pad2(local.getUTCHours())}:${pad2(local.getUTCMinutes())}`
  if (diff === 1) return copy.yesterday
  if (diff < 7) return copy.weekdaysShort[(local.getUTCDay() + 6) % 7]!
  const month = copy.monthsShort[local.getUTCMonth()]!
  const date = `${month} ${local.getUTCDate()}`
  return local.getUTCFullYear() === today.getUTCFullYear() ? date : `${date}, ${local.getUTCFullYear()}`
}

function unreadCount(rootId: string, messages: readonly TreeMessage[], reads: readonly TreeSessionRead[]): number {
  const read = ms(reads.find((r) => r.sessionId === rootId)?.readAt) ?? -1
  return messages.filter(
    (m) => m.sessionId === rootId && m.authorKind === `agent` && m.card?.kind !== `event` && (ms(m.createdAt) ?? 0) > read
  ).length
}

/**
 * The phone Chats list (`listCases`): the ROOT rows of `partitionThreads`
 * (active team) flattened and sorted by activity desc (ties by id), each
 * with its lead, preview, time and unread count, then filtered by the chip.
 */
export function chatList(input: {
  groups: readonly ThreadsListGroup[]
  messages: readonly TreeMessage[]
  reads?: readonly TreeSessionRead[]
  /** The ROOT's PR states by session id (absent = none). */
  prStates?: Readonly<Record<string, readonly string[]>>
  /** threadRowWord by session id (absent = none). */
  words?: Readonly<Record<string, ThreadRowWord | null>>
  filter: ChatFilter
  /** #EXP-1388: absent = `list`. */
  view?: ChatListView
  now: string
  utcOffsetMinutes: number
  /** #EXP-1399 review: the viewer's synced session ids (`chatPreview`). */
  visibleSessionIds?: readonly string[] | null
}): ChatList {
  const reads = input.reads ?? []
  const archivedIds = new Set(reads.filter((r) => r.archivedAt).map((r) => r.sessionId))
  const isArchived = (row: ChatRow) => !row.needsYou && archivedIds.has(row.sessionId)
  const wantArchived = input.view === `archived`
  const rows: ChatRow[] = input.groups
    .flatMap((group) => group.rows.filter((row) => row.depth === 0).map((row) => ({ row, group: group.key })))
    .map(({ row, group }) => {
      const mark = threadRowMarkState(row)
      const prStates = input.prStates?.[row.sessionId] ?? []
      const pr = mark === `waiting` ? threadRowLeadPr(prStates) : null
      const lead: ChatLead = mark === `needs_you` ? `needs_you` : mark === `working` ? `working` : (pr ?? `waiting`)
      return {
        sessionId: row.sessionId,
        teamId: row.teamId,
        title: row.title,
        deviceId: row.deviceId ?? null,
        group,
        lead,
        preview: chatPreview({
          rootId: row.sessionId,
          messages: input.messages,
          needsYou: row.needsYou,
          busy: row.busy,
          visibleSessionIds: input.visibleSessionIds,
        }),
        activityAt: row.activityAt,
        time: chatTimeLabel(row.activityAt, input.now, input.utcOffsetMinutes),
        unreadCount: unreadCount(row.sessionId, input.messages, reads),
        needsYou: row.needsYou,
        busy: row.busy,
        word: input.words?.[row.sessionId] ?? null,
        dim: mark === `waiting` && prStates.length > 0 && prStates.every((state) => state === `merged`),
      }
    })
    .sort(
      (a, b) =>
        (ms(b.activityAt) ?? 0) - (ms(a.activityAt) ?? 0) || (a.sessionId < b.sessionId ? -1 : 1)
    )
  const all = rows.filter((row) => isArchived(row) === wantArchived)
  const keep = (row: ChatRow, filter: ChatFilter) =>
    filter === `all` ||
    (filter === `needs_you` && row.group === `needs_you`) ||
    (filter === `running` && row.group === `running`) ||
    (filter === `unread` && row.unreadCount > 0)
  return {
    rows: all.filter((row) => keep(row, input.filter)),
    counts: {
      needs_you: all.filter((row) => keep(row, `needs_you`)).length,
      running: all.filter((row) => keep(row, `running`)).length,
      unread: all.filter((row) => keep(row, `unread`)).length,
    },
    archived: rows.filter(isArchived).length,
  }
}

/** #EXP-1388: the Chats nav dot = an unread chat outside the archive (an
 *  archived chat never lights it; one that needs you is never archived). */
export function chatsUnread(groups: readonly ThreadsListGroup[], reads: readonly TreeSessionRead[] = []): boolean {
  const archived = new Set(reads.filter((r) => r.archivedAt).map((r) => r.sessionId))
  return groups.some((group) =>
    group.rows.some((row) => row.depth === 0 && row.unread && (row.needsYou || !archived.has(row.sessionId)))
  )
}

/** A chip's label: its copy, then its count when there is one. */
export function chatFilterLabel(filter: ChatFilter, counts: ChatList[`counts`]): string {
  const label = fixture.copy.filters[filter]
  if (filter === `all`) return label
  const n = counts[filter]
  return n > 0 ? `${label} ${n}` : label
}

export type AgentRing = `needs_you` | `unread` | null

export interface AgentBubble extends AgentRow {
  ring: AgentRing
}

/** The Chats header's agent bubbles (`bubbleCases`), in `listAgents`
 *  order: amber ring = the viewer's conversation needs them, white ring =
 *  an agent message newer than their read of it, else none. */
export function agentBubbles(input: {
  agents: readonly AgentRow[]
  sessions: readonly TreeSession[]
  messages: readonly TreeMessage[]
  reads?: readonly TreeSessionRead[]
  me: string | null
}): AgentBubble[] {
  const reads = input.reads ?? []
  return input.agents.map((agent) => {
    if (agent.needsYou) return { ...agent, ring: `needs_you` as const }
    const conversation = input.sessions.find(
      (s) => s.parentId === null && s.agentId === agent.agentId && s.userId === input.me
    )
    const unread = conversation ? unreadCount(conversation.id, input.messages, reads) > 0 : false
    return { ...agent, ring: unread ? (`unread` as const) : null }
  })
}

/** S-3: an agent row pinned on top of the Chats list (`agentRowCases`). */
export interface AgentChatRow {
  agentId: string
  /** The viewer's conversation with the agent; null = none yet. */
  sessionId: string | null
  name: string
  icon: string
  color: string
  /** The viewer's Assistant: the brand mark instead of icon/color. */
  assistant: boolean
  preview: ChatPreview
  /** `chatTimeLabel` of `activityAt`; empty without a conversation. */
  time: string
  /** The newest person/agent message of the conversation, else its
   *  creation; null without one. */
  activityAt: string | null
  unreadCount: number
  needsYou: boolean
  busy: boolean
}

/** S-3: the Chats list's pinned AGENTS section (`agentRowCases`): one row per
 *  `listAgents` row (that order), filtered by the chip like a chat; none on
 *  the Archived page. */
export function agentChatRows(input: {
  agents: readonly AgentRow[]
  sessions: readonly TreeSession[]
  messages: readonly TreeMessage[]
  reads?: readonly TreeSessionRead[]
  me: string | null
  filter: ChatFilter
  view?: ChatListView
  now: string
  utcOffsetMinutes: number
}): AgentChatRow[] {
  if (input.view === `archived`) return []
  const reads = input.reads ?? []
  return input.agents
    .map((agent): AgentChatRow => {
      const conversation = input.sessions.find(
        (s) => s.parentId === null && s.agentId === agent.agentId && s.userId === input.me
      )
      const id = conversation?.id ?? null
      const newest = id
        ? input.messages
            .filter((m) => m.sessionId === id && (m.authorKind === `user` || m.authorKind === `agent`))
            .reduce<string | null>((at, m) => ((ms(m.createdAt) ?? 0) > (ms(at) ?? -1) ? m.createdAt : at), null)
        : null
      const activityAt = newest ?? conversation?.createdAt ?? null
      return {
        agentId: agent.agentId,
        sessionId: id,
        name: agent.name,
        icon: agent.icon,
        color: agent.color,
        assistant: agent.assistant,
        preview: id
          ? chatPreview({ rootId: id, messages: input.messages, needsYou: agent.needsYou, busy: agent.busy })
          : { kind: `empty`, text: fixture.copy.preview.empty },
        time: activityAt ? chatTimeLabel(activityAt, input.now, input.utcOffsetMinutes) : ``,
        activityAt,
        unreadCount: id ? unreadCount(id, input.messages, reads) : 0,
        needsYou: agent.needsYou,
        busy: agent.busy,
      }
    })
    .filter(
      (row) =>
        input.filter === `all` ||
        (input.filter === `needs_you` && row.needsYou) ||
        (input.filter === `running` && row.busy) ||
        (input.filter === `unread` && row.unreadCount > 0)
    )
}

export type ThreadMenuEntry = `delete`

/** A thread row's context menu ×4 (`menuCases`): S-3 (owner 2026-10-28)
 *  no Stop/End anywhere: Delete for a viewer who can steer the chat and
 *  may delete it (the chat's starter or a team owner, the server's
 *  `sessions.delete` rule, which ends what is live first); else nothing. */
export function threadMenuEntries(input: {
  rootStatus: string
  canSteer: boolean
  canDelete: boolean
}): ThreadMenuEntry[] {
  return input.canSteer && input.canDelete ? [`delete`] : []
}

export type ChatSelectionAction = `markRead` | `markUnread` | `archive` | `unarchive` | `delete`

export interface ChatSelectionRow {
  sessionId: string
  rootStatus: string
  unreadCount: number
  canSteer: boolean
  canDelete: boolean
}

/** #EXP-1388: the selection bar's actions ×4 (`selectionCases`), in bar
 *  order, each with the chats it acts on (selection order): Mark read when
 *  any chat is unread, else Mark unread (all); Archive in the list, Unarchive
 *  on the Archived page (all); Delete only when the viewer may delete every
 *  one (all). S-3: no Stop. */
export function chatSelectionActions(input: {
  rows: readonly ChatSelectionRow[]
  view?: ChatListView
}): { action: ChatSelectionAction; sessionIds: string[] }[] {
  const rows = input.rows
  if (rows.length === 0) return []
  const ids = (list: readonly ChatSelectionRow[]) => list.map((row) => row.sessionId)
  const out: { action: ChatSelectionAction; sessionIds: string[] }[] = [
    { action: rows.some((row) => row.unreadCount > 0) ? `markRead` : `markUnread`, sessionIds: ids(rows) },
    { action: input.view === `archived` ? `unarchive` : `archive`, sessionIds: ids(rows) },
  ]
  if (rows.every((row) => row.canSteer && row.canDelete)) out.push({ action: `delete`, sessionIds: ids(rows) })
  return out
}

/** #EXP-1388: a pointer's range select (shift-click): the ids between the
 *  anchor and the clicked row in list order, both ends included; a missing
 *  anchor = the clicked row alone (`rangeCases`). */
export function chatSelectionRange(order: readonly string[], anchor: string | null, target: string): string[] {
  const to = order.indexOf(target)
  if (to < 0) return []
  const from = anchor === null ? -1 : order.indexOf(anchor)
  if (from < 0) return [target]
  return order.slice(Math.min(from, to), Math.max(from, to) + 1)
}

/** #EXP-1388: the multi-delete confirm's title (`copy.selection.confirmDelete`). */
export function chatDeleteTitle(count: number): string {
  return count === 1 ? fixture.copy.confirmDelete.title : fixture.copy.selection.confirmDeleteMany.replace(`{n}`, String(count))
}

/** #EXP-1388: the selection bar's failure toast: `selectionPartial` when
 *  SOME of the `total` chats failed, else `selectionFailed`. */
export function chatSelectionFailedTitle(failed: number, total: number): string {
  return failed > 0 && failed < total
    ? fixture.copy.toasts.selectionPartial.replace(`{n}`, String(failed)).replace(`{m}`, String(total))
    : fixture.copy.toasts.selectionFailed
}

export type AgentMenuEntry = `compact` | `newConversation` | `settings`

/** The agent chat's header menu (and its bubble's long-press) ×4
 *  (`agentMenuCases`). S-3: no Stop (the composer's stop-turn is the one). */
export function agentMenuEntries(input: { busy: boolean; isOwner: boolean }): AgentMenuEntry[] {
  return [`compact`, `newConversation`, ...(input.isOwner ? ([`settings`] as const) : [])]
}

/** What a menu entry sends (`agentMenuCases`): Compact and New conversation
 *  are the builtin lines, so a menu pick and a typed command are one path. */
export const AGENT_MENU_SENDS: Partial<Record<AgentMenuEntry, string>> = fixture.agentMenu.sends

export type SurfaceReply = { text: string } | null

/** A press inside an inline preview (`replyCases`): an event named `reply`
 *  with a non-empty `context.text` posts that text as the person's message;
 *  anything else does nothing. */
export function surfaceReply(event: { name?: unknown; context?: unknown }): SurfaceReply {
  if (event.name !== fixture.surfaceCard.replyEvent) return null
  const context = event.context as { text?: unknown } | null | undefined
  const text = typeof context?.text === `string` ? context.text.trim() : ``
  return text ? { text } : null
}

// ── #EXP-1380: the list's per-conversation DIGEST (web) ──────────────────
// The web list never syncs a transcript: the server keeps, on each
// conversation row (`conversations.digest`, trigger `conversation_digest`
// in 0001_triggers.sql), exactly the messages the list's rules read, and
// `digestMessages` turns it back into TreeMessages, so `sessionTitle`,
// `partitionThreads`, `chatList` and `agentBubbles` run UNCHANGED over it
// and answer as they would over the whole conversation (an unread count
// past `DIGEST_AGENT_TAIL` reads `DIGEST_AGENT_TAIL`). The SQL mirrors
// `conversationDigest` below; `chats.test.ts` locks the equivalence.

/** The newest non-event agent messages a digest keeps the times of. */
export const DIGEST_AGENT_TAIL = 100
/** A digest message's body / card text is cut to this many characters. */
export const DIGEST_TEXT_MAX = 500

export interface DigestMessage {
  id: string
  authorKind: string
  authorSessionId: string | null
  body: string
  card: { kind: string; question?: string; plan?: string; title?: string } | null
  answeredAt: string | null
  createdAt: string
}

export interface ConversationDigest {
  /** `firstPrompt`'s message (the title's last rung). */
  first: DigestMessage | null
  /** The newest preview message (`chatPreview`'s candidates). */
  preview: DigestMessage | null
  /** The newest WAITING question / plan / ask among them. */
  waiting: DigestMessage | null
  /** The newest agent message, event cards included (`partitionThreads`). */
  agentAt: string | null
  /** The newest `DIGEST_AGENT_TAIL` non-event agent messages' times,
   *  oldest first (`unreadCount`). */
  agentTimes: string[]
}

const msOr0 = (iso: string | null | undefined) => ms(iso) ?? 0

/** The index of a body's first usable line (the title rule's `firstLine`:
 *  non-blank, not an image embed), -1 = none. */
function firstUsableLineAt(body: string): number {
  return body.split(`\n`).findIndex((part) => {
    const line = part.trim()
    return line.length > 0 && !line.startsWith(`![`)
  })
}

/** Cut to `DIGEST_TEXT_MAX` characters (code points, SQL `left()`'s count). */
function cut(text: string): string {
  if (text.length <= DIGEST_TEXT_MAX) return text
  const chars = Array.from(text)
  return chars.length > DIGEST_TEXT_MAX ? chars.slice(0, DIGEST_TEXT_MAX).join(``) : text
}

/** The first prompt's body from its first usable line on (the skipped
 *  blank / image lines dropped BEFORE the cut), so the title rule reads the
 *  same line over the digest as over the whole message (review of #1218). */
export function digestFirstBody(body: string): string {
  const at = firstUsableLineAt(body)
  return at <= 0 ? body : body.split(`\n`).slice(at).join(`\n`)
}

const IMAGE_EMBED = /!\[[^\]]*\]\([^)]*\)/g

/** A text from its first line `plainLine` keeps anything of (the blank and
 *  image-only lines before it dropped BEFORE the cut, so the preview reads
 *  the same words); none = its first non-blank line (an image-only body
 *  stays one). */
export function digestText(text: string): string {
  const lines = text.split(`\n`)
  const at = lines.findIndex((line) => line.replace(IMAGE_EMBED, ``).trim() !== ``)
  if (at !== -1) return at === 0 ? text : lines.slice(at).join(`\n`)
  return lines.find((line) => line.trim() !== ``) ?? text
}

function digestMessage(m: TreeMessage, body: string = digestText(m.body)): DigestMessage {
  const card = m.card
    ? {
        kind: String(m.card.kind),
        ...(typeof m.card.question === `string` ? { question: cut(digestText(m.card.question)) } : {}),
        ...(typeof m.card.plan === `string` ? { plan: cut(digestText(m.card.plan)) } : {}),
        ...(typeof m.card.title === `string` ? { title: cut(m.card.title) } : {}),
      }
    : null
  return {
    id: m.id,
    authorKind: m.authorKind,
    authorSessionId: m.authorSessionId ?? null,
    body: cut(body),
    card,
    answeredAt: m.answeredAt,
    createdAt: m.createdAt,
  }
}

/** THE digest of one session's conversation (`sessionId` = its session). */
export function conversationDigest(sessionId: string, messages: readonly TreeMessage[]): ConversationDigest {
  const own = messages.filter((m) => m.sessionId === sessionId)
  const oldestFirst = (a: TreeMessage, b: TreeMessage) =>
    msOr0(a.createdAt) - msOr0(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  const first =
    own
      .filter(
        (m) =>
          (m.card === null || m.card === undefined) &&
          (m.authorKind === `user` || (!!m.authorSessionId && m.authorSessionId !== sessionId)) &&
          firstUsableLineAt(m.body) !== -1
      )
      .sort(oldestFirst)[0] ?? null
  const candidates = own
    .filter((m) => m.authorKind === `user` || m.authorKind === `agent`)
    .filter((m) => (m.card ? PREVIEW_CARDS.has(String(m.card.kind)) : m.body.trim() !== ``))
    .sort((a, b) => oldestFirst(b, a))
  const waiting =
    candidates.find(
      (m) => m.answeredAt === null && (m.card?.kind === `question` || m.card?.kind === `plan` || m.card?.kind === `ask`)
    ) ?? null
  const agents = own.filter((m) => m.authorKind === `agent`).sort(oldestFirst)
  const tail = agents.filter((m) => m.card?.kind !== `event`).slice(-DIGEST_AGENT_TAIL)
  return {
    first: first ? digestMessage(first, digestFirstBody(first.body)) : null,
    preview: candidates[0] ? digestMessage(candidates[0]) : null,
    waiting: waiting ? digestMessage(waiting) : null,
    agentAt: agents.length > 0 ? agents[agents.length - 1]!.createdAt : null,
    agentTimes: tail.map((m) => m.createdAt),
  }
}

/** A digest back as the TreeMessages the list's rules read: its messages
 *  (once each) plus one bodiless agent message per kept time (the one a
 *  kept message already stands for skipped) and the newest agent time as an
 *  event card. */
export function digestMessages(sessionId: string, digest: ConversationDigest | null | undefined): TreeMessage[] {
  if (!digest) return []
  const out: TreeMessage[] = []
  const seen = new Set<string>()
  // Kept agent messages by time (a multiset: two may share a time).
  const counted = new Map<number, number>()
  for (const m of [digest.first, digest.preview, digest.waiting]) {
    if (!m || seen.has(m.id)) continue
    seen.add(m.id)
    out.push({
      id: m.id,
      sessionId,
      authorKind: m.authorKind,
      authorId: null,
      authorSessionId: m.authorSessionId,
      body: m.body,
      card: m.card,
      answeredAt: m.answeredAt,
      consumedAt: null,
      createdAt: m.createdAt,
    })
    if (m.authorKind === `agent` && m.card?.kind !== `event`) {
      const at = msOr0(m.createdAt)
      counted.set(at, (counted.get(at) ?? 0) + 1)
    }
  }
  digest.agentTimes.forEach((at, index) => {
    const kept = counted.get(msOr0(at)) ?? 0
    if (kept > 0) {
      counted.set(msOr0(at), kept - 1)
      return
    }
    out.push({
      id: `digest:${sessionId}:${index}`,
      sessionId,
      authorKind: `agent`,
      authorId: null,
      authorSessionId: null,
      body: ``,
      card: null,
      answeredAt: null,
      consumedAt: null,
      createdAt: at,
    })
  })
  if (digest.agentAt) {
    out.push({
      id: `digest:${sessionId}:agent`,
      sessionId,
      authorKind: `agent`,
      authorId: null,
      authorSessionId: null,
      body: ``,
      card: { kind: `event` },
      answeredAt: null,
      consumedAt: null,
      createdAt: digest.agentAt,
    })
  }
  return out
}

/** #EXP-1383: the agent chat's "Instructions changed" Dock banner
 *  (`agentHint.cases`): the root session fetched its brief before the
 *  agent's instructions last changed. Either stamp missing = no hint. */
export function agentInstructionsChanged(input: {
  sessionStamp: string | null | undefined
  agentStamp: string | null | undefined
}): boolean {
  if (!input.sessionStamp || !input.agentStamp) return false
  return new Date(input.sessionStamp).getTime() < new Date(input.agentStamp).getTime()
}

/** #EXP-1383: the banner row itself (dock.json kind `banner`). */
export const AGENT_HINT_BANNER = fixture.agentHint.banner

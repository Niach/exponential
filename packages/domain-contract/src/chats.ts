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
}

export interface ChatList {
  rows: ChatRow[]
  /** Chip counts (All has none). */
  counts: Record<Exclude<ChatFilter, `all`>, number>
}

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

/** The root conversation's preview line (`previewCases`). */
export function chatPreview(input: {
  rootId: string
  messages: readonly TreeMessage[]
  needsYou: boolean
  busy: boolean
}): ChatPreview {
  const copy = fixture.copy.preview
  const own = input.messages
    .filter((m) => m.sessionId === input.rootId && (m.authorKind === `user` || m.authorKind === `agent`))
    .filter((m) => (m.card ? PREVIEW_CARDS.has(String(m.card.kind)) : m.body.trim() !== ``))
    .sort((a, b) => (ms(b.createdAt) ?? 0) - (ms(a.createdAt) ?? 0) || (a.id < b.id ? 1 : -1))
  if (input.needsYou) {
    const waiting = own.find(
      (m) => m.answeredAt === null && (m.card?.kind === `question` || m.card?.kind === `plan` || m.card?.kind === `ask`)
    )
    if (waiting) {
      const kind = waiting.card!.kind === `plan` ? `plan` : `question`
      const body = kind === `plan` ? String(waiting.card!.plan ?? ``) : String(waiting.card!.question ?? ``)
      return { kind, text: copy[kind].replace(`{text}`, plainLine(body)) }
    }
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
      return { kind: `question`, text: copy.question.replace(`{text}`, plainLine(String(newest.card.question ?? ``))) }
    case `plan`:
      return { kind: `plan`, text: copy.plan.replace(`{text}`, plainLine(String(newest.card.plan ?? ``))) }
    default:
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
  now: string
  utcOffsetMinutes: number
}): ChatList {
  const reads = input.reads ?? []
  const all: ChatRow[] = input.groups
    .flatMap((group) => group.rows.filter((row) => row.depth === 0).map((row) => ({ row, group: group.key })))
    .map(({ row, group }) => {
      const mark = threadRowMarkState(row)
      const pr = mark === `waiting` ? threadRowLeadPr(input.prStates?.[row.sessionId] ?? []) : null
      const lead: ChatLead = mark === `needs_you` ? `needs_you` : mark === `working` ? `working` : (pr ?? `waiting`)
      return {
        sessionId: row.sessionId,
        teamId: row.teamId,
        title: row.title,
        deviceId: row.deviceId ?? null,
        group,
        lead,
        preview: chatPreview({ rootId: row.sessionId, messages: input.messages, needsYou: row.needsYou, busy: row.busy }),
        activityAt: row.activityAt,
        time: chatTimeLabel(row.activityAt, input.now, input.utcOffsetMinutes),
        unreadCount: unreadCount(row.sessionId, input.messages, reads),
        needsYou: row.needsYou,
        busy: row.busy,
        word: input.words?.[row.sessionId] ?? null,
      }
    })
    .sort(
      (a, b) =>
        (ms(b.activityAt) ?? 0) - (ms(a.activityAt) ?? 0) || (a.sessionId < b.sessionId ? -1 : 1)
    )
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
  }
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

export type ThreadMenuEntry = `stop` | `stopAndDelete` | `delete`

/** A thread row's context menu ×4 (`menuCases`): live → Stop, Stop and
 *  delete; ended → Delete; nothing for a viewer who cannot steer it. The
 *  delete entries need `canDelete` (the chat's starter or a team owner, the
 *  server's `sessions.delete` rule). */
export function threadMenuEntries(input: {
  rootStatus: string
  canSteer: boolean
  canDelete: boolean
}): ThreadMenuEntry[] {
  if (!input.canSteer) return []
  if (input.rootStatus === `ended`) return input.canDelete ? [`delete`] : []
  return input.canDelete ? [`stop`, `stopAndDelete`] : [`stop`]
}

export type AgentMenuEntry = `compact` | `newConversation` | `stop` | `settings`

/** The agent chat's header menu (and its bubble's long-press) ×4
 *  (`agentMenuCases`). */
export function agentMenuEntries(input: { busy: boolean; isOwner: boolean }): AgentMenuEntry[] {
  return [
    `compact`,
    `newConversation`,
    ...(input.busy ? ([`stop`] as const) : []),
    ...(input.isOwner ? ([`settings`] as const) : []),
  ]
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

function firstUsableLine(body: string): boolean {
  return body.split(`\n`).some((part) => {
    const line = part.trim()
    return line.length > 0 && !line.startsWith(`![`)
  })
}

function digestMessage(m: TreeMessage): DigestMessage {
  const card = m.card
    ? {
        kind: String(m.card.kind),
        ...(typeof m.card.question === `string` ? { question: m.card.question.slice(0, DIGEST_TEXT_MAX) } : {}),
        ...(typeof m.card.plan === `string` ? { plan: m.card.plan.slice(0, DIGEST_TEXT_MAX) } : {}),
        ...(typeof m.card.title === `string` ? { title: m.card.title.slice(0, DIGEST_TEXT_MAX) } : {}),
      }
    : null
  return {
    id: m.id,
    authorKind: m.authorKind,
    authorSessionId: m.authorSessionId ?? null,
    body: m.body.slice(0, DIGEST_TEXT_MAX),
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
          firstUsableLine(m.body)
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
    first: first ? digestMessage(first) : null,
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

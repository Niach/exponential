// #EXP-1736: ONE transcript ×4, every chat a group chat. Who sent each row,
// which rows share an avatar, the gaps between them.
// fixtures/transcript-grouping.json holds the rules + cases; mirrors: desktop
// `domain::transcript_grouping`, iOS ExpCore `TranscriptGrouping.swift`,
// Android `domain/TranscriptGrouping.kt`.

import fixture from "../fixtures/transcript-grouping.json" with { type: "json" }

export const TRANSCRIPT_LAYOUT = fixture.layout
export const TRANSCRIPT_COPY = fixture.copy

export type TranscriptItemKind = `message` | `trace` | `full`

export interface TranscriptItem {
  id: string
  kind: TranscriptItemKind
  /** `message` only: `user` | `agent` | `system`. */
  authorKind?: string | null
  authorId?: string | null
  authorSessionId?: string | null
}

export interface TranscriptChat {
  /** The session whose conversation this is. */
  sessionId: string
  title: string
  agentId: string | null
  /** The session that started it (session-chat.json `origin`, from the
   *  synced rows), else null. */
  originSessionId: string | null
  /** The items begin at the conversation's first message (absent = true). */
  fromStart?: boolean
}

export interface TranscriptInput {
  viewerId: string | null
  chat: TranscriptChat
  members: readonly { id: string; name: string }[]
  agents: readonly { id: string; name: string }[]
  sessions: readonly { id: string; title: string; agentId: string | null }[]
  items: readonly TranscriptItem[]
}

export interface TranscriptSender {
  kind: `member` | `agent` | `session`
  id: string
  name: string
  /** A cross-session sender: the session a press opens. */
  link: string | null
}

export interface TranscriptRowLayout {
  id: string
  side: `own` | `other` | `full`
  sender: TranscriptSender | null
  avatar: boolean
  name: boolean
  gap: number
}

export function transcriptLayout(input: TranscriptInput): TranscriptRowLayout[] {
  const { chat } = input
  const sessions = new Map(input.sessions.map((s) => [s.id, s]))
  const members = new Map(input.members.map((m) => [m.id, m.name]))
  const agents = new Map(input.agents.map((a) => [a.id, a.name]))
  const fromSession = (id: string | null | undefined): TranscriptSender | null => {
    const session = id && id !== chat.sessionId ? sessions.get(id) : undefined
    if (!session) return null
    return session.agentId
      ? { kind: `agent`, id: session.agentId, name: session.title, link: session.id }
      : { kind: `session`, id: session.id, name: session.title, link: session.id }
  }
  const ownAgent: TranscriptSender = chat.agentId
    ? { kind: `agent`, id: chat.agentId, name: agents.get(chat.agentId) ?? chat.title, link: null }
    : { kind: `session`, id: chat.sessionId, name: chat.title, link: null }
  let originPending = Boolean(chat.originSessionId) && chat.fromStart !== false

  const out: TranscriptRowLayout[] = []
  let prevKey: string | null = null
  let groupNamed = false
  for (const item of input.items) {
    let side: TranscriptRowLayout[`side`]
    let sender: TranscriptSender | null = null
    if (item.kind === `full` || (item.kind === `message` && item.authorKind === `system`)) {
      side = `full`
    } else if (item.kind === `trace`) {
      side = `other`
      sender = ownAgent
    } else {
      const own = item.authorKind === `user` && Boolean(input.viewerId) && item.authorId === input.viewerId
      const cross = fromSession(item.authorSessionId)
      // The origin: the conversation's first INPUT (a person's row or a
      // known session's), unless it names its own sender; the agent's own
      // output never takes it.
      const isInput = item.authorKind === `user` || cross !== null
      const origin = originPending && isInput && !cross ? fromSession(chat.originSessionId) : null
      if (isInput) originPending = false
      if (cross) {
        side = `other`
        sender = cross
      } else if (origin) {
        side = `other`
        sender = origin
      } else if (own) {
        side = `own`
      } else {
        side = `other`
        if (item.authorKind === `user`) {
          const id = item.authorId ?? ``
          sender = { kind: `member`, id, name: members.get(id) ?? TRANSCRIPT_COPY.formerMember, link: null }
        } else sender = ownAgent
      }
    }
    const key = side === `full` ? null : side === `own` ? `own` : `${sender!.kind}:${sender!.id}:${sender!.link ?? ``}`
    const sameGroup = key !== null && key === prevKey
    if (!sameGroup) groupNamed = false
    const name = side === `other` && item.kind === `message` && !groupNamed
    if (name) groupNamed = true
    out.push({
      id: item.id,
      side,
      sender,
      avatar: side === `other` && !sameGroup,
      name,
      gap: out.length === 0 ? 0 : sameGroup ? TRANSCRIPT_LAYOUT.gapInGroup : TRANSCRIPT_LAYOUT.gapBetween,
    })
    prevKey = key
  }
  return out
}

export interface CrossSessionInput {
  kind: `report` | `message`
  name: string
  sessionId: string
  body: string
}

const INPUT_PATTERN = /^(Report|Message) from (.+?) \(session ([^,)\s]+)(, ended)?\): ([\s\S]*)$/

/** A session input as the device words it (`coding::pickup::input_text`). */
export function crossSessionInput(text: string): CrossSessionInput | null {
  const match = INPUT_PATTERN.exec(text)
  if (!match) return null
  return {
    kind: match[1] === `Report` ? `report` : `message`,
    name: match[2]!,
    sessionId: match[3]!,
    body: match[5]!,
  }
}

// #EXP-1772: only real messages are bubbles. A system input (a schedule or
// event firing, a PR or session status line) is THE inline marker, drawn
// only above the agent's reply to it.

export type FeedMarkerKey = keyof typeof fixture.copy.markers

export type FeedInput =
  | { kind: `marker`; key: FeedMarkerKey; label: string }
  | { kind: `relay`; name: string; sessionId: string; body: string; report: boolean }

const MARKERS = fixture.copy.markers as Record<string, string>
const TAG_PATTERN = /^\[([a-z_]+)\] /
const SCHEDULE_PATTERN = /^Schedule `[^`\n]+`[^\n]* fired/
const EVENT_PATTERN = /^Event `[^`\n]+` \(from /

function marker(key: FeedMarkerKey): FeedInput {
  return { kind: `marker`, key, label: MARKERS[key]! }
}

/** A feed input: the device's tagged system line = a marker, another
 *  session's input = a relay, else null (a person's prompt). */
export function feedInput(text: string): FeedInput | null {
  const tag = TAG_PATTERN.exec(text)
  if (tag && Object.hasOwn(MARKERS, tag[1]!)) return marker(tag[1] as FeedMarkerKey)
  if (SCHEDULE_PATTERN.test(text)) return marker(`schedule`)
  if (EVENT_PATTERN.test(text)) return marker(`event`)
  const cross = crossSessionInput(text)
  if (!cross) return null
  return { kind: `relay`, name: cross.name, sessionId: cross.sessionId, body: cross.body, report: cross.kind === `report` }
}

export type FeedRowKind = `marker` | `input` | `reply` | `other`

/** Per row: is it drawn? A marker only when a reply follows it before the
 *  next input or marker; every other row always. */
export function markersShown(kinds: readonly FeedRowKind[]): boolean[] {
  const shown = kinds.map(() => true)
  let pending: number | null = null
  kinds.forEach((kind, index) => {
    if (kind === `marker` || kind === `input`) {
      if (pending !== null) shown[pending] = false
      pending = kind === `marker` ? index : null
    } else if (kind === `reply`) {
      pending = null
    }
  })
  if (pending !== null) shown[pending] = false
  return shown
}

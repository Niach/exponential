// S-3 STORIES ×4: a session's own show cards (a plan, a question, a result)
// folded into one ring per agent and per chat, for the story ring + viewer.
// TS reference; desktop `domain::stories`, iOS ExpCore `Stories.swift`,
// Android `domain/Stories.kt` replay `fixtures/stories.json` (its `$comment`
// is the spec).

import fixture from "../fixtures/stories.json" with { type: "json" }
import { askDockTitle } from "./ask-dock"

export const STORIES_COPY = fixture.copy
export const STORIES_CONSTANTS = fixture.constants
export const STORY_RING = fixture.ring
export const STORY_PROGRESS = fixture.progress
export const STORY_VIEWER = fixture.viewer
export const STORIES_TEST_IDS = fixture.testIds

export type StoryKind = `plan` | `question` | `result`
export type StoryFooter = `plan` | `choices` | `reply` | `none`

export interface StorySessionInput {
  id: string
  rootId: string
  /** The session's display name (session-title). */
  name: string
  agentId: string | null
  startedByAgentId: string | null
  /** The root's CLI (the chat row's brand mark); the fold ignores it. */
  cli?: string | null
}

export interface StoryAgentInput {
  id: string
  name: string
  assistant: boolean
}

export interface StoryMessageInput {
  id: string
  sessionId: string
  authorSessionId: string | null
  /** Epoch ms. */
  createdAt: number
  answeredAt: number | null
  /** The raw `messages.card`. */
  card: unknown
}

export interface StoryReadInput {
  /** The root session of the read mark. */
  sessionId: string
  readAt: number | null
  storySeenAt: number | null
}

export interface StoriesInput {
  now: number
  sessions: readonly StorySessionInput[]
  agents: readonly StoryAgentInput[]
  messages: readonly StoryMessageInput[]
  reads: readonly StoryReadInput[]
}

export interface StoryPicture {
  label: string
  attachmentId: string
  width: number | null
  height: number | null
  caption: string | null
}

export interface Story {
  id: string
  sessionId: string
  rootId: string
  sessionName: string
  kind: StoryKind
  waiting: boolean
  seen: boolean
  createdAt: number
  meta: string
  title: string | null
  pictures: StoryPicture[]
  surface: boolean
  text: string | null
  footer: StoryFooter
  autoAdvanceMs: number | null
}

export interface StoryRing {
  key: string
  agentId: string | null
  rootId: string | null
  title: string
  assistant: boolean
  waiting: number
  unseen: boolean
  stories: Story[]
}

type Card = Record<string, unknown>

const str = (value: unknown): string | null => (typeof value === `string` ? value : null)
const num = (value: unknown): number | null => (typeof value === `number` ? value : null)

function resolvedPictures(card: Card): StoryPicture[] {
  const list = Array.isArray(card.pictures) ? card.pictures : []
  const out: StoryPicture[] = []
  for (const raw of list) {
    if (!raw || typeof raw !== `object`) continue
    const picture = raw as Card
    const attachmentId = str(picture.attachmentId)
    if (!attachmentId) continue
    out.push({
      label: str(picture.label) ?? ``,
      attachmentId,
      width: num(picture.width),
      height: num(picture.height),
      caption: str(picture.caption),
    })
  }
  return out
}

/** The card's story flag: explicit, else ask/plan or a resolved picture. */
export function isStoryCard(card: unknown): boolean {
  if (!card || typeof card !== `object`) return false
  const c = card as Card
  if (c.kind !== `plan` && c.kind !== `question` && c.kind !== `surface`) return false
  if (str(c.questionId)) return false
  if (typeof c.story === `boolean`) return c.story
  return c.kind !== `surface` || resolvedPictures(c).length > 0
}

/** `<m>m` / `<h>h` / `<d>d`, floored; under a minute = `now`. */
export function storyAge(createdAt: number, now: number): string {
  const minutes = Math.floor(Math.max(0, now - createdAt) / 60_000)
  if (minutes < 1) return fixture.copy.now
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h`
  return `${Math.floor(hours / 24)}d`
}

const byCreated = (a: Story, b: Story) => a.createdAt - b.createdAt || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
const HOUR = 3_600_000

/** THE fold: the viewer's story rings, in display order. */
export function stories(input: StoriesInput): StoryRing[] {
  const sessions = new Map(input.sessions.map((session) => [session.id, session]))
  const agents = new Map(input.agents.map((agent) => [agent.id, agent]))
  const marks = new Map(
    input.reads.map((read) => [read.sessionId, Math.max(read.readAt ?? -Infinity, read.storySeenAt ?? -Infinity)])
  )
  const rings = new Map<string, StoryRing>()

  for (const message of input.messages) {
    if (!message.authorSessionId || message.authorSessionId !== message.sessionId) continue
    if (!isStoryCard(message.card)) continue
    const card = message.card as Card
    const session = sessions.get(message.sessionId)
    if (!session) continue
    const root = sessions.get(session.rootId)
    if (!root) continue
    const kind: StoryKind = card.kind === `plan` ? `plan` : card.kind === `question` ? `question` : `result`
    const waiting =
      kind === `plan`
        ? card.decision == null
        : kind === `question`
          ? message.answeredAt == null && card.answer == null
          : false
    if (kind !== `result` && !waiting) continue
    const seen = message.createdAt <= (marks.get(root.id) ?? -Infinity)
    const age = input.now - message.createdAt
    if (kind === `result`) {
      const keep = seen ? fixture.constants.seenHours : fixture.constants.unseenHours
      if (age >= keep * HOUR) continue
    }
    const pictures = resolvedPictures(card)
    const surface = card.surface != null && typeof card.surface === `object`
    const options = Array.isArray(card.options) ? card.options : []
    const title =
      kind === `result`
        ? str(card.title)?.trim() || null
        : kind === `question`
          ? str(card.question)?.trim() || null
          : askDockTitle(str(card.plan) ?? ``) || null
    const story: Story = {
      id: message.id,
      sessionId: session.id,
      rootId: root.id,
      sessionName: session.name,
      kind,
      waiting,
      seen,
      createdAt: message.createdAt,
      meta: `${fixture.copy[kind]} · ${storyAge(message.createdAt, input.now)}`,
      title,
      pictures,
      surface,
      text: kind === `plan` && pictures.length === 0 && !surface ? (str(card.plan) ?? null) : null,
      footer: kind === `plan` ? `plan` : kind === `question` ? (options.length > 0 ? `choices` : `reply`) : `none`,
      autoAdvanceMs: kind === `result` ? fixture.constants.autoAdvanceMs : null,
    }
    const agentId = root.agentId ?? root.startedByAgentId
    const agent = agentId ? agents.get(agentId) : undefined
    const key = agentId ? `agent:${agentId}` : `root:${root.id}`
    let ring = rings.get(key)
    if (!ring) {
      ring = {
        key,
        agentId: agentId ?? null,
        rootId: agentId ? null : root.id,
        title: agentId ? (agent?.name ?? root.name) : root.name,
        assistant: agent?.assistant ?? false,
        waiting: 0,
        unseen: false,
        stories: [],
      }
      rings.set(key, ring)
    }
    ring.stories.push(story)
  }

  const list = [...rings.values()]
  for (const ring of list) {
    const waiting = ring.stories.filter((story) => story.waiting).sort(byCreated)
    const unseen = ring.stories.filter((story) => !story.waiting && !story.seen).sort(byCreated)
    const seen = ring.stories.filter((story) => !story.waiting && story.seen).sort(byCreated)
    ring.stories = [...waiting, ...unseen, ...seen]
    ring.waiting = waiting.length
    ring.unseen = ring.stories.some((story) => story.waiting || !story.seen)
  }
  const rank = (ring: StoryRing) => (ring.assistant ? 0 : ring.waiting > 0 ? 1 : ring.unseen ? 2 : 3)
  const newest = (ring: StoryRing) => Math.max(...ring.stories.map((story) => story.createdAt))
  return list.sort(
    (a, b) => rank(a) - rank(b) || newest(b) - newest(a) || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0)
  )
}

/** The chats header row (chats.json BUBBLES + STORY RING): each agent
 *  bubble wears its agent's ring (`byAgent`, keyed by agentId); `rest` = the
 *  rings no bubble wears (the root rings, an agent ring with no bubble), in
 *  ring order, painted after the bubbles in the same row. */
export function bubbleRings(
  bubbleAgentIds: readonly string[],
  rings: readonly StoryRing[]
): { byAgent: Map<string, StoryRing>; rest: StoryRing[] } {
  const ids = new Set(bubbleAgentIds)
  const byAgent = new Map<string, StoryRing>()
  const rest: StoryRing[] = []
  for (const ring of rings) {
    if (ring.agentId && ids.has(ring.agentId) && !byAgent.has(ring.agentId)) byAgent.set(ring.agentId, ring)
    else rest.push(ring)
  }
  return { byAgent, rest }
}

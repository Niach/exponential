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
export const STORY_TAB = fixture.tab

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
  /** The card's `description` (a result's one line under its body). */
  description: string | null
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

type StoryResolution = `approved` | `changed` | `answered` | `superseded`

/** One message as a story (null = not a story of a known root). `resolution`
 *  = how a plan/question was settled (null while waiting, and for results). */
function storyOf(
  message: StoryMessageInput,
  sessions: ReadonlyMap<string, StorySessionInput>,
  marks: ReadonlyMap<string, number>,
  now: number
): { story: Story; root: StorySessionInput; resolution: StoryResolution | null } | null {
  if (!message.authorSessionId || message.authorSessionId !== message.sessionId) return null
  if (!isStoryCard(message.card)) return null
  const card = message.card as Card
  const session = sessions.get(message.sessionId)
  if (!session) return null
  const root = sessions.get(session.rootId)
  if (!root) return null
  const kind: StoryKind = card.kind === `plan` ? `plan` : card.kind === `question` ? `question` : `result`
  const waiting =
    kind === `plan`
      ? card.decision == null
      : kind === `question`
        ? message.answeredAt == null && card.answer == null
        : false
  const resolution: StoryResolution | null =
    kind === `result` || waiting
      ? null
      : kind === `question`
        ? `answered`
        : card.decision === `approved`
          ? `approved`
          : card.decision === `superseded`
            ? `superseded`
            : `changed`
  const pictures = resolvedPictures(card)
  const surface = card.surface != null && typeof card.surface === `object`
  const options = Array.isArray(card.options) ? card.options : []
  const title =
    kind === `result`
      ? str(card.title)?.trim() || null
      : kind === `question`
        ? str(card.question)?.trim() || null
        : askDockTitle(str(card.plan) ?? ``) || null
  const age = storyAge(message.createdAt, now)
  const story: Story = {
    id: message.id,
    sessionId: session.id,
    rootId: root.id,
    sessionName: session.name,
    kind,
    waiting,
    // A settled plan/question was acted on: seen.
    seen: resolution !== null || message.createdAt <= (marks.get(root.id) ?? -Infinity),
    createdAt: message.createdAt,
    meta:
      resolution && resolution !== `superseded`
        ? `${fixture.copy[kind]} · ${fixture.tab.copy[resolution]} · ${age}`
        : `${fixture.copy[kind]} · ${age}`,
    title,
    description: str(card.description)?.trim() || null,
    pictures,
    surface,
    text: kind === `plan` && pictures.length === 0 && !surface ? (str(card.plan) ?? null) : null,
    footer:
      resolution !== null
        ? `none`
        : kind === `plan`
          ? `plan`
          : kind === `question`
            ? options.length > 0
              ? `choices`
              : `reply`
            : `none`,
    autoAdvanceMs: kind === `result` ? fixture.constants.autoAdvanceMs : null,
  }
  return { story, root, resolution }
}

/** The ring a root's stories ride (an agent's, else the chat's), empty. */
function emptyRing(root: StorySessionInput, agents: ReadonlyMap<string, StoryAgentInput>): StoryRing {
  const agentId = root.agentId ?? root.startedByAgentId
  const agent = agentId ? agents.get(agentId) : undefined
  return {
    key: agentId ? `agent:${agentId}` : `root:${root.id}`,
    agentId: agentId ?? null,
    rootId: agentId ? null : root.id,
    title: agentId ? (agent?.name ?? root.name) : root.name,
    assistant: agent?.assistant ?? false,
    waiting: 0,
    unseen: false,
    stories: [],
  }
}

const marksOf = (reads: readonly StoryReadInput[]) =>
  new Map(reads.map((read) => [read.sessionId, Math.max(read.readAt ?? -Infinity, read.storySeenAt ?? -Infinity)]))

/** THE fold: the viewer's story rings, in display order. */
export function stories(input: StoriesInput): StoryRing[] {
  const sessions = new Map(input.sessions.map((session) => [session.id, session]))
  const agents = new Map(input.agents.map((agent) => [agent.id, agent]))
  const marks = marksOf(input.reads)
  const rings = new Map<string, StoryRing>()

  for (const message of input.messages) {
    const built = storyOf(message, sessions, marks, input.now)
    // A settled plan/question leaves the ring (the Stories tab keeps it).
    if (!built || built.resolution !== null) continue
    const { story, root } = built
    // EXP-1461: a ring holds the last `windowHours` only, waiting or not.
    if (input.now - story.createdAt >= fixture.constants.windowHours * HOUR) continue
    const ring = emptyRing(root, agents)
    const existing = rings.get(ring.key)
    if (existing) existing.stories.push(story)
    else rings.set(ring.key, { ...ring, stories: [story] })
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

export type StoryFrameKind = `picture` | `surface` | `text` | `title`

/** One step of THE viewer (`frames`): a picture OR a surface (or a plan's
 *  text / a bare question), never both. */
export interface StoryFrame {
  /** The story's index in the ring and its id. */
  story: number
  storyId: string
  /** This frame's place among its story's frames. */
  index: number
  count: number
  kind: StoryFrameKind
  /** The picture's index in `story.pictures` (kind `picture`), else null. */
  picture: number | null
  title: string | null
  description: string | null
  footer: StoryFooter
  autoAdvanceMs: number | null
}

/** A story's frames: one per picture (pictures win: the surface stays in the
 *  chat), else the surface, else the plan text, else the bare title. */
function framesOf(story: Story, at: number): StoryFrame[] {
  const kinds: { kind: StoryFrameKind; picture: number | null }[] =
    story.pictures.length > 0
      ? story.pictures.map((_, picture) => ({ kind: `picture` as const, picture }))
      : [{ kind: story.surface ? `surface` : story.text ? `text` : `title`, picture: null }]
  return kinds.map(({ kind, picture }, index) => ({
    story: at,
    storyId: story.id,
    index,
    count: kinds.length,
    kind,
    picture,
    // The plan text already begins with the title line.
    title: kind === `text` ? null : story.title,
    description:
      kind === `picture` ? (story.pictures[picture!]!.caption?.trim() || null) : kind === `text` ? null : story.description,
    footer: index === kinds.length - 1 ? story.footer : `none`,
    autoAdvanceMs: story.autoAdvanceMs,
  }))
}

/** THE viewer's steps over a ring (stories.json `frames`): every story's
 *  frames, flattened in story order. */
export function storyFrames(ring: Pick<StoryRing, `stories`>): StoryFrame[] {
  return ring.stories.flatMap((story, at) => framesOf(story, at))
}

/** The flat frame index of a story (and one of its pictures); -1 = none. */
export function storyFrameIndex(frames: readonly StoryFrame[], storyId: string, picture = 0): number {
  const first = frames.findIndex((frame) => frame.storyId === storyId)
  if (first < 0) return -1
  const hit = frames.findIndex((frame) => frame.storyId === storyId && frame.picture === picture)
  return hit >= 0 ? hit : first
}

/** THE STORY ROW of the chats header (EXP-1461, `rowCases`): ONLY rings.
 *  `bubbles` = the agent bubbles (chats.json BUBBLES, `bubbleAgentIds` in
 *  listAgents order) whose agent has a ring, each wearing it (`byAgent`);
 *  `rest` = the rings no bubble wears (the root rings, an agent ring with no
 *  bubble), in ring order. A bubble without a ring is not drawn; both empty
 *  = no row at all. */
export function storyRow(
  bubbleAgentIds: readonly string[],
  rings: readonly StoryRing[]
): { bubbles: string[]; byAgent: Map<string, StoryRing>; rest: StoryRing[] } {
  const ids = new Set(bubbleAgentIds)
  const byAgent = new Map<string, StoryRing>()
  const rest: StoryRing[] = []
  for (const ring of rings) {
    if (ring.agentId && ids.has(ring.agentId) && !byAgent.has(ring.agentId)) byAgent.set(ring.agentId, ring)
    else rest.push(ring)
  }
  const bubbles = [...new Set(bubbleAgentIds)].filter((id) => byAgent.has(id))
  return { bubbles, byAgent, rest }
}

export interface StoryViewerSize {
  width: number
  height: number
}

/** THE wide viewer's modal size (EXP-1463, `sizeCases`): a picture frame
 *  with a known size = the picture fitted (contain) into the viewport less
 *  `viewer.inset` on each side, its width at least `viewer.minWidth` (the
 *  picture letterboxed on black inside); any other frame (or an unknown
 *  size) = `viewer.width` x `viewer.height`, each capped by the same box.
 *  Rounded to whole px. Phones never call it (full screen). */
export function storyViewerSize(
  picture: { width: number | null; height: number | null } | null,
  viewport: StoryViewerSize
): StoryViewerSize {
  const { inset, minWidth } = fixture.viewer
  const boxWidth = Math.max(0, viewport.width - 2 * inset.x)
  const boxHeight = Math.max(0, viewport.height - 2 * inset.y)
  if (!picture?.width || !picture.height) {
    return { width: Math.min(fixture.viewer.width, boxWidth), height: Math.min(fixture.viewer.height, boxHeight) }
  }
  const scale = Math.min(boxWidth / picture.width, boxHeight / picture.height)
  const width = Math.round(picture.width * scale)
  return { width: Math.min(boxWidth, Math.max(minWidth, width)), height: Math.round(picture.height * scale) }
}

/** THE Stories tab of a session screen (stories.json `tab`): every story the
 *  root's tree ever posted, never expiring (no `windowHours` cut); settled plans/questions stay
 *  (footer none, `meta` names the outcome), a superseded plan drops. Waiting
 *  first (oldest first), then the rest newest first. ONE ring (the root's,
 *  as `stories` keys it) so the viewer opens on it; null = the empty state.
 *  `unseen` = the tab chip's dot. */
export function storyTab(input: StoriesInput & { rootId: string }): StoryRing | null {
  const sessions = new Map(input.sessions.map((session) => [session.id, session]))
  const agents = new Map(input.agents.map((agent) => [agent.id, agent]))
  const root = sessions.get(input.rootId)
  if (!root || root.rootId !== root.id) return null
  const marks = marksOf(input.reads)
  const list: Story[] = []
  for (const message of input.messages) {
    const built = storyOf(message, sessions, marks, input.now)
    if (!built || built.root.id !== root.id || built.resolution === `superseded`) continue
    list.push(built.story)
  }
  if (list.length === 0) return null
  const waiting = list.filter((story) => story.waiting).sort(byCreated)
  const rest = list.filter((story) => !story.waiting).sort((a, b) => byCreated(b, a))
  return {
    ...emptyRing(root, agents),
    waiting: waiting.length,
    unseen: list.some((story) => story.waiting || !story.seen),
    stories: [...waiting, ...rest],
  }
}

/** THE styleguide specimen of the Stories tab ×4 (`tab.specimen`): the
 *  ring of `tabCases[tab.specimen.case]`. */
export const STORY_TAB_SPECIMEN = fixture.tabCases[fixture.tab.specimen.case]!.expected as unknown as StoryRing

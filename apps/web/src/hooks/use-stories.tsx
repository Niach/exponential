import { useCallback, useMemo, useRef, type ReactNode } from "react"
import { and, eq, gt, inArray, isNull, not, useLiveQuery } from "@tanstack/react-db"
import {
  STORIES_CONSTANTS,
  stories as foldStories,
  isStoryCard,
  storyTab,
  type StoriesInput,
  type Story,
  type StoryMessageInput,
  type StoryRing,
} from "@exp/domain-contract/stories"
import { StoryAvatar, type StoryAnswer, type StoryDecision } from "@exp/ui"
import { useAnswerMessage } from "@/hooks/use-answer-message"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { agentCollection, messageCollection, sessionReadsCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import {
  useConversationSessions,
  useDigestMessages,
  useSessionTitles,
  useTeamSessions,
} from "@/hooks/use-session-tree"
import type { SessionRow } from "@/lib/session-tree"

// S-3 STORIES (web, fixtures/stories.json): the host's stories state for the
// ONE active team. The model is the contract's `stories()` fold over:
//
//   sessions  the synced sessions, named by THE session title
//   agents    the synced agents (`assistant` = role 'assistant' once that
//             column exists, else false)
//   messages  the cards the web holds: every conversation's digest (its
//             WAITING card, team-wide) plus the BACKGROUND set below
//   reads     the viewer's session_reads (`readAt`, `storySeenAt`)
//
// BACKGROUND SYNC (#EXP-1380: every messages query narrows by rootId): the
// member's agent conversations and EVERY root whose tree was active within a
// result's keep window (`constants.unseenHours`) load that window's
// session-authored CARD rows (`author_session_id` + `card` set: the only
// rows a story can be) eagerly, so every story the fold keeps shows without
// its chat being opened, as on the natives (F40: no root cap); transcripts
// stay on demand.

const WINDOW_MS = STORIES_CONSTANTS.unseenHours * 3_600_000
const DAY = 86_400_000

const ms = (value: unknown): number | null => {
  if (value instanceof Date) return value.getTime()
  if (typeof value === `string` || typeof value === `number`) {
    const at = new Date(value).getTime()
    return Number.isNaN(at) ? null : at
  }
  return null
}

/** The roots whose story cards sync in the background: the member's agent
 *  conversations, plus every root whose tree (the root or any session under
 *  it) moved within the keep window. */
export function backgroundStoryRoots(
  rows: readonly Pick<SessionRow, `id` | `rootId` | `agentId` | `userId` | `createdAt` | `updatedAt`>[],
  me: string | null,
  now: number
): string[] {
  const since = now - WINDOW_MS
  const out = new Set<string>()
  for (const row of rows) {
    if (row.id === row.rootId && row.agentId && row.userId === me) out.add(row.id)
    if ((ms(row.updatedAt) ?? ms(row.createdAt) ?? 0) >= since) out.add(row.rootId)
  }
  return [...out].sort()
}

/** THE background sync: the member's agent conversations and the recently
 *  active roots, their session-authored cards of the keep window (narrowed
 *  by rootId, #EXP-1380).
 *  The team layout mounts it so a result is there before the chats page
 *  opens; `useStories` reads the same subset. */
export function useStoryBackgroundSync(teamId: string | null | undefined): unknown[] | undefined {
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const now = useNow(60_000).getTime()
  const { rows } = useTeamSessions(teamId)
  // The day's grain keeps the query stable through a session; the roots
  // move on the hour.
  const day = Math.floor(now / DAY)
  const hour = Math.floor(now / 3_600_000)
  const since = useMemo(() => new Date(day * DAY - WINDOW_MS), [day])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const roots = useMemo(() => backgroundStoryRoots(rows, me, now), [rows, me, hour])
  const rootKey = roots.join(`,`)
  const { data } = useLiveQuery(
    (query) =>
      roots.length > 0
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) =>
              and(
                inArray(messages.rootId, roots),
                gt(messages.createdAt, since),
                not(isNull(messages.authorSessionId)),
                not(isNull(messages.card))
              )
            )
        : undefined,
    [rootKey, since.getTime()]
  )
  return data
}

export interface StoriesState {
  rings: StoryRing[]
  /** A story's raw card (its surface and options). */
  cardFor: (story: Story) => unknown
  /** A ring's avatar (the agent's mark / the root's brand mark). */
  avatarFor: (ring: StoryRing) => ReactNode
  /** A story came on screen: mark its root seen up to it. */
  markShown: (story: Story) => void
  onDecide: (story: Story, decision: StoryDecision) => Promise<void>
  onAnswer: (story: Story, answer: StoryAnswer) => Promise<void>
  pending: ReadonlySet<string>
}

type LoadedCardRow = {
  id: string
  conversationId: string
  authorSessionId: string | null
  createdAt: unknown
  answeredAt: unknown
  card: unknown
}

/** Synced card rows (keyed through `conversations`) as the fold's input. */
function storyMessagesOf(
  rows: readonly LoadedCardRow[],
  conversations: ReadonlyMap<string, string>,
  into = new Map<string, StoryMessageInput & { card: unknown }>()
): Map<string, StoryMessageInput & { card: unknown }> {
  for (const raw of rows) {
    if (!isStoryCard(raw.card)) continue
    const sessionId = conversations.get(raw.conversationId)
    if (!sessionId) continue
    // A transcript row is fresher than a digest copy.
    into.set(raw.id, {
      id: raw.id,
      sessionId,
      authorSessionId: raw.authorSessionId ?? null,
      createdAt: ms(raw.createdAt) ?? 0,
      answeredAt: ms(raw.answeredAt),
      card: raw.card,
    })
  }
  return into
}

type StoryParts = Omit<StoriesState, `rings`> & { input: Omit<StoriesInput, `now`>; now: number }

/** The fold's input over `messages` (sessions named by THE session title,
 *  the team's agents, the viewer's reads) and the host's story actions. */
function useStoryParts(
  teamId: string | null | undefined,
  messages: readonly (StoryMessageInput & { card: unknown })[]
): StoryParts {
  const now = useNow(60_000).getTime()
  const { rows } = useTeamSessions(teamId)

  // ── agents ──
  const { data: agentRows } = useLiveQuery(
    (query) =>
      teamId ? query.from({ agents: agentCollection }).where(({ agents }) => eq(agents.teamId, teamId)) : undefined,
    [teamId]
  )

  // ── reads ──
  const { data: readRows } = useLiveQuery((query) => query.from({ reads: sessionReadsCollection }), [])

  // THE session title, read only for the sessions that post a story (and
  // their roots).
  const byId = useMemo(() => new Map(rows.map((row) => [row.id, row])), [rows])
  const titled = useMemo(() => {
    const ids = new Set<string>()
    for (const message of messages) {
      const session = byId.get(message.sessionId)
      if (!session) continue
      ids.add(session.id)
      ids.add(session.rootId)
    }
    return [...ids]
      .sort()
      .map((id) => byId.get(id))
      .filter((row): row is SessionRow => row !== undefined)
  }, [messages, byId])
  const { titles } = useSessionTitles(titled)

  const input = useMemo<Omit<StoriesInput, `now`>>(
    () => ({
      sessions: titled.map((row) => ({
        id: row.id,
        rootId: row.rootId,
        name: titles.get(row.id) ?? row.title ?? ``,
        agentId: row.agentId ?? null,
        startedByAgentId: row.startedByAgentId ?? null,
      })),
      agents: ((agentRows ?? []) as { id: string; name: string; role?: unknown }[]).map((agent) => ({
        id: agent.id,
        name: agent.name,
        assistant: agent.role === `assistant`,
      })),
      messages,
      reads: ((readRows ?? []) as { sessionId: string; readAt: unknown; storySeenAt?: unknown }[]).map((read) => ({
        sessionId: read.sessionId,
        readAt: ms(read.readAt),
        storySeenAt: ms(read.storySeenAt),
      })),
    }),
    [titled, titles, agentRows, messages, readRows]
  )

  const cards = useMemo(() => new Map(messages.map((message) => [message.id, message.card])), [messages])
  const cardFor = useCallback((story: Story) => cards.get(story.id) ?? null, [cards])

  const agentMarks = useMemo(
    () =>
      new Map(
        ((agentRows ?? []) as { id: string; icon: string | null; color: string | null }[]).map((agent) => [
          agent.id,
          { icon: agent.icon, color: agent.color },
        ])
      ),
    [agentRows]
  )
  const avatarFor = useCallback(
    (ring: StoryRing): ReactNode =>
      ring.agentId ? (
        <StoryAvatar agent={agentMarks.get(ring.agentId) ?? { icon: null, color: null }} />
      ) : (
        <StoryAvatar cli={(ring.rootId ? byId.get(ring.rootId)?.cli : null) ?? null} />
      ),
    [agentMarks, byId]
  )

  // Seen when SHOWN: once per story per page life; the server keeps the max.
  const marked = useRef(new Set<string>())
  const markShown = useCallback((story: Story) => {
    if (story.seen || marked.current.has(story.id)) return
    marked.current.add(story.id)
    void trpc.sessions.markStoriesSeen
      .mutate({ sessionId: story.rootId, at: story.createdAt }, SKIP_ERROR_TOAST)
      .catch(() => {
        marked.current.delete(story.id)
      })
  }, [])

  // THE card-answer path (the inline card's `sessions.answer`).
  const { answer, pending } = useAnswerMessage()
  const onDecide = useCallback(
    (story: Story, decision: StoryDecision) =>
      answer(story.id, {
        decision: decision.decision,
        ...(decision.fresh ? { fresh: true } : {}),
        ...(decision.feedback ? { feedback: decision.feedback } : {}),
        ...(decision.surfaceData ? { surfaceData: decision.surfaceData } : {}),
      }),
    [answer]
  )
  const onAnswer = useCallback(
    (story: Story, reply: StoryAnswer) =>
      answer(story.id, {
        answer: reply.answer,
        ...(reply.answers ? { answers: reply.answers } : {}),
        ...(reply.surfaceData ? { surfaceData: reply.surfaceData } : {}),
      }),
    [answer]
  )

  return { input, now, cardFor, avatarFor, markShown, onDecide, onAnswer, pending }
}

export function useStories(teamId: string | null | undefined): StoriesState {
  const { conversations } = useConversationSessions()
  const teamIds = useMemo(() => (teamId ? [teamId] : []), [teamId])
  const { messages: digest } = useDigestMessages({ teamIds })

  // ── background sync ──
  const loaded = useStoryBackgroundSync(teamId)

  const messages = useMemo(() => {
    const byId = new Map<string, StoryMessageInput & { card: unknown }>()
    for (const row of digest) {
      if (!isStoryCard(row.card)) continue
      byId.set(row.id, {
        id: row.id,
        sessionId: row.sessionId,
        authorSessionId: row.authorSessionId ?? null,
        createdAt: ms(row.createdAt) ?? 0,
        answeredAt: ms(row.answeredAt),
        card: row.card,
      })
    }
    return [...storyMessagesOf((loaded ?? []) as LoadedCardRow[], conversations, byId).values()]
  }, [digest, loaded, conversations])

  const { input, now, ...parts } = useStoryParts(teamId, messages)
  const rings = useMemo(() => foldStories({ now, ...input }), [now, input])
  return { rings, ...parts }
}

export interface StoryTabState extends Omit<StoriesState, `rings`> {
  /** `storyTab()`: the tab's ONE ring (null = the empty state). */
  ring: StoryRing | null
  /** The fold is still loading (no empty state yet). */
  loading: boolean
}

/** STORIES TAB (stories.json): every story card of the root's tree, no time
 *  window (#EXP-1380: narrowed by rootId), folded by `storyTab()`. */
export function useStoryTab(teamId: string | null | undefined, rootId: string | null): StoryTabState {
  const { conversations } = useConversationSessions()
  const { data, isLoading } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) =>
              and(eq(messages.rootId, rootId), not(isNull(messages.authorSessionId)), not(isNull(messages.card)))
            )
        : undefined,
    [rootId]
  )
  const messages = useMemo(
    () => [...storyMessagesOf((data ?? []) as LoadedCardRow[], conversations).values()],
    [data, conversations]
  )
  const { input, now, ...parts } = useStoryParts(teamId, messages)
  const ring = useMemo(() => (rootId ? storyTab({ now, ...input, rootId }) : null), [now, input, rootId])
  return { ring, loading: isLoading, ...parts }
}

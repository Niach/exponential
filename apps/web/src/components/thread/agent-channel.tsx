import { useCallback, useMemo, useState } from "react"
import {
  BoardGlyph,
  ChannelMessageRow,
  SessionCard,
  ThreadConversation,
  UserAvatar,
  userMessageCaption,
} from "@exp/ui"
import type { User } from "@/db/schema"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useNow } from "@/hooks/use-now"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { messageCollection } from "@/lib/collections"
import {
  buildSessionThread,
  toTreeMessage,
  toTreeSession,
  type MessageRow,
  type SessionRow,
  type ThreadRow,
} from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1293 Threads v2: a team AGENT's channel = a root that stays open. Its
// messages read as comment rows (avatar or the agent's icon, the name, the
// time, the body); a thread it started (`thread_started`) or a session it
// spawned (`session`) is THE session card under it, opening that thread (a
// root) or that session (a child). Question and plan cards answer in the
// card. Model = `buildSessionThread` over the agent root with the team's
// sessions as card targets.

/** Answer a question / decide a plan IN its card (`sessions.answer`); a
 *  failure toasts `Couldn't answer the question`. `pending` = the cards whose
 *  answer is in flight. */
export function useAnswerMessage(): {
  answer: (messageId: string, input: { answer?: string; decision?: `approved` | `rejected` }) => Promise<void>
  pending: ReadonlySet<string>
} {
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set())
  const answer = useCallback(
    async (messageId: string, input: { answer?: string; decision?: `approved` | `rejected` }) => {
      setPending((current) => new Set([...current, messageId]))
      try {
        const result = await trpc.sessions.answer.mutate({ messageId, ...input }, SKIP_ERROR_TOAST)
        await messageCollection.utils.awaitTxId(result.txid).catch(() => undefined)
      } catch (error) {
        failedAction(`answer`, `the question`, error)
      } finally {
        setPending((current) => {
          const next = new Set(current)
          next.delete(messageId)
          return next
        })
      }
    },
    []
  )
  return { answer, pending }
}

export function AgentChannel({
  agent,
  messages,
  teamSessions,
  users,
  canAnswer,
  onOpenSession,
}: {
  agent: SessionRow
  messages: readonly MessageRow[]
  /** Every synced session of the team (the cards' targets). */
  teamSessions: readonly SessionRow[]
  users: readonly User[]
  canAnswer: boolean
  /** A card was tapped: its session row (a root opens its thread). */
  onOpenSession: (session: SessionRow) => void
}) {
  const now = useNow(1_000)
  // The agent's own card (row 0) stays off its channel.
  const rows = useMemo(
    () =>
      buildSessionThread({
        root: toTreeSession(agent),
        sessions: teamSessions.map(toTreeSession),
        sessionIssues: [],
        issues: [],
        messages: messages.map(toTreeMessage),
        pullRequests: [],
        now: now.toISOString(),
      }).rows.slice(1),
    [agent, teamSessions, messages, now]
  )
  const messagesById = useMemo(() => new Map(messages.map((m) => [m.id, m])), [messages])
  const usersById = useMemo(() => new Map(users.map((user) => [user.id, user])), [users])
  const sessionsById = useMemo(
    () => new Map(teamSessions.map((row) => [row.id, row])),
    [teamSessions]
  )
  const { answer, pending } = useAnswerMessage()

  const agentName = agent.name ?? ``
  const agentLead = (
    <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-4" />
  )
  const time = (id: string) => {
    const message = messagesById.get(id)
    return message ? userMessageCaption({ at: new Date(message.createdAt) }) : null
  }

  const render = (row: ThreadRow) => {
    if (row.type === `message`) {
      const user = row.author === `user` && row.authorId ? usersById.get(row.authorId) : undefined
      const name = row.author === `agent` ? agentName : (user?.name ?? ``)
      return (
        <ChannelMessageRow
          key={row.id}
          lead={row.author === `agent` ? agentLead : <UserAvatar user={user} size={32} />}
          name={name}
          time={[time(row.id), row.queued ? `queued` : null].filter(Boolean).join(` · `)}
        >
          {renderThreadText(row.body)}
        </ChannelMessageRow>
      )
    }
    if (row.type === `session`) {
      const target = sessionsById.get(row.card.sessionId)
      return (
        <div key={row.id} className="pl-11">
          <SessionCard
            card={row.card}
            onOpen={target ? () => onOpenSession(target) : undefined}
          />
        </div>
      )
    }
    return (
      <div key={row.id} className="pl-11">
        <ThreadConversation
          rows={[row]}
          renderText={renderThreadText}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          onAnswer={canAnswer ? (id, value) => void answer(id, { answer: value }) : undefined}
          onDecide={canAnswer ? (id, decision) => void answer(id, { decision }) : undefined}
          pending={pending}
        />
      </div>
    )
  }

  return (
    <div data-testid="agent-channel" className="flex flex-col gap-4">
      {rows.map(render)}
    </div>
  )
}

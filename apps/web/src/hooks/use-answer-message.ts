import { useCallback, useMemo, useState } from "react"
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

// The card answer paths every conversation shares (the thread screen, its
// Dock, an app editor's chat): `sessions.answer`, `sessions.unqueue`, and a
// conversation root's rows (`buildSessionThread`) for a host that draws
// them without the whole thread model.

/** A card's answer (`sessions.answer`): a question's text + its picked
 *  labels, or a plan's decision + note + EXP-1280 answers by question id;
 *  both carry the inline surface's data model. */
export type CardAnswer = {
  answer?: string
  /** A plan: question id → labels; a question: its labels. */
  answers?: Record<string, string[]> | string[]
  decision?: `approved` | `rejected`
  /** A plan approval that starts in a fresh context. */
  fresh?: boolean
  feedback?: string
  surfaceData?: Record<string, unknown>
}

/** Answer a question / decide a plan IN its card (`sessions.answer`); a
 *  failure toasts `Couldn't answer the question` (a plan: `the plan`).
 *  `pending` = the cards whose answer is in flight. */
export function useAnswerMessage(): {
  answer: (messageId: string, input: CardAnswer) => Promise<void>
  pending: ReadonlySet<string>
} {
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set())
  const answer = useCallback(
    async (messageId: string, input: CardAnswer) => {
      setPending((current) => new Set([...current, messageId]))
      try {
        const result = await trpc.sessions.answer.mutate({ messageId, ...input }, SKIP_ERROR_TOAST)
        await messageCollection.utils.awaitTxId(result.txid).catch(() => undefined)
      } catch (error) {
        failedAction(`answer`, input.decision ? `the plan` : `the question`, error)
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

/** EXP-1317: revoke a person's own queued message (`sessions.unqueue`); a
 *  failure toasts `Couldn't remove the message`. */
export function useUnqueueMessage(): (messageId: string) => Promise<void> {
  return useCallback(async (messageId: string) => {
    try {
      const result = await trpc.sessions.unqueue.mutate({ messageId }, SKIP_ERROR_TOAST)
      await messageCollection.utils.awaitTxId(result.txid).catch(() => undefined)
    } catch (error) {
      failedAction(`remove`, `the message`, error)
    }
  }, [])
}

/** A conversation's rows: `buildSessionThread` over the conversation root
 *  (EXP-1334: no root card)
 *  and its `tabs` (#EXP-1407: the Dock's asking-session names). */
export function useConversationRows({
  conversation,
  messages,
  teamSessions,
}: {
  conversation: SessionRow | null
  messages: readonly MessageRow[]
  teamSessions: readonly SessionRow[]
}): { rows: readonly ThreadRow[]; tabs: ReturnType<typeof buildSessionThread>[`tabs`] } {
  // #EXP-1380 (chats.json PERF): the clock only drives the session cards'
  // elapsed time, so it ticks only while a card's session is live.
  const liveCard = useMemo(() => {
    const targets = new Set<string>()
    for (const message of messages) {
      const card = message.card as { kind?: unknown; sessionId?: unknown } | null
      // #EXP-1399: an escalated card (`ask`) heads with its origin's card too.
      if ((card?.kind === `session` || card?.kind === `thread_started` || card?.kind === `ask`) && typeof card.sessionId === `string`) {
        targets.add(card.sessionId)
      }
    }
    return teamSessions.some((row) => targets.has(row.id) && row.status !== `ended`)
  }, [messages, teamSessions])
  const now = useNow(liveCard ? 1_000 : 3_600_000)
  const view = useMemo(
    () =>
      conversation
        ? buildSessionThread({
            root: toTreeSession(conversation),
            sessions: teamSessions.map(toTreeSession),
            sessionIssues: [],
            issues: [],
            messages: messages.map(toTreeMessage),
            pullRequests: [],
            now: now.toISOString(),
          })
        : null,
    [conversation, teamSessions, messages, now]
  )
  return useMemo(() => ({ rows: view?.rows ?? [], tabs: view?.tabs ?? [] }), [view])
}


import { useCallback, useState } from "react"
import { answerSessionCard } from "@/lib/enet/send-sealed"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { messageCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// The card answer paths every conversation shares (the chat screen, its
// Dock, an app editor's chat): `sessions.answer`, `sessions.unqueue`, and a
// conversation root's rows (`buildSessionChat`) for a host that draws
// them without the whole chat model.

/** A card's answer (`sessions.answer`): a question's text + its picked
 *  labels, or a plan's decision + note + EXP-1280 answers by question id;
 *  both carry the inline surface's data model. */
export type CardAnswer = {
  /** #EXP-1561 (waiting-cards.json): dismissed instead of answered (alone). */
  dismissed?: true
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
  /** Resolves true once answered, false after a failure (toasted). */
  answer: (messageId: string, input: CardAnswer) => Promise<boolean>
  pending: ReadonlySet<string>
} {
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set())
  const answer = useCallback(
    async (messageId: string, input: CardAnswer) => {
      setPending((current) => new Set([...current, messageId]))
      try {
        const result = await answerSessionCard(messageId, input, SKIP_ERROR_TOAST)
        await messageCollection.utils.awaitTxId(result.txid).catch(() => undefined)
        return true
      } catch (error) {
        failedAction(`answer`, input.decision ? `the plan` : `the question`, error)
        return false
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


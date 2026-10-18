import { useCallback, useMemo, useState } from "react"
import { composerReplies, type ComposerReplies } from "@exp/domain-contract/session-tree"
import { useSessionMessages } from "@/hooks/use-session-tree"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { toTreeMessage } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1211: the quick replies above a session's composer (session-thread.json
// `repliesCases`): the session's newest `replies` card minus the sent ones.
// A tap = `sessions.sendReply` (the reply as the next message), dismiss =
// `sessions.dismissReplies`; the synced message row repaints the row.

export function useComposerReplies(sessionId: string): {
  replies: ComposerReplies | null
  pending: ReadonlySet<string>
  send: (messageId: string, itemId: string) => void
  dismiss: (messageId: string) => void
} {
  const messages = useSessionMessages(sessionId)
  const replies = useMemo(
    () => composerReplies({ sessionId, messages: messages.map(toTreeMessage) }),
    [sessionId, messages]
  )
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set())
  const send = useCallback((messageId: string, itemId: string) => {
    setPending((current) => new Set(current).add(itemId))
    trpc.sessions.sendReply
      .mutate({ messageId, itemId }, SKIP_ERROR_TOAST)
      .catch((error: unknown) => failedAction(`send`, `the reply`, error))
      .finally(() =>
        setPending((current) => {
          const next = new Set(current)
          next.delete(itemId)
          return next
        })
      )
  }, [])
  const dismiss = useCallback((messageId: string) => {
    trpc.sessions.dismissReplies
      .mutate({ messageId }, SKIP_ERROR_TOAST)
      .catch((error: unknown) => failedAction(`dismiss`, `the replies`, error))
  }, [])
  return { replies, pending, send, dismiss }
}

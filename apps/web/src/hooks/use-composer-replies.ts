import { useCallback, useMemo, useState } from "react"
import { composerReplies, type ComposerReplies } from "@exp/domain-contract/session-tree"
import { useSessionMessages } from "@/hooks/use-session-tree"
import { failedAction } from "@/lib/action-error"
import { tapCardItem } from "@/lib/enet/card-taps"
import { toTreeMessage } from "@/lib/session-tree"

// EXP-1211: the quick replies above a session's composer (session-chat.json
// `repliesCases`): the session's newest `replies` card minus the sent ones.
// A tap = `sessions.sendReply` (the reply as the next message), dismiss =
// `sessions.dismissReplies`; on a SEALED card the reply leaves sealed
// (`tapCardItem`, #EXP-1958). The synced message row repaints the row.

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
    tapCardItem({ action: `reply`, messageId, itemId })
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
    tapCardItem({ action: `dismissReplies`, messageId })
      .catch((error: unknown) => failedAction(`dismiss`, `the replies`, error))
  }, [])
  return { replies, pending, send, dismiss }
}

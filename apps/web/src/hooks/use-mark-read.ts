import { useEffect, useMemo, useRef } from "react"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { useConversationSessions, useDigestMessages } from "@/hooks/use-session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1323: opening a thread or a session marks it read
// (`sessions.markRead`, the synced `session_reads` row the Threads list's
// bold rows and the nav dot read). ONE call per open, debounced, and again
// whenever a newer agent message lands while the view stays open. A failed
// mark is background work: never a toast.

/** How long a view must stay open before it counts as read. */
const MARK_READ_DELAY_MS = 600

/** The newest agent-authored message among `sessionIds` (ISO), or null.
 *  NEW-8: the sessions' own conversations' messages, keyed through them.
 *  `undefined` = not known yet: the conversations or messages still sync,
 *  or a requested session's conversation row has not landed (a disabled
 *  live query reports ready with no rows, so it is never read as none). */
export function useNewestAgentMessageAt(sessionIds: readonly string[]): string | null | undefined {
  const { conversations, isReady: conversationsReady } = useConversationSessions()
  // Every session owns a conversation from its insert: a missing one lags.
  const synced = new Set(conversations.values())
  const awaiting = sessionIds.some((id) => !synced.has(id))
  // #EXP-1380: the digests' newest agent time (`agentAt`), no transcript.
  const { messages, isReady } = useDigestMessages({ sessionIds })
  const ready = conversationsReady && isReady && !awaiting
  return useMemo(() => {
    if (!ready) return undefined
    let newest: number | null = null
    for (const message of messages) {
      if (message.authorKind !== `agent`) continue
      const at = new Date(message.createdAt).getTime()
      if (Number.isFinite(at) && (newest === null || at > newest)) newest = at
    }
    return newest === null ? null : new Date(newest).toISOString()
  }, [messages, ready])
}

/** Mark `sessionId` read once it has been open a beat, and again when
 *  `newestAgentAt` moves on. `null` = nothing is open; `newestAgentAt`
 *  `undefined` = its messages are not known yet: wait for them. */
export function useMarkRead(
  sessionId: string | null | undefined,
  newestAgentAt: string | null | undefined
) {
  const marked = useRef<string | null>(null)
  useEffect(() => {
    if (!sessionId || newestAgentAt === undefined) return
    const mark = `${sessionId}@${newestAgentAt ?? ``}`
    if (marked.current === mark) return
    const timer = setTimeout(() => {
      marked.current = mark
      void trpc.sessions.markRead.mutate({ sessionId }, SKIP_ERROR_TOAST).catch(() => undefined)
    }, MARK_READ_DELAY_MS)
    return () => clearTimeout(timer)
  }, [sessionId, newestAgentAt])
}

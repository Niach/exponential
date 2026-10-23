import { useEffect, useMemo, useRef } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { useConversationSessions } from "@/hooks/use-session-tree"
import { messageCollection } from "@/lib/collections"
import { keyMessageRows, sessionConversationIds, type SyncedMessageRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1323: opening a thread or a session marks it read
// (`sessions.markRead`, the synced `session_reads` row the Threads list's
// bold rows and the nav dot read). ONE call per open, debounced, and again
// whenever a newer agent message lands while the view stays open. A failed
// mark is background work: never a toast.

/** How long a view must stay open before it counts as read. */
const MARK_READ_DELAY_MS = 600

/** The newest agent-authored message among `sessionIds` (ISO), or null.
 *  NEW-8: the sessions' own conversations' messages, keyed through them. */
export function useNewestAgentMessageAt(sessionIds: readonly string[]): string | null {
  const { conversations } = useConversationSessions()
  const conversationIds = sessionConversationIds(conversations, sessionIds)
  const key = conversationIds.join(`,`)
  const { data } = useLiveQuery(
    (query) =>
      conversationIds.length > 0
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) =>
              and(inArray(messages.conversationId, conversationIds), eq(messages.authorKind, `agent`))
            )
        : undefined,
    [key]
  )
  return useMemo(() => {
    let newest: number | null = null
    for (const row of keyMessageRows((data ?? []) as SyncedMessageRow[], conversations)) {
      const at = new Date(row.createdAt).getTime()
      if (Number.isFinite(at) && (newest === null || at > newest)) newest = at
    }
    return newest === null ? null : new Date(newest).toISOString()
  }, [data, conversations])
}

/** Mark `sessionId` read once it has been open a beat, and again when
 *  `newestAgentAt` moves on. `null` = nothing is open. */
export function useMarkRead(sessionId: string | null | undefined, newestAgentAt: string | null) {
  const marked = useRef<string | null>(null)
  useEffect(() => {
    if (!sessionId) return
    const mark = `${sessionId}@${newestAgentAt ?? ``}`
    if (marked.current === mark) return
    const timer = setTimeout(() => {
      marked.current = mark
      void trpc.sessions.markRead.mutate({ sessionId }, SKIP_ERROR_TOAST).catch(() => undefined)
    }, MARK_READ_DELAY_MS)
    return () => clearTimeout(timer)
  }, [sessionId, newestAgentAt])
}

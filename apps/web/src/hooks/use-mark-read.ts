import { useEffect, useMemo, useRef } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { messageCollection } from "@/lib/collections"
import type { MessageRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1323: opening a thread or a session marks it read
// (`sessions.markRead`, the synced `session_reads` row the Threads list's
// bold rows and the nav dot read). ONE call per open, debounced, and again
// whenever a newer agent message lands while the view stays open. A failed
// mark is background work: never a toast.

/** How long a view must stay open before it counts as read. */
const MARK_READ_DELAY_MS = 600

/** The newest agent-authored message among `sessionIds` (ISO), or null. */
export function useNewestAgentMessageAt(sessionIds: readonly string[]): string | null {
  const key = [...sessionIds].sort().join(`,`)
  const { data } = useLiveQuery(
    (query) =>
      sessionIds.length > 0
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) =>
              and(inArray(messages.sessionId, [...sessionIds]), eq(messages.authorKind, `agent`))
            )
            .select(({ messages }) => ({ createdAt: messages.createdAt }))
        : undefined,
    [key]
  )
  return useMemo(() => {
    let newest: number | null = null
    for (const row of (data ?? []) as Pick<MessageRow, `createdAt`>[]) {
      const at = new Date(row.createdAt).getTime()
      if (Number.isFinite(at) && (newest === null || at > newest)) newest = at
    }
    return newest === null ? null : new Date(newest).toISOString()
  }, [data])
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

import { useCallback } from "react"
import { useNavigate, useParams } from "@tanstack/react-router"
import { GlassSectionHeader, SkeletonRows } from "@exp/ui"
import { Empty } from "@/components/empty"
import { ThreadRow } from "@/components/thread/thread-row"
import type { ThreadBundle } from "@/hooks/use-threads"
import { formatOrigin, type DetailOrigin } from "@/lib/detail-origin"
import type { EmptyStateId } from "@/lib/empty-states"

// EXP-1284: lists show THREADS, not bare runs — the Agent page's Running +
// Recent (phones), Agent › Recent, Agent home's Recent. One band per
// partition (`partitionThreadList`), a `ThreadRow` per thread, opening the
// thread page with the list's origin (`?from=`, `lib/detail-origin.ts`).

/** Opens a thread's page; `teamSlug` = the thread's own team when it is not
 *  the route's (a phone's cross-team lists). */
export function useOpenThread(): (
  bundle: ThreadBundle,
  options?: { origin?: DetailOrigin | null; teamSlug?: string }
) => void {
  const navigate = useNavigate()
  const { teamSlug: routeSlug } = useParams({ strict: false }) as { teamSlug?: string }
  return useCallback(
    (bundle, options = {}) => {
      const teamSlug = options.teamSlug ?? routeSlug
      if (!teamSlug) return
      const from = formatOrigin(options.origin ?? null)
      void navigate({
        to: `/t/$teamSlug/threads/$threadId`,
        params: { teamSlug, threadId: bundle.thread.id },
        search: from ? { from } : {},
      })
    },
    [navigate, routeSlug]
  )
}

export function ThreadListSection({
  label,
  threads,
  onOpen,
  activeThreadId = null,
  loading = false,
  empty,
  ringClassName,
  testId,
}: {
  label?: string
  threads: readonly ThreadBundle[]
  onOpen: (bundle: ThreadBundle) => void
  activeThreadId?: string | null
  loading?: boolean
  /** The empty state under the band; absent = nothing. */
  empty?: EmptyStateId
  ringClassName?: string
  testId?: string
}) {
  return (
    <div data-testid={testId}>
      {label && <GlassSectionHeader label={label} />}
      {loading ? (
        // D8 first load: the row geometry (52px, mark dot).
        <SkeletonRows rowHeight={52} />
      ) : threads.length === 0 ? (
        empty ? (
          <Empty id={empty} />
        ) : null
      ) : (
        <div className="flex flex-col">
          {threads.map((bundle) => (
            <ThreadRow
              key={bundle.thread.id}
              bundle={bundle}
              active={bundle.thread.id === activeThreadId}
              ringClassName={ringClassName}
              onOpen={() => onOpen(bundle)}
            />
          ))}
        </div>
      )}
    </div>
  )
}

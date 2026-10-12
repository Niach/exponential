import { Link } from "@tanstack/react-router"
import { THREAD_COPY, ThreadGlyph } from "@exp/ui"
import { useThreadBundle } from "@/hooks/use-threads"
import type { Thread } from "@/db/schema"

// EXP-1284 (rule 7): a run's own page (deep links, pushes, the parallel
// card's rows) OFFERS its thread — a slim band over the run naming the
// thread it sits in, linking to the thread page. Absent until the run's
// thread item synced.

export function RunThreadLink({
  thread,
  teamId,
  teamSlug,
  currentUserId,
}: {
  thread: Thread | null
  teamId: string
  teamSlug: string
  currentUserId: string | undefined
}) {
  const { bundle } = useThreadBundle(teamId, currentUserId, thread?.id)
  if (!thread || !bundle) return null
  return (
    <div
      data-testid="run-thread-link"
      className="flex shrink-0 items-center gap-1.5 border-b border-border bg-card/40 px-3 py-1.5 text-xs text-muted-foreground"
    >
      <ThreadGlyph className="size-3" />
      <span className="shrink-0">In</span>
      <Link
        to="/t/$teamSlug/threads/$threadId"
        params={{ teamSlug, threadId: thread.id }}
        className="min-w-0 truncate font-medium text-foreground hover:underline"
      >
        {bundle.view.title}
      </Link>
      <span className="shrink-0">{THREAD_COPY.word}</span>
      <span className="ml-auto shrink-0 truncate">{bundle.view.headerCaption}</span>
    </div>
  )
}

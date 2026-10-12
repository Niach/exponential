import { useNavigate } from "@tanstack/react-router"
import { THREAD_COPY } from "@exp/ui"
import { useThreadsOfIssue } from "@/hooks/use-threads"
import { ThreadLead } from "@/components/thread/thread-row"
import { useSession } from "@/hooks/use-session"

// EXP-1284 (rule 6): the issue page NAMES its threads — a `Threads` row in
// the header area, one chip per thread holding the issue (its live run's
// mark while one runs, else the thread glyph, then the title), opening the
// thread page. Absent while the issue sits in none.

export function IssueThreadsRow({
  issueId,
  teamId,
  teamSlug,
  className,
}: {
  issueId: string
  teamId: string
  teamSlug: string
  className?: string
}) {
  const navigate = useNavigate()
  const { data: session } = useSession()
  const threads = useThreadsOfIssue(teamId, session?.user?.id, issueId)
  if (threads.length === 0) return null
  return (
    <div
      data-testid="issue-threads-row"
      className={`flex min-w-0 flex-wrap items-center gap-1.5 text-sm ${className ?? ``}`}
    >
      <span className="mr-1 shrink-0 text-xs text-muted-foreground">{THREAD_COPY.threads}</span>
      {threads.map((bundle) => (
        <button
          key={bundle.thread.id}
          type="button"
          data-testid={`issue-thread-chip-${bundle.thread.id}`}
          className="flex h-6 max-w-[18rem] min-w-0 items-center gap-1.5 rounded-md border border-border bg-accent/60 px-2 text-[0.8125rem] font-medium text-foreground hover:bg-accent"
          onClick={() =>
            void navigate({
              to: `/t/$teamSlug/threads/$threadId`,
              params: { teamSlug, threadId: bundle.thread.id },
            })
          }
        >
          <ThreadLead bundle={bundle} ringClassName="ring-accent" />
          <span className="min-w-0 truncate">{bundle.view.title}</span>
        </button>
      ))}
    </div>
  )
}

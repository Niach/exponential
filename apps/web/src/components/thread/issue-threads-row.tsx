import { CHATS_COPY } from "@exp/domain-contract/chats"
import { useMemo } from "react"
import { Link } from "@tanstack/react-router"
import { AgentBrandMark, AgentWorkingMark, Pill } from "@exp/ui"
import { useSessionRow, useSessionTitles, useThreadsOfIssue } from "@/hooks/use-session-tree"
import type { SessionRow } from "@/lib/session-tree"

// EXP-1284 / EXP-1293: the issue page NAMES the threads it sits in: a
// `Threads` row under the title, one chip per thread (a ROOT session linked
// through `session_issues`): the agent's mark (working while it works) and
// the thread's title (`sessionTitle`, `useSessionTitles`), opening the thread page. Absent while
// the issue sits in none.

/** Thread 58cb32df: the product says CHATS (chats.json `copy.title`). */
export const ISSUE_THREADS_LABEL = CHATS_COPY.title
/** The draft page's word before its thread chip. */
export const DRAFT_THREAD_LABEL = `In`

/** One thread as a chip: the agent's mark and the title, opening it. */
function ThreadChip({
  root,
  title,
  teamSlug,
}: {
  root: SessionRow
  title: string
  teamSlug: string
}) {
  const working = root.status !== `ended` && root.busy
  return (
    <Pill
      asChild
      mode="action"
      className="max-w-[18rem] min-w-0 text-foreground"
      leading={
        working ? (
          <AgentWorkingMark agent={root.cli} className="size-3.5" />
        ) : (
          <AgentBrandMark agent={root.cli} className="size-3.5" />
        )
      }
    >
      <Link
        to="/t/$teamSlug/threads/$threadId"
        params={{ teamSlug, threadId: root.id }}
        data-testid={`issue-thread-chip-${root.id}`}
      >
        <span className="min-w-0 truncate">{title}</span>
      </Link>
    </Pill>
  )
}

function ThreadChipRow({
  label,
  roots,
  teamSlug,
  testId,
  className,
}: {
  label: string
  roots: readonly SessionRow[]
  teamSlug: string
  testId: string
  className?: string
}) {
  // EXP-1316: no chip until every title settled (`useSessionTitles`).
  const { titles, ready } = useSessionTitles(roots)
  if (roots.length === 0 || !ready) return null
  return (
    <div
      data-testid={testId}
      className={`flex min-w-0 flex-wrap items-center gap-1.5 text-sm ${className ?? ``}`}
    >
      <span className="mr-1 shrink-0 text-xs text-muted-foreground">{label}</span>
      {roots.map((root) => (
        <ThreadChip
          key={root.id}
          root={root}
          title={titles.get(root.id) ?? ``}
          teamSlug={teamSlug}
        />
      ))}
    </div>
  )
}

/** EXP-1284: a "New issue in this thread" draft says where it files: `In
 *  <thread>` under the title. Absent for a plain draft (or a thread not
 *  synced / gone). */
export function DraftThreadRow({
  threadId,
  teamSlug,
  className,
}: {
  threadId: string | null
  /** Kept for the draft page's call; the root row carries its team. */
  teamId?: string
  teamSlug: string
  className?: string
}) {
  const { row } = useSessionRow(threadId)
  const roots = useMemo(
    () => (row && row.parentId === null ? [row] : []),
    [row]
  )
  return (
    <ThreadChipRow
      label={DRAFT_THREAD_LABEL}
      roots={roots}
      teamSlug={teamSlug}
      testId="draft-thread-row"
      className={className}
    />
  )
}

export function IssueThreadsRow({
  issueId,
  teamSlug,
  className,
}: {
  issueId: string
  /** Kept for the callers; the links carry their team. */
  teamId?: string
  teamSlug: string
  className?: string
}) {
  const roots = useThreadsOfIssue(issueId)
  // Threads only: an agent's conversation is not a thread the issue sits in.
  const threads = useMemo(
    () => roots.filter((root) => root.parentId === null && !root.agentId && !root.appId),
    [roots]
  )
  return (
    <ThreadChipRow
      label={ISSUE_THREADS_LABEL}
      roots={threads}
      teamSlug={teamSlug}
      testId="issue-threads-row"
      className={className}
    />
  )
}

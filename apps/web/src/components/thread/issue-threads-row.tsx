import { useMemo } from "react"
import { Link } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { AgentBrandMark, AgentWorkingMark, Pill } from "@exp/ui"
import {
  issueCollection,
  messageCollection,
  sessionIssueCollection,
} from "@/lib/collections"
import { titleShapesReady, useSessionRow, useThreadsOfIssue } from "@/hooks/use-session-tree"
import {
  threadTitle,
  toTreeIssue,
  toTreeMessage,
  toTreeSession,
  type MessageRow,
  type SessionIssueRow,
  type SessionRow,
  type TreeIssue,
} from "@/lib/session-tree"

// EXP-1284 / EXP-1293: the issue page NAMES the threads it sits in: a
// `Threads` row under the title, one chip per thread (a ROOT session linked
// through `session_issues`): the agent's mark (working while it works) and
// the thread's title (`threadTitle`), opening the thread page. Absent while
// the issue sits in none.

/** The rail/page title is the one place copy may say it. */
export const ISSUE_THREADS_LABEL = `Threads`
/** The draft page's word before its thread chip. */
export const DRAFT_THREAD_LABEL = `In`

/** `threadTitle` for each root: its linked issues (sort order) and its first
 *  person message, read in three live queries for the whole set. */
function useThreadTitles(roots: readonly SessionRow[]): Map<string, string> | null {
  const rootIds = roots.map((root) => root.id).sort()
  const key = rootIds.join(`,`)
  const { data: links, isReady: linksReady } = useLiveQuery(
    (query) =>
      rootIds.length > 0
        ? query
            .from({ links: sessionIssueCollection })
            .where(({ links }) => inArray(links.sessionId, rootIds))
        : undefined,
    [key]
  )
  const linkRows = (links ?? []) as SessionIssueRow[]
  const issueIds = [...new Set(linkRows.map((link) => link.issueId))].sort()
  const issueKey = issueIds.join(`,`)
  const { data: issues, isReady: issuesReady } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
            .select(({ issues }) => ({
              id: issues.id,
              identifier: issues.identifier,
              title: issues.title,
            }))
        : undefined,
    [issueKey]
  )
  const { data: messages, isReady: messagesReady } = useLiveQuery(
    (query) =>
      rootIds.length > 0
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => inArray(messages.sessionId, rootIds))
        : undefined,
    [key]
  )
  // EXP-1316: no title (and no chip) until the issue shapes are ready.
  const ready = titleShapesReady({
    linksReady,
    issueCount: issueIds.length,
    issuesReady,
    messagesReady,
  })
  return useMemo(() => {
    if (!ready) return null
    const issueById = new Map(
      ((issues ?? []) as TreeIssue[]).map((issue) => [issue.id, toTreeIssue(issue)])
    )
    const treeMessages = ((messages ?? []) as MessageRow[]).map(toTreeMessage)
    const titles = new Map<string, string>()
    for (const root of roots) {
      const rootIssues = linkRows
        .filter((link) => link.sessionId === root.id)
        .sort((a, b) => a.sortOrder - b.sortOrder)
        .map((link) => issueById.get(link.issueId))
        .filter((issue): issue is TreeIssue => issue !== undefined)
      titles.set(root.id, threadTitle(toTreeSession(root), rootIssues, treeMessages))
    }
    return titles
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [roots, links, issues, messages, ready])
}

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
  const titles = useThreadTitles(roots)
  if (roots.length === 0 || titles === null) return null
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
  // Threads only: an agent's channel is not a thread the issue sits in.
  const threads = useMemo(
    () => roots.filter((root) => root.parentId === null && root.kind !== `agent`),
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

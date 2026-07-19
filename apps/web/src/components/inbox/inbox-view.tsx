import { useMemo } from "react"
import { Link } from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import {
  Bell,
  CheckCircle2,
  CircleDot,
  GitMerge,
  GitPullRequest,
  LifeBuoy,
  MessageSquare,
  MessageSquarePlus,
  UserPlus,
} from "lucide-react"
import type { Issue, Notification, Board, Team } from "@/db/schema"
import { EmptyState } from "@/components/empty-state"
import { trpc } from "@/lib/trpc-client"
import {
  issueCollection,
  notificationCollection,
  boardCollection,
  teamCollection,
} from "@/lib/collections"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"

const typeIcon: Record<string, typeof Bell> = {
  issue_assigned: UserPlus,
  issue_created: MessageSquarePlus,
  issue_comment: MessageSquare,
  issue_mention: MessageSquare,
  issue_status_changed: CircleDot,
  pr_opened: GitPullRequest,
  pr_merged: GitMerge,
  support_reply: LifeBuoy,
}

function relativeTime(value: Date | string): string {
  const d = value instanceof Date ? value : new Date(value)
  const diff = Date.now() - d.getTime()
  const mins = Math.round(diff / 60000)
  if (mins < 1) return `just now`
  if (mins < 60) return `${mins}m`
  const hrs = Math.round(mins / 60)
  if (hrs < 24) return `${hrs}h`
  return `${Math.round(hrs / 24)}d`
}

type IssueGroup = {
  kind: `issue`
  issue: Issue
  board: Board
  teamSlug: string
  items: Notification[]
  unread: number
}

// Issue-less support_reply notifications (EXP-180: support threads are
// standalone; legacy issue-anchored rows keep flowing through the issue
// grouping above). One synthetic group PER TEAM, each linking to that
// team's Support inbox — the rows carry a synced team_id for exactly this
// (rows from before the column existed fall into one null-team group that
// links to the current team's inbox).
type SupportGroup = {
  kind: `support`
  teamId: string | null
  teamSlug: string | null
  teamName: string | null
  items: Notification[]
  unread: number
}

type Group = IssueGroup | SupportGroup

// Single Linear-style activity stream: one row per issue group, showing the
// latest notification's sentence (titles are already full human sentences —
// no composition, no actor avatar). Reviewing open PRs moved to the
// dedicated Reviews page.
export function InboxView({ teamSlug }: { teamSlug: string }) {
  // The notifications shape is scoped to the current user, NOT to a
  // team — the stream spans all the user's teams (matching the
  // user-wide sidebar unread badge and "Mark all read").
  const { data: notifications } = useLiveQuery((query) =>
    query
      .from({ n: notificationCollection })
      .orderBy(({ n }) => n.createdAt, `desc`)
  )
  const { data: issues } = useLiveQuery((query) =>
    query.from({ issues: issueCollection })
  )
  const { data: boards } = useLiveQuery((query) =>
    query.from({ boards: boardCollection })
  )
  const { data: teams } = useLiveQuery((query) =>
    query.from({ teams: teamCollection })
  )

  const issueMap = useMemo(
    () => new Map((issues ?? []).map((i) => [i.id, i as Issue])),
    [issues]
  )
  const boardMap = useMemo(
    () => new Map((boards ?? []).map((p) => [p.id, p as Board])),
    [boards]
  )
  const teamMap = useMemo(
    () => new Map((teams ?? []).map((w) => [w.id, w as Team])),
    [teams]
  )

  // Group notifications by issue (newest first, tracking unread count), plus
  // synthetic per-team Support groups for issue-less support_reply rows.
  // Each group links into its OWN team — linking with the current route's
  // slug would dead-end for issues/tickets from other teams.
  const groups = useMemo<Group[]>(() => {
    const byIssue = new Map<string, IssueGroup>()
    const supportByTeam = new Map<string | null, SupportGroup>()
    for (const n of (notifications ?? []) as Notification[]) {
      if (!n.issueId) {
        if (n.type === `support_reply`) {
          const team = n.teamId ? teamMap.get(n.teamId) : undefined
          const key = team?.id ?? null
          let g = supportByTeam.get(key)
          if (!g) {
            g = {
              kind: `support`,
              teamId: key,
              teamSlug: team?.slug ?? null,
              teamName: team?.name ?? null,
              items: [],
              unread: 0,
            }
            supportByTeam.set(key, g)
          }
          g.items.push(n)
          if (!n.readAt) g.unread += 1
        }
        continue
      }
      const issue = issueMap.get(n.issueId)
      if (!issue) continue
      const board = boardMap.get(issue.boardId)
      if (!board) continue
      const slug = teamMap.get(board.teamId)?.slug
      if (!slug) continue
      let g = byIssue.get(n.issueId)
      if (!g) {
        g = {
          kind: `issue`,
          issue,
          board,
          teamSlug: slug,
          items: [],
          unread: 0,
        }
        byIssue.set(n.issueId, g)
      }
      g.items.push(n)
      if (!n.readAt) g.unread += 1
    }
    const all: Group[] = [...byIssue.values(), ...supportByTeam.values()]
    return all.sort(
      (a, b) =>
        new Date(b.items[0].createdAt).getTime() -
        new Date(a.items[0].createdAt).getTime()
    )
  }, [notifications, issueMap, boardMap, teamMap])

  const totalUnread = groups.reduce((sum, g) => sum + g.unread, 0)

  const markGroupRead = async (items: Notification[]) => {
    await Promise.all(
      items
        .filter((n) => !n.readAt)
        .map((n) => trpc.notifications.markRead.mutate({ id: n.id }))
    )
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4">
      <div className="mb-3 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Bell className="h-4 w-4" />
          Inbox
        </h1>
        {totalUnread > 0 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void trpc.notifications.markAllRead.mutate()}
          >
            Mark all read
          </Button>
        )}
      </div>

      <div className="flex-1 space-y-2 overflow-y-auto">
        {groups.length === 0 ? (
          <EmptyState
            icon={CheckCircle2}
            title="All caught up"
            description="Assignments, comments and mentions on issues you follow will show up here."
          />
        ) : (
          groups.map((g) => {
            const latest = g.items[0]
            if (g.kind === `support`) {
              return (
                <Link
                  key={`support:${g.teamId ?? `unknown`}`}
                  to="/t/$teamSlug/support"
                  params={{ teamSlug: g.teamSlug ?? teamSlug }}
                  onClick={() => void markGroupRead(g.items)}
                  className={cn(
                    `flex items-start gap-3 rounded-md border px-3 py-2 transition-colors hover:bg-accent/50`,
                    g.unread === 0 && `opacity-60`
                  )}
                >
                  <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-muted">
                    <LifeBuoy className="h-3.5 w-3.5 text-muted-foreground" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          `truncate text-sm`,
                          g.unread > 0 && `font-medium`
                        )}
                      >
                        Support
                      </span>
                      {g.teamName != null && teamMap.size > 1 && (
                        <span className="truncate text-xs text-muted-foreground">
                          {g.teamName}
                        </span>
                      )}
                      <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                        {relativeTime(latest.createdAt)}
                      </span>
                      {g.unread > 0 && (
                        <span className="h-2 w-2 shrink-0 rounded-full bg-primary" />
                      )}
                    </div>
                    <div className="mt-0.5 truncate text-xs text-muted-foreground">
                      {latest.title}
                    </div>
                  </div>
                </Link>
              )
            }
            const Icon = typeIcon[latest.type] ?? Bell
            return (
              <Link
                key={g.issue.id}
                to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                params={{
                  teamSlug: g.teamSlug,
                  boardSlug: g.board.slug,
                  issueIdentifier: g.issue.identifier,
                }}
                onClick={() => void markGroupRead(g.items)}
                className={cn(
                  `flex items-start gap-3 rounded-md border px-3 py-2 transition-colors hover:bg-accent/50`,
                  g.unread === 0 && `opacity-60`
                )}
              >
                <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-muted">
                  <Icon className="h-3.5 w-3.5 text-muted-foreground" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="shrink-0 font-mono text-xs text-muted-foreground">
                      {g.issue.identifier}
                    </span>
                    <span
                      className={cn(
                        `truncate text-sm`,
                        g.unread > 0 && `font-medium`
                      )}
                    >
                      {g.issue.title}
                    </span>
                    <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                      {relativeTime(latest.createdAt)}
                    </span>
                    {g.unread > 0 && (
                      <span className="h-2 w-2 shrink-0 rounded-full bg-primary" />
                    )}
                  </div>
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">
                    {latest.title}
                  </div>
                </div>
              </Link>
            )
          })
        )}
      </div>
    </div>
  )
}

import { useMemo, useState, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import {
  Bell,
  CheckCircle2,
  CircleDot,
  GitPullRequest,
  MessageSquare,
  Sparkles,
  UserPlus,
} from "lucide-react"
import type { Issue, Notification, Project } from "@/db/schema"
import { EmptyState } from "@/components/empty-state"
import { trpc } from "@/lib/trpc-client"
import {
  issueCollection,
  notificationCollection,
  projectCollection,
} from "@/lib/collections"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

type InboxTab = `for_me` | `needs_review`

const typeIcon: Record<string, typeof Bell> = {
  issue_assigned: UserPlus,
  issue_comment: MessageSquare,
  issue_mention: MessageSquare,
  issue_status_changed: CircleDot,
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

export function InboxView({
  workspaceSlug,
  workspaceId,
}: {
  workspaceSlug: string
  workspaceId: string | undefined
}) {
  const [tab, setTab] = useState<InboxTab>(`for_me`)

  // The notifications shape is already scoped to the current user.
  const { data: notifications } = useLiveQuery((query) =>
    query
      .from({ n: notificationCollection })
      .orderBy(({ n }) => n.createdAt, `desc`)
  )
  const { data: issues } = useLiveQuery((query) =>
    query.from({ issues: issueCollection })
  )
  const { data: projects } = useLiveQuery((query) =>
    query.from({ projects: projectCollection })
  )

  const issueMap = useMemo(
    () => new Map((issues ?? []).map((i) => [i.id, i as Issue])),
    [issues]
  )
  const projectMap = useMemo(
    () => new Map((projects ?? []).map((p) => [p.id, p as Project])),
    [projects]
  )

  // Group notifications by issue, newest first, tracking unread count.
  const groups = useMemo(() => {
    const byIssue = new Map<
      string,
      { issue: Issue; project: Project; items: Notification[]; unread: number }
    >()
    for (const n of (notifications ?? []) as Notification[]) {
      if (!n.issueId) continue
      const issue = issueMap.get(n.issueId)
      if (!issue) continue
      const project = projectMap.get(issue.projectId)
      if (!project) continue
      let g = byIssue.get(n.issueId)
      if (!g) {
        g = { issue, project, items: [], unread: 0 }
        byIssue.set(n.issueId, g)
      }
      g.items.push(n)
      if (!n.readAt) g.unread += 1
    }
    return [...byIssue.values()].sort(
      (a, b) =>
        new Date(b.items[0].createdAt).getTime() -
        new Date(a.items[0].createdAt).getTime()
    )
  }, [notifications, issueMap, projectMap])

  // "Needs your review": issues in this workspace with a plan awaiting approval
  // or an open PR (pr_state populated by the agent). Independent of the
  // notification feed.
  const reviewIssues = useMemo(() => {
    return (issues ?? [])
      .filter((i) => {
        const project = projectMap.get((i as Issue).projectId)
        if (!project || project.workspaceId !== workspaceId) return false
        return (
          (i as Issue).agentPlanState === `awaiting_approval` ||
          (i as Issue).prState === `open`
        )
      })
      .map((i) => ({ issue: i as Issue, project: projectMap.get((i as Issue).projectId)! }))
  }, [issues, projectMap, workspaceId])

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

      <div className="flex gap-1 rounded-md bg-muted/50 p-0.5 text-sm">
        <TabButton active={tab === `for_me`} onClick={() => setTab(`for_me`)}>
          For me{totalUnread > 0 ? ` · ${totalUnread}` : ``}
        </TabButton>
        <TabButton
          active={tab === `needs_review`}
          onClick={() => setTab(`needs_review`)}
        >
          Needs your review
          {reviewIssues.length > 0 ? ` · ${reviewIssues.length}` : ``}
        </TabButton>
      </div>

      <div className="mt-3 flex-1 space-y-2 overflow-y-auto">
        {tab === `for_me` ? (
          groups.length === 0 ? (
            <EmptyState
              icon={CheckCircle2}
              title="All caught up"
              description="Assignments, comments and mentions on issues you follow will show up here."
            />
          ) : (
            groups.map((g) => (
              <Link
                key={g.issue.id}
                to="/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier"
                params={{
                  workspaceSlug,
                  projectSlug: g.project.slug,
                  issueIdentifier: g.issue.identifier,
                }}
                onClick={() => void markGroupRead(g.items)}
                className={cn(
                  `block rounded-md border px-3 py-2 transition-colors hover:bg-accent/50`,
                  g.unread > 0 && `border-l-2 border-l-primary`
                )}
              >
                <div className="flex items-center gap-2">
                  {g.unread > 0 && (
                    <span className="h-2 w-2 shrink-0 rounded-full bg-primary" />
                  )}
                  <span className="font-mono text-xs text-muted-foreground">
                    {g.issue.identifier}
                  </span>
                  <span className="truncate text-sm font-medium">
                    {g.issue.title}
                  </span>
                  <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                    {relativeTime(g.items[0].createdAt)}
                  </span>
                </div>
                <div className="mt-1 space-y-0.5 pl-4">
                  {g.items.slice(0, 3).map((n) => {
                    const Icon = typeIcon[n.type] ?? Bell
                    return (
                      <div
                        key={n.id}
                        className="flex items-center gap-2 text-xs text-muted-foreground"
                      >
                        <Icon className="h-3 w-3 shrink-0" />
                        <span className="truncate">{n.title}</span>
                      </div>
                    )
                  })}
                  {g.items.length > 3 && (
                    <div className="text-xs text-muted-foreground/70">
                      +{g.items.length - 3} more
                    </div>
                  )}
                </div>
              </Link>
            ))
          )
        ) : reviewIssues.length === 0 ? (
          <Empty label="Nothing waiting on your review." />
        ) : (
          reviewIssues.map(({ issue, project }) => (
            <Link
              key={issue.id}
              to="/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier"
              params={{
                workspaceSlug,
                projectSlug: project.slug,
                issueIdentifier: issue.identifier,
              }}
              className="block rounded-md border px-3 py-2 transition-colors hover:bg-accent/50"
            >
              <div className="flex items-center gap-2">
                <span className="font-mono text-xs text-muted-foreground">
                  {issue.identifier}
                </span>
                <span className="truncate text-sm font-medium">
                  {issue.title}
                </span>
                <Badge variant="secondary" className="ml-auto shrink-0 gap-1">
                  {issue.agentPlanState === `awaiting_approval` ? (
                    <>
                      <Sparkles className="h-3 w-3" /> Plan
                    </>
                  ) : (
                    <>
                      <GitPullRequest className="h-3 w-3" /> PR
                    </>
                  )}
                </Badge>
              </div>
            </Link>
          ))
        )}
      </div>
    </div>
  )
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        `flex-1 rounded px-3 py-1 font-medium transition-colors`,
        active
          ? `bg-background text-foreground shadow-sm`
          : `text-muted-foreground hover:text-foreground`
      )}
    >
      {children}
    </button>
  )
}

function Empty({ label }: { label: string }) {
  return (
    <div className="rounded-md border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
      {label}
    </div>
  )
}

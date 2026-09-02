import { useMemo, useState } from "react"
import { Link } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { Bell, CircleCheck } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { NotificationType } from "@exp/db-schema/domain"
import { notificationTypeValues } from "@exp/db-schema/domain"
import { conceptIcon } from "@/lib/icons.generated"

const SupportIcon = conceptIcon(`nav-support`)
import type { Issue, Notification, Board, Team } from "@/db/schema"
import { EmptyState } from "@/components/empty-state"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { trpc } from "@/lib/trpc-client"
import {
  issueCollection,
  notificationCollection,
  boardCollection,
  teamCollection,
} from "@/lib/collections"
import { GlassRow } from "@/components/ui/glass-rows"
import { cn } from "@/lib/utils"

// EXP-273: derived from the shared registry rather than hand-listed, so the
// inbox can't drift from the other three clients (it had: `issue_mention`
// drawing the same glyph as `issue_comment`, and `issue_created` on a
// comment-plus mark). `notification_type` values map 1:1 onto
// `notification-<kebab>` concepts, so a new enum value fails the build here
// until the registry gains its concept.
const typeIcon = Object.fromEntries(
  notificationTypeValues.map((type) => [
    type,
    conceptIcon(`notification-${type.replace(/_/g, `-`)}` as never),
  ])
) as Record<NotificationType, typeof Bell>

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

// REV-46: the notifications shape syncs every delivered row, so a long-lived
// account can group into thousands of rows — cap + expand like the board's
// issue list instead of mounting them all.
const GROUP_CAP = 100
const GROUP_CHUNK = 200

// Single Linear-style activity stream: one row per issue group, showing the
// latest notification's sentence (titles are already full human sentences —
// no composition, no actor avatar). Reviewing open PRs moved to the
// dedicated Reviews page. Rendered as the "Inbox" tab of the Inbox page
// (EXP-186), whose header owns the tab switcher + "Mark all read".
export function InboxView({ teamSlug }: { teamSlug: string }) {
  // The notifications shape is scoped to the current user, NOT to a
  // team — the stream spans all the user's teams (matching the
  // user-wide sidebar unread badge and "Mark all read").
  const { data: notifications } = useLiveQuery((query) =>
    query
      .from({ n: notificationCollection })
      .orderBy(({ n }) => n.createdAt, `desc`)
      // Equal timestamps must not reorder between syncs (EXP-668).
      .orderBy(({ n }) => n.id)
  )
  // Only the issues the notifications actually reference — subscribing to the
  // whole collection re-fired the grouping memo on every issue change in any
  // of the user's teams (REV-48).
  const notifiedIssueIds = useMemo(() => {
    const ids = new Set<string>()
    for (const n of (notifications ?? []) as Notification[]) {
      if (n.issueId) ids.add(n.issueId)
    }
    return [...ids].sort()
  }, [notifications])
  const { data: issues } = useLiveQuery(
    (query) =>
      notifiedIssueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, notifiedIssueIds))
        : undefined,
    [notifiedIssueIds.join(`,`)]
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

  const [visibleCount, setVisibleCount] = useState(GROUP_CAP)
  const visibleGroups =
    groups.length > visibleCount ? groups.slice(0, visibleCount) : groups
  const hiddenCount = groups.length - visibleGroups.length

  // One mutation per group, not one per row (REV-48): the server-side
  // by-issue/by-team clears also catch rows the client hasn't synced yet.
  // Only the legacy null-team support group (rows from before team_id
  // existed) still clears row-by-row — markReadSupport needs a team.
  const markGroupRead = async (g: Group) => {
    if (g.unread === 0) return
    if (g.kind === `issue`) {
      await trpc.notifications.markReadByIssue.mutate({ issueId: g.issue.id })
      return
    }
    if (g.teamId) {
      await trpc.notifications.markReadSupport.mutate({ teamId: g.teamId })
      return
    }
    await Promise.all(
      g.items
        .filter((n) => !n.readAt)
        .map((n) => trpc.notifications.markRead.mutate({ id: n.id }))
    )
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4">
      <div
        className={`flex flex-1 flex-col gap-2 overflow-y-auto ${TAB_BAR_CLEARANCE}`}
      >
        {groups.length === 0 ? (
          <EmptyState
            icon={CircleCheck}
            title="All caught up"
            description="Assignments, comments and mentions on issues you follow will show up here."
          />
        ) : (
          visibleGroups.map((g) => {
            const latest = g.items[0]
            if (g.kind === `support`) {
              return (
                <GlassRow
                  key={`support:${g.teamId ?? `unknown`}`}
                  asChild
                  interactive
                  className={cn(
                    `items-start px-3 py-2`,
                    g.unread === 0 && `opacity-60`
                  )}
                >
                  <Link
                    to="/t/$teamSlug/support"
                    params={{ teamSlug: g.teamSlug ?? teamSlug }}
                    onClick={() => void markGroupRead(g)}
                  >
                    <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-muted">
                      <SupportIcon className="h-3.5 w-3.5 text-muted-foreground" />
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
                </GlassRow>
              )
            }
            const Icon = typeIcon[latest.type] ?? Bell
            return (
              <GlassRow
                key={g.issue.id}
                asChild
                interactive
                className={cn(
                  `items-start px-3 py-2`,
                  g.unread === 0 && `opacity-60`
                )}
              >
                <Link
                  to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                  params={{
                    teamSlug: g.teamSlug,
                    boardSlug: g.board.slug,
                    issueIdentifier: g.issue.identifier,
                  }}
                  onClick={() => void markGroupRead(g)}
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
              </GlassRow>
            )
          })
        )}
        {hiddenCount > 0 && (
          <Button
            variant="ghost"
            size="sm"
            className="w-full text-xs text-muted-foreground"
            onClick={() => setVisibleCount((c) => c + GROUP_CHUNK)}
          >
            Show {Math.min(GROUP_CHUNK, hiddenCount)} more ({hiddenCount}{` `}
            hidden)
          </Button>
        )}
      </div>
    </div>
  )
}

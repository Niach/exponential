import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Board, Issue, IssueLabel, Team } from "@/db/schema"
import { IssueDetailView } from "@/components/issue-detail-view"
import {
  boardCollection,
  issueCollection,
  issueLabelCollection,
  teamCollection,
} from "@/lib/collections"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// EXP-827: the RIGHT pane of the desktop inbox split view — the selected
// notification's issue, rendered by the same `IssueDetailView` the issue
// route mounts. The inbox spans every team the caller is in, so the board,
// team, roster and permissions all resolve from the ISSUE (never from the
// route's team slug).

export function InboxIssuePane({ issueId }: { issueId: string }) {
  const { data: issues, isReady } = useLiveQuery(
    (query) =>
      query
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.id, issueId)),
    [issueId]
  )
  const issue = ((issues ?? []) as Issue[])[0] ?? null

  const { data: boards } = useLiveQuery(
    (query) =>
      issue
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.id, issue.boardId))
        : undefined,
    [issue?.boardId]
  )
  const board = ((boards ?? []) as Board[])[0] ?? null

  const { data: teams } = useLiveQuery(
    (query) =>
      board
        ? query
            .from({ teams: teamCollection })
            .where(({ teams }) => eq(teams.id, board.teamId))
        : undefined,
    [board?.teamId]
  )
  const team = ((teams ?? []) as Team[])[0] ?? null

  const { data: issueLabels } = useLiveQuery(
    (query) =>
      query
        .from({ issueLabels: issueLabelCollection })
        .where(({ issueLabels }) => eq(issueLabels.issueId, issueId)),
    [issueId]
  )
  const issueLabelIds = ((issueLabels ?? []) as IssueLabel[]).map(
    (label) => label.labelId
  )

  const { users } = useTeamUsers(team?.id)
  const permissions = useTeamPermissions(team ?? undefined)

  if (!issue || !board || !team) {
    return (
      <div className="p-6 text-sm text-muted-foreground">
        {isReady && !issue ? `This issue is no longer available.` : `Loading…`}
      </div>
    )
  }

  return (
    <IssueDetailView
      issue={issue}
      issueLabelIds={issueLabelIds}
      users={users}
      board={board}
      teamSlug={team.slug}
      teamId={team.id}
      readOnly={!permissions.canMutateIssue(issue)}
    />
  )
}

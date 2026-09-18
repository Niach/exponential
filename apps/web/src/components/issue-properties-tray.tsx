import type { Board, Issue, User } from "@/db/schema"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { mergeTargetProps } from "@/hooks/use-agents-data"
import type { IssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import { useSteerConfig } from "@/components/agent-session"
import { useIsTeamMember } from "@/components/issue-coding-rows"
import { IssueCodingAction } from "@/components/issue-coding-action"
import { IssuePropertiesPanel } from "@/components/issue-properties-panel"
import { MergePrPill } from "@/components/run-action-pills"
import { WORK_COLUMN_CLASS } from "@/components/work-header"

// EXP-877: row 2 of the unified work header — the issue's properties tray
// (status, priority, assignee, labels, due, board, origin) with the right
// cluster `[Merge PR when the PR is open] [the ONE coding action]` inside it.
// Rendered by the issue route AND by an issue-bound run's session route, so
// the tray never changes between the two faces. The phone renders the same
// node at the top of its scroll column (EXP-568) with the coding action left
// out — its floating bar's circle owns the start there.
export function IssuePropertiesTray({
  issue,
  board,
  users,
  teamId,
  currentUserId,
  readOnly = false,
  handlers,
  showCodingAction = true,
  preferredSessionId,
}: {
  issue: Issue
  board: Board
  users: User[]
  teamId: string
  currentUserId: string | null
  readOnly?: boolean
  handlers: IssuePropertyHandlers
  /** The phone keeps its start in the floating bar, not here. */
  showCodingAction?: boolean
  /** The run this header is bound to (the session route) — Stop/Resume act
   *  on it when it qualifies. */
  preferredSessionId?: string
}) {
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const statusOption = resolveStatus(issue)
  // EXP-760: Merge sits INSIDE the tray, so this owns the two signals
  // SessionMergeButton needs — membership (the recovery run's launcher) and
  // whether the relay is configured at all. The pill self-hides unless the
  // linked PR is open.
  const isMember = useIsTeamMember(teamId, currentUserId ?? ``)
  const steerConfig = useSteerConfig()
  const prOpen = issue.prState === `open`
  const dueDate = issue.dueDate ?? null

  const mergeButton =
    currentUserId && isMember && prOpen ? (
      <MergePrPill
        {...mergeTargetProps({ kind: `issue`, issue })}
        steerEnabled={steerConfig?.enabled === true}
      />
    ) : null

  const codingAction =
    currentUserId && showCodingAction ? (
      <IssueCodingAction
        issue={issue}
        board={board}
        teamId={teamId}
        currentUserId={currentUserId}
        preferredSessionId={preferredSessionId}
      />
    ) : null

  return (
    <div className={`${WORK_COLUMN_CLASS} px-4 pt-3`}>
      {/* The IDE's `glass_tray`: the section fill inside the card hairline
          (`bg-popover/40` vanished against the panel). */}
      <div className="flex items-center gap-1.5 rounded-xl border border-glass-stroke-card bg-glass-section">
        <div className="min-w-0 flex-1">
          <IssuePropertiesPanel
            status={statusOption}
            onStatusChange={handlers.handleStatusChange}
            priority={issue.priority}
            onPriorityChange={handlers.handlePriorityChange}
            assigneeId={issue.assigneeId}
            onAssigneeChange={handlers.handleAssigneeChange}
            users={users}
            teamId={teamId}
            selectedLabelIds={handlers.issueLabelIds}
            onToggleLabel={handlers.handleToggleLabel}
            dueDate={dueDate}
            onDueDateSelect={handlers.handleDueDateSelect}
            source={issue.source}
            boardColor={board.color}
            boardPrefix={board.prefix}
            boardIcon={board.icon}
            boardRepositoryId={board.repositoryId}
            boardId={issue.boardId}
            issueIdentifier={issue.identifier}
            onBoardChange={handlers.handleBoardChange}
            disabled={readOnly}
          />
        </div>
        {/* min-w-0, not shrink-0: the "No desktop online" caption beside the
            capsule truncates rather than squeezing the property pills. */}
        {(mergeButton || codingAction) && (
          <div className="flex min-w-0 items-center gap-1.5 pr-3">
            {mergeButton}
            {codingAction}
          </div>
        )}
      </div>
    </div>
  )
}

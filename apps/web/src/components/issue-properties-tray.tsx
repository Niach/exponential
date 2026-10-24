import type { ReactNode } from "react"
import type { Board, Issue, User } from "@/db/schema"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { useTeamById } from "@/hooks/use-team-data"
import type { IssueEstimation } from "@/lib/domain"
import type { IssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import { IssuePropertiesPanel } from "@/components/issue-properties-panel"
import { WORK_COLUMN_CLASS } from "@exp/ui"

// EXP-877: row 2 of the issue's work header: the properties tray (status,
// priority, assignee, labels, due, board, origin). EXP-1293: properties
// only; Start lives in the header's trailing slot (the phone: its bar's
// circle) and nothing here merges or stops. The phone renders the same node
// at the top of its scroll column (EXP-568).
export function IssuePropertiesTray({
  issue,
  board,
  users,
  teamId,
  readOnly = false,
  handlers,
}: {
  issue: Issue
  board: Board
  users: User[]
  teamId: string
  readOnly?: boolean
  handlers: IssuePropertyHandlers
}) {
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const statusOption = resolveStatus(issue)
  const team = useTeamById(teamId)
  const dueDate = issue.dueDate ?? null

  return (
    <PropertiesTrayCard>
      <div className="min-w-0 flex-1">
        <IssuePropertiesPanel
          className="md:px-0"
          status={statusOption}
          onStatusChange={handlers.handleStatusChange}
          priority={issue.priority}
          onPriorityChange={handlers.handlePriorityChange}
          assigneeId={issue.assigneeId}
          assigneeAgentId={issue.assigneeAgentId}
          onAssigneeChange={handlers.handleAssigneeChange}
          users={users}
          teamId={teamId}
          selectedLabelIds={handlers.issueLabelIds}
          onToggleLabel={handlers.handleToggleLabel}
          dueDate={dueDate}
          onDueDateSelect={handlers.handleDueDateSelect}
          estimate={issue.estimate ?? null}
          estimation={(team?.estimationType as IssueEstimation | undefined) ?? `none`}
          onEstimateChange={handlers.handleEstimateChange}
          source={issue.source}
          boardColor={board.color}
          boardPrefix={board.prefix}
          boardName={board.name}
          boardIcon={board.icon}
          boardRepositoryId={board.repositoryId}
          boardId={issue.boardId}
          issueIdentifier={issue.identifier}
          onBoardChange={handlers.handleBoardChange}
          disabled={readOnly}
        />
      </div>
    </PropertiesTrayCard>
  )
}

/** EXP-1170: the tray's card — the issue detail's and the New issue page's
 *  properties row sit in the very same chrome. EXP-1191: a card on phones
 *  only; md+ (like the IDE) sets the properties straight on the page, under
 *  the title. */
export function PropertiesTrayCard({ children }: { children: ReactNode }) {
  return (
    <div className={`${WORK_COLUMN_CLASS} px-4 pt-3 md:px-5`}>
      {/* The IDE's `glass_tray`: the section fill inside the card hairline
          (`bg-popover/40` vanished against the panel). */}
      <div className="flex items-center gap-1.5 max-md:rounded-xl max-md:border max-md:border-glass-stroke-card max-md:bg-glass-section">
        {children}
      </div>
    </div>
  )
}

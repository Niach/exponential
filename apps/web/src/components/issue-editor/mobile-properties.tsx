import type { User } from "@/db/schema"
import type { IssuePriority, IssueEstimation } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  creatableStatusOptions,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { useTeamBoards } from "@/hooks/use-team-data"
import { displayUserName } from "@/lib/user-display"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { BoardPicker } from "@/components/issue-properties/board-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import { MobileRelationBands } from "@/components/issue-relations-card"
import {
  getPriorityConfig,
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import {
  toStatusMenuOption,
  toStatusPickerStatuses,
} from "@/components/issue-properties/status-dropdown"
import {
  Picker,
  PriorityPicker,
  StatusPicker,
  conceptIcon,
  DatePicker,
  GlassGroup,
  PropertyRow,
  UserAvatar,
  BoardGlyph,
} from "@exp/ui"
import {
  estimateLabel,
  parseEstimatePick,
  estimatePickerValues,
  NO_ESTIMATE,
} from "@/lib/issue-estimate"

const DueDateGlyph = conceptIcon(`ui-due-date`)
const EstimateGlyph = conceptIcon(`ui-estimate`)
const LabelsGlyph = conceptIcon(`settings-labels`)
const UnassignedGlyph = conceptIcon(`ui-unassigned`)

export interface IssueEditorMobilePropertiesProps {
  status: StatusRowOption
  priority: IssuePriority
  assigneeId: string | null
  selectedLabelIds: string[]
  teamId: string
  users: User[]
  /** `YYYY-MM-DD`, or null for "no due date" (REV2-49: no time of day). */
  dueDate: string | null
  hideAssignee?: boolean
  hideDueDateChip?: boolean
  /** EXP-630 (the issue detail's phone sheet only): an Estimate row after Due
   * date. Absent on the create form. */
  estimate?: number | null
  estimation?: IssueEstimation
  onEstimateChange?: (estimate: number | null) => void | Promise<void>
  /** EXP-698 r5 (the issue detail's phone sheet only): a Board row after Due
   * date, moving the issue through the same confirm dialog the desktop chip
   * uses. Absent on the create form — a new issue is already ON its board. */
  board?: {
    boardId: string
    teamId: string
    issueIdentifier?: string | null
    onBoardChange: (boardId: string) => void | Promise<void>
  }
  /** EXP-736 (the issue detail's phone sheet only): a Relations section after
   * Labels — the same rows and the same "Add relation" picker the desktop card
   * shows, minus its own card chrome. Absent on the create form: a new issue
   * has no id to relate yet. */
  relations?: {
    issueId: string
    readOnly?: boolean
  }
  disableStatus?: boolean
  disabled?: boolean
  onStatusChange: (status: StatusRowOption) => void | Promise<void>
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onToggleLabel: (labelId: string) => void | Promise<void>
  onDueDateSelect: (date: string | null) => void | Promise<void>
}

export function IssueEditorMobileProperties({
  status,
  priority,
  assigneeId,
  selectedLabelIds,
  teamId,
  users,
  dueDate,
  hideAssignee,
  hideDueDateChip,
  estimate,
  estimation,
  onEstimateChange,
  board,
  relations,
  disableStatus,
  disabled,
  onStatusChange,
  onPriorityChange,
  onAssigneeChange,
  onToggleLabel,
  onDueDateSelect,
}: IssueEditorMobilePropertiesProps) {
  const { options, byId } = useTeamStatusesContext()
  // Only queried when the Board row is asked for (`undefined` skips it).
  const boardOptions = useTeamBoards(board?.teamId)
  const currentBoard = board
    ? boardOptions.find((row) => row.id === board.boardId)
    : undefined
  const statusOptions = creatableStatusOptions(options)
  const assignee = assigneeId
    ? users.find((user) => user.id === assigneeId)
    : undefined

  // EXP-958: both rows draw the RESOLVED property this form holds, never the
  // picker's matched option — a duplicate-status issue is not in
  // `creatableStatusOptions` and an unknown priority is not in the table, so
  // a match would be empty where the resolver already falls back (REV2-85).
  const statusTrigger = toStatusMenuOption(status)
  const StatusTriggerIcon = statusTrigger.icon
  const priorityTrigger = getPriorityConfig(priority)

  return (
    <div className="mx-3 my-3 flex flex-col gap-4">
      {/* EXP-994: the divided-rows shell, not a hand-divided card. */}
      <GlassGroup>
        <StatusPicker
          statuses={toStatusPickerStatuses(statusOptions)}
          value={status.id}
          disabled={disabled || disableStatus}
          width="sm"
          onChange={(id) => {
            const picked = byId.get(id)
            if (picked) void onStatusChange(picked)
          }}
          mobileTitle="Status"
          trigger={
            <PropertyRow
              label="Status"
              disabled={disabled || disableStatus}
              value={
                <>
                  <StatusTriggerIcon
                    className={`!h-3.5 !w-3.5 ${statusTrigger.color}`}
                    style={
                      statusTrigger.colorHex
                        ? { color: statusTrigger.colorHex }
                        : undefined
                    }
                  />
                  {statusTrigger.label}
                </>
              }
            />
          }
        />

        <PriorityPicker
          options={priorities}
          value={priority}
          disabled={disabled}
          width="sm"
          onChange={(next) => void onPriorityChange(next as IssuePriority)}
          mobileTitle="Priority"
          trigger={
            <PropertyRow
              label="Priority"
              disabled={disabled}
              value={
                <>
                  <PriorityIcon
                    priority={priorityTrigger.value}
                    className="!h-3.5 !w-3.5"
                  />
                  {priorityTrigger.label}
                </>
              }
            />
          }
        />

        {!hideAssignee && (
          <AssigneePicker
            disabled={disabled}
            users={users}
            selectedUserId={assigneeId}
            onSelect={onAssigneeChange}
            trigger={
              <PropertyRow
                label="Assignee"
                disabled={disabled}
                value={
                  assignee ? (
                    <>
                      <UserAvatar
                        size={16}
                        user={{
                          id: assignee.id,
                          name: displayUserName(assignee, assignee.id),
                          image: assignee.image,
                        }}
                      />
                      <span className="max-w-[8rem] truncate">
                        {displayUserName(assignee, assignee.id)}
                      </span>
                    </>
                  ) : (
                    <>
                      <UnassignedGlyph className="size-3.5" />
                      Unassigned
                    </>
                  )
                }
              />
            }
          />
        )}

        {/* EXP-1170: Labels is a ROW like every other property (×3 phones):
            the picks joined in the team's sort order, tapping opens the
            shared picker sheet. No cloud of toggle chips. */}
        <LabelPicker
          disabled={disabled}
          teamId={teamId}
          selectedLabelIds={selectedLabelIds}
          onToggle={onToggleLabel}
          renderTrigger={(selected) => (
            <PropertyRow
              label="Labels"
              disabled={disabled}
              value={
                <>
                  <LabelsGlyph className="size-3.5" />
                  <span className="max-w-[8rem] truncate">
                    {selected.length > 0
                      ? selected.map((label) => label.name).join(`, `)
                      : `None`}
                  </span>
                </>
              }
            />
          )}
        />

        {!hideDueDateChip && (
          <DatePicker
            value={dueDate}
            onChange={(date) => {
              void onDueDateSelect(date)
            }}
            disabled={disabled}
            mobileTitle="Due date"
            placeholder="No date"
            renderTrigger={({ label }) => (
              <PropertyRow
                label="Due date"
                disabled={disabled}
                value={
                  <>
                    <DueDateGlyph className="size-3.5" />
                    {label}
                  </>
                }
              />
            )}
          />
        )}

        {onEstimateChange && estimation && estimation !== `none` && (
          <Picker
            mode="single"
            value={estimate == null ? null : String(estimate)}
            disabled={disabled}
            items={estimatePickerValues(estimate ?? null, estimation).map(
              (value) => ({
                value: String(value),
                label: estimateLabel(value, estimation),
              })
            )}
            noneLabel={NO_ESTIMATE}
            onNone={() => void onEstimateChange(null)}
            onChange={(next) => void onEstimateChange(parseEstimatePick(next))}
            mobileTitle="Estimate"
            trigger={() => (
              <PropertyRow
                label="Estimate"
                disabled={disabled}
                value={
                  <>
                    <EstimateGlyph className="size-3.5" />
                    {estimateLabel(estimate, estimation)}
                  </>
                }
              />
            )}
          />
        )}

        {board && (
          <BoardPicker
            disabled={disabled}
            teamId={board.teamId}
            selectedBoardId={board.boardId}
            issueIdentifier={board.issueIdentifier}
            onSelect={board.onBoardChange}
            trigger={
              <PropertyRow
                label="Board"
                disabled={disabled}
                value={
                  <>
                    <BoardGlyph
                      board={currentBoard ?? { color: `#71717a` }}
                      className="!h-3.5 !w-3.5"
                    />
                    <span className="max-w-[8rem] truncate">
                      {currentBoard?.name ?? `Board`}
                    </span>
                  </>
                }
              />
            }
          />
        )}
      </GlassGroup>

      {/* EXP-1097: the relation bands (Blocked by, Blocking, Duplicate
          of/by, Related) live HERE on phones, under a "Relations" heading
          with "Add"; sub-issues and the parent stay on the detail itself. */}
      {relations && <MobileRelationBands {...relations} />}
    </div>
  )
}

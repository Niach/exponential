import type { ReactNode } from "react"
import { forwardRef } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Label as LabelRow, User } from "@/db/schema"
import type { IssuePriority, IssueEstimation } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  creatableStatusOptions,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { labelCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { displayUserName } from "@/lib/user-display"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { BoardPicker } from "@/components/issue-properties/board-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import {
  IssueRelationsAdd,
  IssueRelationGroups,
  useIssueRelations,
} from "@/components/issue-relations-card"
import {
  getPriorityConfig,
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import {
  toStatusMenuOption,
  toStatusMenuOptions,
} from "@/components/issue-properties/status-dropdown"
import {
  Combobox,
  Button,
  conceptIcon,
  DatePicker,
  GlassGroup,
  GlassSectionHeader,
  Pill,
  UserAvatar,
  BoardGlyph,
} from "@exp/ui"
import {
  estimateLabel,
  estimatePickerOptions,
  parseEstimatePick,
} from "@/lib/issue-estimate"

const DueDateGlyph = conceptIcon(`ui-due-date`)
const EstimateGlyph = conceptIcon(`ui-estimate`)
const AddGlyph = conceptIcon(`ui-add`)
const UnassignedGlyph = conceptIcon(`ui-unassigned`)

// Full-width tappable property row: label left, value right — the web
// counterpart of the native create form's metadata card rows (EXP-247).
// EXP-698 r4 matches Android exactly: the LABEL is the muted half and the
// value the readable one, the value's glyph rides with it as one trailing
// unit, and there is no chevron (the whole row is the target).
const PropertyRow = forwardRef<
  HTMLButtonElement,
  Omit<React.ComponentProps<typeof Button>, `value`> & {
    label: string
    value: ReactNode
  }
>(function PropertyRow({ label, value, ...props }, ref) {
  return (
    <Button
      ref={ref}
      type="button"
      variant="ghost"
      className="h-11 w-full justify-between rounded-none px-4 font-normal"
      {...props}
    >
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="flex min-w-0 items-center gap-1.5 text-sm text-foreground">
        {value}
      </span>
    </Button>
  )
})

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

  // The label chips render the WHOLE team list (Android parity) rather than a
  // summary of the picks — the same rows the picker sheet lists, so tapping a
  // chip and ticking it in the sheet are one state.
  const { data: labelRows } = useLiveQuery(
    (q) =>
      teamId
        ? q
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.teamId, teamId))
            .orderBy(({ labels }) => labels.sortOrder)
        : undefined,
    [teamId]
  )
  const labels = (labelRows ?? []) as LabelRow[]

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
        <Combobox
          searchable={false}
          value={status.id}
          disabled={disabled || disableStatus}
          options={toStatusMenuOptions(statusOptions)}
          width="sm"
          onChange={(id) => {
            if (!id) return
            const picked = byId.get(id)
            if (picked) void onStatusChange(picked)
          }}
          mobileTitle="Status"
          renderTrigger={() => (
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
          )}
        />

        <Combobox
          searchable={false}
          value={priority}
          disabled={disabled}
          options={priorities}
          width="sm"
          onChange={(next) => {
            if (next) void onPriorityChange(next)
          }}
          mobileTitle="Priority"
          renderTrigger={() => (
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
          )}
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
          <Combobox
            searchable={false}
            value={estimate == null ? `` : String(estimate)}
            disabled={disabled}
            options={estimatePickerOptions(estimate ?? null, estimation)}
            onChange={(next) => void onEstimateChange(parseEstimatePick(next))}
            mobileTitle="Estimate"
            renderTrigger={() => (
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

      {/* EXP-698 r4: labels leave the row list. Every team label is a chip
          that toggles on tap (Android parity), and the trailing "+ Label"
          chip opens the picker sheet — both write the same selection. */}
      <div className="flex flex-col gap-2">
        <GlassSectionHeader label="Labels" className="px-4 pb-0" />
        <div className="flex flex-wrap items-center gap-1.5 px-4">
          {labels.map((label) => (
            <Pill
              key={label.id}
              size="sm"
              mode="select"
              dot={label.color}
              selected={selectedLabelIds.includes(label.id)}
              disabled={disabled}
              onClick={() => void onToggleLabel(label.id)}
            >
              {label.name}
            </Pill>
          ))}
          <LabelPicker
            disabled={disabled}
            teamId={teamId}
            selectedLabelIds={selectedLabelIds}
            onToggle={onToggleLabel}
            renderTrigger={() => (
              <Pill
                size="sm"
                mode="action"
                disabled={disabled}
                leading={<AddGlyph />}
              >
                Label
              </Pill>
            )}
          />
        </div>
      </div>

      {relations && <MobileRelationsSection {...relations} />}
    </div>
  )
}

// The phone's Relations block: the shared list plus the shared picker, in the
// same header + chips shape the Labels block above uses.
function MobileRelationsSection({
  issueId,
  readOnly = false,
}: {
  issueId: string
  readOnly?: boolean
}) {
  const rows = useIssueRelations(issueId)
  if (readOnly && rows.length === 0) return null

  return (
    <div className="flex flex-col gap-2">
      <GlassSectionHeader label="Relations" className="px-4 pb-0" />
      <div className="flex flex-col gap-1.5 px-4">
        <IssueRelationGroups rows={rows} readOnly={readOnly} />
        {!readOnly && (
          <IssueRelationsAdd
            issueId={issueId}
            trigger={
              <Pill size="sm" mode="action" leading={<AddGlyph />} className="self-start">
                Add relation
              </Pill>
            }
          />
        )}
      </div>
    </div>
  )
}

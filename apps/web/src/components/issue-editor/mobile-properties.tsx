import type { ReactNode } from "react"
import { forwardRef } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { CalendarDays, Plus, User as UserIcon } from "lucide-react"
import type { Label as LabelRow, User } from "@/db/schema"
import { ISSUE_PRIORITY_FALLBACK, type IssuePriority } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  creatableStatusOptions,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { labelCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { formatDate, getInitials } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { BoardPicker } from "@/components/issue-properties/board-picker"
import { BoardGlyph } from "@/components/board-glyph"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import {
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import { toStatusMenuOptions } from "@/components/issue-properties/status-dropdown"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import {
  GlassSectionHeader,
  GlassToggleRow,
} from "@/components/ui/glass-rows"
import { Pill } from "@/components/ui/pill"

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
  dueDate: Date | undefined
  hideAssignee?: boolean
  hideDueDateChip?: boolean
  /** EXP-698 r5 (the issue detail's phone sheet only): a Board row after Due
   * date, moving the issue through the same confirm dialog the desktop chip
   * uses. Absent on the create form — a new issue is already ON its board. */
  board?: {
    boardId: string
    teamId: string
    issueIdentifier?: string | null
    onBoardChange: (boardId: string) => void | Promise<void>
  }
  disableStatus?: boolean
  disabled?: boolean
  onStatusChange: (status: StatusRowOption) => void | Promise<void>
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onToggleLabel: (labelId: string) => void | Promise<void>
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  /** EXP-698 r4 (create only): keep the form open after a create. Absent =
   * no toggle row at all, which is what the edit surfaces want. */
  createMore?: boolean
  onCreateMoreChange?: (next: boolean) => void
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
  board,
  disableStatus,
  disabled,
  onStatusChange,
  onPriorityChange,
  onAssigneeChange,
  onToggleLabel,
  onDueDateSelect,
  createMore,
  onCreateMoreChange,
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

  return (
    <div className="mx-3 my-3 flex flex-col gap-4">
      <div className="divide-y divide-glass-stroke overflow-hidden rounded-xl border border-glass-stroke-card bg-popover/40">
        <OptionDropdownMenu
          value={status.id}
          fallbackValue={status.id}
          disabled={disabled || disableStatus}
          options={toStatusMenuOptions(statusOptions)}
          onSelect={(id) => {
            const picked = byId.get(id)
            if (picked) void onStatusChange(picked)
          }}
          mobileTitle="Status"
          renderTrigger={(selected) => {
            const Icon = selected.icon
            return (
              <PropertyRow
                label="Status"
                disabled={disabled || disableStatus}
                value={
                  <>
                    <Icon
                      className={`!h-3.5 !w-3.5 ${selected.color}`}
                      style={
                        selected.colorHex
                          ? { color: selected.colorHex }
                          : undefined
                      }
                    />
                    {selected.label}
                  </>
                }
              />
            )
          }}
        />

        <OptionDropdownMenu
          value={priority}
          fallbackValue={ISSUE_PRIORITY_FALLBACK}
          disabled={disabled}
          options={priorities}
          onSelect={onPriorityChange}
          mobileTitle="Priority"
          renderTrigger={(selected) => (
            <PropertyRow
              label="Priority"
              disabled={disabled}
              value={
                <>
                  <PriorityIcon
                    priority={selected.value}
                    className="!h-3.5 !w-3.5"
                  />
                  {selected.label}
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
                      <Avatar className="size-4">
                        {assignee.image && (
                          <AvatarImage
                            src={assignee.image}
                            alt={displayUserName(assignee, assignee.id)}
                          />
                        )}
                        <AvatarFallback
                          className="text-[0.5rem]"
                          userId={assignee.id}
                        >
                          {getInitials(displayUserName(assignee, assignee.id))}
                        </AvatarFallback>
                      </Avatar>
                      <span className="max-w-[8rem] truncate">
                        {displayUserName(assignee, assignee.id)}
                      </span>
                    </>
                  ) : (
                    <>
                      <UserIcon className="size-3.5" />
                      Unassigned
                    </>
                  )
                }
              />
            }
          />
        )}

        {!hideDueDateChip && (
          <MobilePopover>
            <MobilePopoverTrigger asChild>
              <PropertyRow
                label="Due date"
                disabled={disabled}
                value={
                  <>
                    <CalendarDays className="size-3.5" />
                    {dueDate ? formatDate(dueDate) : `No date`}
                  </>
                }
              />
            </MobilePopoverTrigger>
            <MobilePopoverContent mobileTitle="Due date">
              <Calendar
                mode="single"
                selected={dueDate}
                onSelect={(date) => {
                  void onDueDateSelect(date)
                }}
                className="mx-auto"
              />
            </MobilePopoverContent>
          </MobilePopover>
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
      </div>

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
                leading={<Plus />}
              >
                Label
              </Pill>
            )}
          />
        </div>
      </div>

      {/* Android/iOS order: properties card, labels, then the bare
          "Create more" row (no card of its own). */}
      {onCreateMoreChange && (
        <GlassToggleRow
          id="issue-editor-create-more"
          label="Create more"
          checked={createMore === true}
          disabled={disabled}
          onCheckedChange={onCreateMoreChange}
          className="px-4"
        />
      )}
    </div>
  )
}

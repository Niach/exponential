import { CalendarDays } from "lucide-react"
import type { User } from "@/db/schema"
import { type IssuePriority, type IssueStatus } from "@/lib/domain"
import { formatDate } from "@/lib/utils"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { priorities, PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { statuses, StatusIcon } from "@/components/issue-properties/status-dropdown"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import { BoardPicker } from "@/components/issue-properties/board-picker"
import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { TimeInput } from "@/components/time-input"

export interface IssuePropertiesPanelProps {
  layout: `sidebar` | `chiprow`
  status: IssueStatus
  onStatusChange: (status: IssueStatus) => void | Promise<void>
  priority: IssuePriority
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  assigneeId: string | null
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  users: User[]
  teamId: string
  selectedLabelIds: string[]
  onToggleLabel: (labelId: string) => void | Promise<void>
  dueDate: Date | undefined
  dueTime: string | null
  endTime: string | null
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  onDueTimeChange: (time: string | null) => void | Promise<void>
  onEndTimeChange: (time: string | null) => void | Promise<void>
  boardName: string
  boardColor: string
  boardPrefix: string
  // Move-to-board control (EXP-57). Optional: when boardId +
  // onBoardChange are provided the read-only board chip becomes a picker
  // (detail view); surfaces without a move affordance simply omit them.
  boardId?: string
  onBoardChange?: (boardId: string) => void | Promise<void>
  disabled?: boolean
  restrictModeration?: boolean
}

function DueDateControl({
  layout,
  disabled,
  dueDate,
  dueTime,
  endTime,
  onDueDateSelect,
  onDueTimeChange,
  onEndTimeChange,
}: Pick<
  IssuePropertiesPanelProps,
  | `layout`
  | `disabled`
  | `dueDate`
  | `dueTime`
  | `endTime`
  | `onDueDateSelect`
  | `onDueTimeChange`
  | `onEndTimeChange`
>) {
  const triggerLabel = dueDate
    ? `${formatDate(dueDate)}${dueTime ? ` · ${dueTime.slice(0, 5)}` : ``}`
    : `Due date`
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className={
            layout === `sidebar`
              ? `justify-start text-muted-foreground hover:text-foreground`
              : `text-muted-foreground shrink-0`
          }
          disabled={disabled}
        >
          <CalendarDays className="size-3" />
          {triggerLabel}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0" align="start">
        <Calendar
          mode="single"
          selected={dueDate}
          onSelect={(date) => void onDueDateSelect(date)}
        />
        {dueDate && (
          <div className="flex items-center gap-2 border-t border-border px-3 py-2 text-xs text-muted-foreground">
            <span>Time</span>
            <TimeInput
              value={dueTime}
              onChange={(t) => void onDueTimeChange(t)}
              className="h-7 w-20 text-xs tabular-nums"
              ariaLabel="Start time"
            />
            <span>–</span>
            <TimeInput
              value={endTime}
              onChange={(t) => void onEndTimeChange(t)}
              disabled={!dueTime}
              className="h-7 w-20 text-xs tabular-nums"
              ariaLabel="End time"
            />
            {(dueTime || endTime) && (
              <Button
                type="button"
                variant="ghost"
                size="xs"
                className="ml-auto h-6 text-xs"
                onClick={() => {
                  void onDueTimeChange(null)
                  void onEndTimeChange(null)
                }}
              >
                All day
              </Button>
            )}
          </div>
        )}
      </PopoverContent>
    </Popover>
  )
}

function BoardChip({
  boardColor,
  boardPrefix,
  boardName,
  layout,
}: Pick<
  IssuePropertiesPanelProps,
  `boardColor` | `boardPrefix` | `boardName` | `layout`
>) {
  return (
    <div
      className={
        layout === `sidebar`
          ? `inline-flex items-center gap-1.5 rounded-md bg-accent/40 px-2 py-1 text-xs font-medium text-foreground`
          : `inline-flex items-center gap-1.5 rounded-md bg-accent/50 px-2 py-0.5 text-xs font-medium text-foreground shrink-0`
      }
    >
      <div
        className="h-2.5 w-2.5 rounded-full"
        style={{ backgroundColor: boardColor }}
      />
      {layout === `sidebar` ? boardName : boardPrefix}
    </div>
  )
}

export function IssuePropertiesPanel(props: IssuePropertiesPanelProps) {
  const {
    layout,
    status,
    onStatusChange,
    priority,
    onPriorityChange,
    assigneeId,
    onAssigneeChange,
    users,
    teamId,
    selectedLabelIds,
    onToggleLabel,
    disabled,
    restrictModeration,
  } = props

  const moderationDisabled = disabled || restrictModeration

  // Solo team (exactly one human member): hide the assignee control
  // entirely — nobody else to assign to. `users` is the bot-excluded member
  // list; length 0 means still loading (never a genuine empty), so multi-member
  // teams never briefly read as solo.
  const isSolo = users.length === 1

  const statusControl = (
    <OptionDropdownMenu
      value={status}
      disabled={moderationDisabled}
      options={statuses}
      onSelect={onStatusChange}
      mobileTitle="Status"
      renderTrigger={(selected) => (
        <Button
          variant="ghost"
          size="xs"
          className={
            layout === `sidebar`
              ? `justify-start text-muted-foreground hover:text-foreground`
              : `text-muted-foreground shrink-0`
          }
          disabled={moderationDisabled}
        >
          <StatusIcon status={selected.value} className="!h-3 !w-3" />
          {selected.label}
        </Button>
      )}
    />
  )

  const priorityControl = (
    <OptionDropdownMenu
      value={priority}
      disabled={moderationDisabled}
      options={priorities}
      onSelect={onPriorityChange}
      mobileTitle="Priority"
      renderTrigger={(selected) => (
        <Button
          variant="ghost"
          size="xs"
          className={
            layout === `sidebar`
              ? `justify-start text-muted-foreground hover:text-foreground`
              : `text-muted-foreground shrink-0`
          }
          disabled={moderationDisabled}
        >
          <PriorityIcon priority={selected.value} className="!h-3 !w-3" />
          {selected.label}
        </Button>
      )}
    />
  )

  const assigneeControl = (
    <AssigneePicker
      disabled={moderationDisabled}
      users={users}
      selectedUserId={assigneeId}
      onSelect={onAssigneeChange}
    />
  )

  const labelControl = (
    <LabelPicker
      disabled={disabled}
      teamId={teamId}
      selectedLabelIds={selectedLabelIds}
      onToggle={onToggleLabel}
    />
  )

  const dueDateControl = (
    <DueDateControl
      layout={layout}
      disabled={moderationDisabled}
      dueDate={props.dueDate}
      dueTime={props.dueTime}
      endTime={props.endTime}
      onDueDateSelect={props.onDueDateSelect}
      onDueTimeChange={props.onDueTimeChange}
      onEndTimeChange={props.onEndTimeChange}
    />
  )

  const boardChip =
    props.boardId && props.onBoardChange ? (
      <BoardPicker
        disabled={moderationDisabled}
        teamId={teamId}
        selectedBoardId={props.boardId}
        onSelect={props.onBoardChange}
      />
    ) : (
      <BoardChip
        boardColor={props.boardColor}
        boardPrefix={props.boardPrefix}
        boardName={props.boardName}
        layout={layout}
      />
    )

  if (layout === `sidebar`) {
    return (
      <aside className="w-72 shrink-0 border-l border-border px-4 py-4 space-y-4 text-sm">
        <PropertyGroup label="Status">{statusControl}</PropertyGroup>
        <PropertyGroup label="Priority">{priorityControl}</PropertyGroup>
        {!isSolo && (
          <PropertyGroup label="Assignee">{assigneeControl}</PropertyGroup>
        )}
        <PropertyGroup label="Labels">{labelControl}</PropertyGroup>
        <PropertyGroup label="Due date">{dueDateControl}</PropertyGroup>
        <PropertyGroup label="Board">{boardChip}</PropertyGroup>
      </aside>
    )
  }

  // chiprow — horizontal scroll on overflow
  return (
    <div className="flex flex-wrap items-center gap-1.5 px-3 py-2 border-b border-border">
      {statusControl}
      {priorityControl}
      {!isSolo && assigneeControl}
      {labelControl}
      {dueDateControl}
      {boardChip}
    </div>
  )
}

function PropertyGroup({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-1">
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </div>
      <div className="flex items-center">{children}</div>
    </div>
  )
}

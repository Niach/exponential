import type { ReactNode } from "react"
import { CalendarDays, MoreHorizontal } from "lucide-react"
import type { User } from "@/db/schema"
import type { IssuePriority, IssueStatus } from "@/lib/domain"
import { formatDate } from "@/lib/utils"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import {
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import {
  statuses,
  StatusIcon,
} from "@/components/issue-properties/status-dropdown"

// Marking a brand-new issue as a duplicate is nonsense (there is nothing yet to
// dedupe), so the create/edit chip row never offers `duplicate` — the status is
// only ever reached via the duplicate-picker interception on an existing issue
// (masterplan §4.1 / L27).
export const creatableStatuses = statuses.filter(
  (option) => option.value !== `duplicate`
)
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { TimeInput } from "@/components/time-input"
import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

export interface IssueEditorChipsProps {
  status: IssueStatus
  priority: IssuePriority
  assigneeId: string | null
  selectedLabelIds: string[]
  teamId: string
  users: User[]
  dueDate: Date | undefined
  dueTime: string | null
  endTime: string | null
  hideAssignee?: boolean
  hideDueDateChip?: boolean
  disableStatus?: boolean
  disabled?: boolean
  chipRowExtras?: ReactNode
  overflowMenuItems?: ReactNode
  onStatusChange: (status: IssueStatus) => void | Promise<void>
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onToggleLabel: (labelId: string) => void | Promise<void>
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  onDueTimeChange: (time: string | null) => void | Promise<void>
  onEndTimeChange: (time: string | null) => void | Promise<void>
}

export function IssueEditorChips({
  status,
  priority,
  assigneeId,
  selectedLabelIds,
  teamId,
  users,
  dueDate,
  dueTime,
  endTime,
  hideAssignee,
  hideDueDateChip,
  disableStatus,
  disabled,
  chipRowExtras,
  overflowMenuItems,
  onStatusChange,
  onPriorityChange,
  onAssigneeChange,
  onToggleLabel,
  onDueDateSelect,
  onDueTimeChange,
  onEndTimeChange,
}: IssueEditorChipsProps) {
  return (
    <>
      <OptionDropdownMenu
        value={status}
        disabled={disabled || disableStatus}
        options={creatableStatuses}
        onSelect={onStatusChange}
        mobileTitle="Status"
        renderTrigger={(selected) => (
          <Button
            variant="ghost"
            size="xs"
            className="text-muted-foreground shrink-0"
            disabled={disabled || disableStatus}
          >
            <StatusIcon status={selected.value} className="!h-3 !w-3" />
            {selected.label}
          </Button>
        )}
      />

      <OptionDropdownMenu
        value={priority}
        disabled={disabled}
        options={priorities}
        onSelect={onPriorityChange}
        mobileTitle="Priority"
        renderTrigger={(selected) => (
          <Button
            variant="ghost"
            size="xs"
            className="text-muted-foreground shrink-0"
            disabled={disabled}
          >
            <PriorityIcon priority={selected.value} className="!h-3 !w-3" />
            {selected.label}
          </Button>
        )}
      />

      {!hideAssignee && (
        <AssigneePicker
          disabled={disabled}
          users={users}
          selectedUserId={assigneeId}
          onSelect={onAssigneeChange}
        />
      )}

      <LabelPicker
        disabled={disabled}
        teamId={teamId}
        selectedLabelIds={selectedLabelIds}
        onToggle={onToggleLabel}
      />

      {!hideDueDateChip && (
        <Popover>
          <PopoverTrigger asChild>
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground shrink-0"
              disabled={disabled}
            >
              <CalendarDays className="size-3" />
              {dueDate
                ? `${formatDate(dueDate)}${dueTime ? ` · ${dueTime.slice(0, 5)}` : ``}`
                : `Due date`}
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-auto p-0" align="start">
            <Calendar
              mode="single"
              selected={dueDate}
              onSelect={(date) => {
                void onDueDateSelect(date)
              }}
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
      )}

      {chipRowExtras}

      {overflowMenuItems && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label="More options"
              disabled={disabled}
              className="text-muted-foreground shrink-0"
            >
              <MoreHorizontal className="size-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {overflowMenuItems}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </>
  )
}

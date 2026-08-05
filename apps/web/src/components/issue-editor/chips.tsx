import type { ReactNode } from "react"
import { CalendarDays, Ellipsis } from "lucide-react"
import type { User } from "@/db/schema"
import { ISSUE_PRIORITY_FALLBACK, type IssuePriority } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  creatableStatusOptions,
  sortStatusesForPicker,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { formatDate } from "@/lib/utils"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import {
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import { toStatusMenuOptions } from "@/components/issue-properties/status-dropdown"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
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
  // EXP-314: the RESOLVED team status row (never a bare enum) so a custom
  // status survives a round-trip through the editor.
  status: StatusRowOption
  priority: IssuePriority
  assigneeId: string | null
  selectedLabelIds: string[]
  teamId: string
  users: User[]
  dueDate: Date | undefined
  hideAssignee?: boolean
  hideDueDateChip?: boolean
  disableStatus?: boolean
  disabled?: boolean
  chipRowExtras?: ReactNode
  overflowMenuItems?: ReactNode
  onStatusChange: (status: StatusRowOption) => void | Promise<void>
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onToggleLabel: (labelId: string) => void | Promise<void>
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
}

export function IssueEditorChips({
  status,
  priority,
  assigneeId,
  selectedLabelIds,
  teamId,
  users,
  dueDate,
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
}: IssueEditorChipsProps) {
  const { options, byId } = useTeamStatusesContext()
  const statusOptions = sortStatusesForPicker(creatableStatusOptions(options))

  return (
    <>
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
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground shrink-0"
              disabled={disabled || disableStatus}
            >
              <Icon
                className={`!h-3 !w-3 ${selected.color}`}
                style={
                  selected.colorHex ? { color: selected.colorHex } : undefined
                }
              />
              {selected.label}
            </Button>
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
              {dueDate ? formatDate(dueDate) : `Due date`}
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
              <Ellipsis className="size-3" />
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

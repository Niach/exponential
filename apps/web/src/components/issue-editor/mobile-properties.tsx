import type { ReactNode } from "react"
import { forwardRef } from "react"
import { CalendarDays, Tag, User as UserIcon } from "lucide-react"
import type { Label as LabelRow, User } from "@/db/schema"
import { ISSUE_PRIORITY_FALLBACK, type IssuePriority } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  creatableStatusOptions,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { formatDate, getInitials } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
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

// Full-width tappable property row: label left, value right — the web
// counterpart of the native create form's metadata card rows (EXP-247).
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
      <span className="text-sm">{label}</span>
      <span className="flex min-w-0 items-center gap-1.5 text-sm text-muted-foreground">
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
  disableStatus?: boolean
  disabled?: boolean
  onStatusChange: (status: StatusRowOption) => void | Promise<void>
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onToggleLabel: (labelId: string) => void | Promise<void>
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
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
  disableStatus,
  disabled,
  onStatusChange,
  onPriorityChange,
  onAssigneeChange,
  onToggleLabel,
  onDueDateSelect,
}: IssueEditorMobilePropertiesProps) {
  const { options, byId } = useTeamStatusesContext()
  const statusOptions = creatableStatusOptions(options)
  const assignee = assigneeId
    ? users.find((user) => user.id === assigneeId)
    : undefined

  return (
    <div className="mx-3 my-3 divide-y divide-border/40 overflow-hidden rounded-xl border border-border/60 bg-accent/20">
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
                      <AvatarFallback className="text-[0.5rem]">
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
                  {dueDate ? formatDate(dueDate) : `None`}
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

      <LabelPicker
        disabled={disabled}
        teamId={teamId}
        selectedLabelIds={selectedLabelIds}
        onToggle={onToggleLabel}
        renderTrigger={(selectedLabels: LabelRow[]) => (
          <PropertyRow
            label="Labels"
            disabled={disabled}
            value={
              selectedLabels.length > 0 ? (
                <>
                  <span className="flex items-center -space-x-0.5">
                    {selectedLabels.slice(0, 3).map((row) => (
                      <span
                        key={row.id}
                        className="h-2 w-2 shrink-0 rounded-full ring-1 ring-background"
                        style={{ backgroundColor: row.color }}
                      />
                    ))}
                  </span>
                  <span className="max-w-[8rem] truncate">
                    {selectedLabels.map((row) => row.name).join(`, `)}
                  </span>
                </>
              ) : (
                <>
                  <Tag className="size-3.5" />
                  None
                </>
              )
            }
          />
        )}
      />

    </div>
  )
}

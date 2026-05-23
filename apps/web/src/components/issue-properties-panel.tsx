import { CalendarDays, Repeat } from "lucide-react"
import type { User } from "@/db/schema"
import {
  formatRecurrence,
  type IssuePriority,
  type IssueStatus,
  type RecurrenceUnit,
} from "@/lib/domain"
import { formatDate } from "@/lib/utils"
import {
  RecurrenceEditor,
  type RecurrenceValue,
} from "@/components/recurrence-editor"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { priorities, PriorityIcon } from "@/components/priority-dropdown"
import { statuses, StatusIcon } from "@/components/status-dropdown"
import { AssigneePicker } from "@/components/assignee-picker"
import { LabelPicker } from "@/components/label-picker"
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
  workspaceId: string
  selectedLabelIds: string[]
  onToggleLabel: (labelId: string) => void | Promise<void>
  dueDate: Date | undefined
  dueTime: string | null
  endTime: string | null
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  onDueTimeChange: (time: string | null) => void | Promise<void>
  onEndTimeChange: (time: string | null) => void | Promise<void>
  recurrenceInterval: number | null
  recurrenceUnit: RecurrenceUnit | null
  onRecurrenceChange: (next: RecurrenceValue | null) => void | Promise<void>
  projectName: string
  projectColor: string
  projectPrefix: string
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

function RecurrenceControl({
  layout,
  disabled,
  recurrenceInterval,
  recurrenceUnit,
  dueDate,
  onRecurrenceChange,
}: Pick<
  IssuePropertiesPanelProps,
  | `layout`
  | `disabled`
  | `recurrenceInterval`
  | `recurrenceUnit`
  | `dueDate`
  | `onRecurrenceChange`
>) {
  const isRecurring = recurrenceInterval !== null && recurrenceUnit !== null
  const label = isRecurring
    ? formatRecurrence(recurrenceInterval!, recurrenceUnit!)
    : `Add recurrence`
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
          <Repeat className="size-3" />
          {label}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-auto p-3 space-y-2">
        <RecurrenceEditor
          value={
            isRecurring
              ? {
                  firstDue: dueDate,
                  interval: recurrenceInterval!,
                  unit: recurrenceUnit!,
                }
              : {
                  firstDue: dueDate ?? new Date(),
                  interval: 1,
                  unit: `week`,
                }
          }
          onChange={(next) => void onRecurrenceChange(next)}
        />
        {isRecurring && (
          <Button
            type="button"
            variant="ghost"
            size="xs"
            className="text-muted-foreground"
            onClick={() => void onRecurrenceChange(null)}
          >
            Stop recurring
          </Button>
        )}
      </PopoverContent>
    </Popover>
  )
}

function ProjectChip({
  projectColor,
  projectPrefix,
  projectName,
  layout,
}: Pick<
  IssuePropertiesPanelProps,
  `projectColor` | `projectPrefix` | `projectName` | `layout`
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
        style={{ backgroundColor: projectColor }}
      />
      {layout === `sidebar` ? projectName : projectPrefix}
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
    workspaceId,
    selectedLabelIds,
    onToggleLabel,
    disabled,
    restrictModeration,
  } = props

  const moderationDisabled = disabled || restrictModeration

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
      workspaceId={workspaceId}
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

  const recurrenceControl = (
    <RecurrenceControl
      layout={layout}
      disabled={moderationDisabled}
      recurrenceInterval={props.recurrenceInterval}
      recurrenceUnit={props.recurrenceUnit}
      dueDate={props.dueDate}
      onRecurrenceChange={props.onRecurrenceChange}
    />
  )

  const projectChip = (
    <ProjectChip
      projectColor={props.projectColor}
      projectPrefix={props.projectPrefix}
      projectName={props.projectName}
      layout={layout}
    />
  )

  if (layout === `sidebar`) {
    return (
      <aside className="w-72 shrink-0 border-l border-border px-4 py-4 space-y-4 text-sm">
        <PropertyGroup label="Status">{statusControl}</PropertyGroup>
        <PropertyGroup label="Priority">{priorityControl}</PropertyGroup>
        <PropertyGroup label="Assignee">{assigneeControl}</PropertyGroup>
        <PropertyGroup label="Labels">{labelControl}</PropertyGroup>
        <PropertyGroup label="Due date">
          <div className="flex flex-col items-start gap-1">
            {dueDateControl}
            {recurrenceControl}
          </div>
        </PropertyGroup>
        <PropertyGroup label="Project">{projectChip}</PropertyGroup>
      </aside>
    )
  }

  // chiprow — horizontal scroll on overflow
  return (
    <div className="flex flex-wrap items-center gap-1.5 px-3 py-2 border-b border-border">
      {statusControl}
      {priorityControl}
      {assigneeControl}
      {labelControl}
      {dueDateControl}
      {recurrenceControl}
      {projectChip}
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

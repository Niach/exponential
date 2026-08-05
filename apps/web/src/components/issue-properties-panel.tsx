import { CalendarDays, Megaphone, Plus, Tag } from "lucide-react"
import type { User } from "@/db/schema"
import {
  ISSUE_PRIORITY_FALLBACK,
  type IssuePriority,
  type IssueSource,
} from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  sortStatusesForPicker,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { Badge } from "@/components/ui/badge"
import { formatDate } from "@/lib/utils"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import {
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import { toStatusMenuOptions } from "@/components/issue-properties/status-dropdown"
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

export interface IssuePropertiesPanelProps {
  layout: `sidebar` | `chiprow`
  // EXP-314: the RESOLVED team status row. The duplicate-category row stays in
  // the menu (the picker intercepts it), matching the pre-EXP-314 control.
  status: StatusRowOption
  onStatusChange: (status: StatusRowOption) => void | Promise<void>
  priority: IssuePriority
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  assigneeId: string | null
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  users: User[]
  teamId: string
  selectedLabelIds: string[]
  onToggleLabel: (labelId: string) => void | Promise<void>
  dueDate: Date | undefined
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  // Where the issue came from. Only `widget` renders anything (a muted
  // "Feedback widget" pill); `user` (the default) shows nothing.
  source?: IssueSource
  boardName: string
  boardColor: string
  boardPrefix: string
  // Move-to-board control (EXP-57). Optional: when boardId +
  // onBoardChange are provided the read-only board chip becomes a picker
  // (detail view); surfaces without a move affordance simply omit them.
  boardId?: string
  onBoardChange?: (boardId: string) => void | Promise<void>
  // Names the issue in the move confirmation; only read alongside a picker.
  issueIdentifier?: string | null
  disabled?: boolean
  // Coding section (EXP-184) — sidebar layout only. The slot owns its own
  // unlabeled PropertyGroup (its gating lives in issue-coding-rows.tsx, so the
  // panel can't know whether it renders anything).
  codingSlot?: React.ReactNode
}

function DueDateControl({
  layout,
  disabled,
  dueDate,
  onDueDateSelect,
}: Pick<
  IssuePropertiesPanelProps,
  `layout` | `disabled` | `dueDate` | `onDueDateSelect`
>) {
  const triggerLabel = dueDate ? formatDate(dueDate) : `Due date`
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

// Muted origin pill shown only for widget-filed issues (no user creator). The
// author byline slot for feedback that came in through the embeddable widget.
function SourceChip() {
  return (
    <Badge
      variant="secondary"
      className="gap-1 font-normal text-muted-foreground"
    >
      <Megaphone className="size-3" />
      Feedback widget
    </Badge>
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
  } = props

  // Solo team (exactly one human member): hide the assignee control
  // entirely — nobody else to assign to. `users` is the bot-excluded member
  // list; length 0 means still loading (never a genuine empty), so multi-member
  // teams never briefly read as solo.
  const isSolo = users.length === 1
  const { options: teamStatusOptions, byId: statusById } =
    useTeamStatusesContext()

  const statusControl = (
    <OptionDropdownMenu
      value={status.id}
      fallbackValue={status.id}
      disabled={disabled}
      options={toStatusMenuOptions(sortStatusesForPicker(teamStatusOptions))}
      onSelect={(id) => {
        const picked = statusById.get(id)
        if (picked) void onStatusChange(picked)
      }}
      mobileTitle="Status"
      renderTrigger={(selected) => {
        const Icon = selected.icon
        return (
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
  )

  const priorityControl = (
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
          className={
            layout === `sidebar`
              ? `justify-start text-muted-foreground hover:text-foreground`
              : `text-muted-foreground shrink-0`
          }
          disabled={disabled}
        >
          <PriorityIcon priority={selected.value} className="!h-3 !w-3" />
          {selected.label}
        </Button>
      )}
    />
  )

  const assigneeControl = (
    <AssigneePicker
      disabled={disabled}
      users={users}
      selectedUserId={assigneeId}
      onSelect={onAssigneeChange}
      triggerClassName={
        layout === `sidebar` ? `justify-start hover:text-foreground` : undefined
      }
    />
  )

  // Sidebar (EXP-427): Linear-style label chips + a "+" affordance under the
  // Labels heading; the whole row opens the picker. Chiprow keeps the compact
  // dots-and-names default trigger.
  const labelControl =
    layout === `sidebar` ? (
      <LabelPicker
        disabled={disabled}
        teamId={teamId}
        selectedLabelIds={selectedLabelIds}
        onToggle={onToggleLabel}
        renderTrigger={(selectedLabels) => (
          <Button
            variant="ghost"
            size="xs"
            className="h-auto max-w-full justify-start py-1 text-muted-foreground hover:text-foreground"
            disabled={disabled}
          >
            {selectedLabels.length > 0 ? (
              <span className="flex flex-wrap items-center gap-1">
                {selectedLabels.map((label) => (
                  <span
                    key={label.id}
                    className="inline-flex max-w-full items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-xs text-foreground"
                  >
                    <span
                      className="h-2 w-2 shrink-0 rounded-full"
                      style={{ backgroundColor: label.color }}
                    />
                    <span className="truncate">{label.name}</span>
                  </span>
                ))}
                <Plus className="size-3" />
              </span>
            ) : (
              <>
                <Tag className="size-3" />
                Add label
              </>
            )}
          </Button>
        )}
      />
    ) : (
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
      disabled={disabled}
      dueDate={props.dueDate}
      onDueDateSelect={props.onDueDateSelect}
    />
  )

  const boardChip =
    props.boardId && props.onBoardChange ? (
      <BoardPicker
        disabled={disabled}
        teamId={teamId}
        selectedBoardId={props.boardId}
        issueIdentifier={props.issueIdentifier}
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

  const isWidgetSourced = props.source === `widget`

  // EXP-422: no left rule (the reading column's own width already reads as the
  // boundary), and the sidebar scrolls on its own so a tall property stack
  // stays reachable without moving the description.
  // EXP-427: Linear-style grouping — one "Properties" stack without per-field
  // titles, a Labels section, then the (unlabeled) coding slot.
  if (layout === `sidebar`) {
    return (
      <aside className="w-72 shrink-0 overflow-y-auto overscroll-contain px-4 py-4 space-y-5 text-sm">
        <PropertyGroup label="Properties">
          {statusControl}
          {priorityControl}
          {!isSolo && assigneeControl}
          {dueDateControl}
          {boardChip}
          {isWidgetSourced && <SourceChip />}
        </PropertyGroup>
        <PropertyGroup label="Labels">{labelControl}</PropertyGroup>
        {props.codingSlot}
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
      {isWidgetSourced && <SourceChip />}
    </div>
  )
}

// A sidebar section (EXP-427): an optional Linear-style muted heading over a
// stacked run of rows. No label = just the stack (the coding slot).
export function PropertyGroup({
  label,
  children,
}: {
  label?: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-1.5">
      {label && (
        <div className="text-xs font-medium text-muted-foreground">{label}</div>
      )}
      <div className="flex flex-col items-start gap-1">{children}</div>
    </div>
  )
}

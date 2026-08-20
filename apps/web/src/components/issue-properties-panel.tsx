import { CalendarDays, Megaphone } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { User } from "@/db/schema"
import {
  ISSUE_PRIORITY_FALLBACK,
  type IssuePriority,
  type IssueSource,
} from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusRowOption } from "@/lib/team-statuses"
import { Badge } from "@/components/ui/badge"
import { cn, formatDate } from "@/lib/utils"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import {
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import { toStatusMenuOptions } from "@/components/issue-properties/status-dropdown"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import { BoardPicker } from "@/components/issue-properties/board-picker"
import { BoardGlyph } from "@/components/board-glyph"
import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

export interface IssuePropertiesPanelProps {
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
  boardColor: string
  boardPrefix: string
  // Board glyph inputs for the read-only chip (EXP-449: icon+color instead
  // of the anonymous dot).
  boardIcon?: string | null
  boardRepositoryId?: string | null
  // Move-to-board control (EXP-57). Optional: when boardId +
  // onBoardChange are provided the read-only board chip becomes a picker
  // (detail view); surfaces without a move affordance simply omit them.
  boardId?: string
  onBoardChange?: (boardId: string) => void | Promise<void>
  // Names the issue in the move confirmation; only read alongside a picker.
  issueIdentifier?: string | null
  disabled?: boolean
  /** Extra classes for the chip row's own container, so hosts can drop it
   *  into their own card without fighting a baked-in border. */
  className?: string
}

function DueDateControl({
  disabled,
  dueDate,
  onDueDateSelect,
}: Pick<
  IssuePropertiesPanelProps,
  `disabled` | `dueDate` | `onDueDateSelect`
>) {
  const triggerLabel = dueDate ? formatDate(dueDate) : `Due date`
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground shrink-0"
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
  boardIcon,
  boardRepositoryId,
}: Pick<
  IssuePropertiesPanelProps,
  `boardColor` | `boardPrefix` | `boardIcon` | `boardRepositoryId`
>) {
  return (
    <div className="inline-flex shrink-0 items-center gap-1.5 rounded-md bg-accent/50 px-2 py-0.5 text-xs font-medium text-foreground">
      <BoardGlyph
        board={{
          icon: boardIcon,
          repositoryId: boardRepositoryId,
          color: boardColor,
        }}
        className="size-3.5"
      />
      {boardPrefix}
    </div>
  )
}

const AgentSourceIcon = conceptIcon(`ui-agent-source`)

// Muted origin pill shown only for issues without a user creator: feedback
// that came in through the embeddable widget, or a bug report filed by a
// coding agent via the MCP `exponential_report_bug` tool (EXP-496).
function SourceChip({ source }: { source: string }) {
  const Icon = source === `agent` ? AgentSourceIcon : Megaphone
  return (
    <Badge
      variant="secondary"
      className="gap-1 font-normal text-muted-foreground"
    >
      <Icon className="size-3" />
      {source === `agent` ? `Agent` : `Feedback widget`}
    </Badge>
  )
}

export function IssuePropertiesPanel(props: IssuePropertiesPanelProps) {
  const {
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
      options={toStatusMenuOptions(teamStatusOptions)}
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
            className="text-muted-foreground shrink-0"
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
          className="text-muted-foreground shrink-0"
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
        boardIcon={props.boardIcon}
        boardRepositoryId={props.boardRepositoryId}
      />
    )

  const source = props.source
  const sourceChip =
    source === `widget` || source === `agent` ? (
      <SourceChip source={source} />
    ) : null

  // EXP-568: ONE layout. The properties moved to the top of the reading
  // column on every viewport (the desktop sidebar is gone), so the row owns no
  // chrome of its own — the host wraps it in the glass card that separates it
  // from the title above and the description below.
  return (
    <div
      className={cn(
        `flex flex-wrap items-center gap-1.5 px-3 py-2`,
        props.className
      )}
    >
      {statusControl}
      {priorityControl}
      {!isSolo && assigneeControl}
      {labelControl}
      {dueDateControl}
      {boardChip}
      {sourceChip}
    </div>
  )
}

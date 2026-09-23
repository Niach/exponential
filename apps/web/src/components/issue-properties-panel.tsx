import { Megaphone } from "lucide-react"
import { conceptIcon, Combobox, Pill, DatePicker, BoardGlyph } from "@exp/ui"
import type { User } from "@/db/schema"
import type { IssuePriority, IssueSource } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusRowOption } from "@/lib/team-statuses"
import { cn } from "@/lib/utils"
import {
  estimatePickerOptions,
  estimateShortLabel,
  parseEstimatePick,
} from "@/lib/issue-estimate"
import {
  getPriorityConfig,
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import {
  toStatusMenuOption,
  toStatusMenuOptions,
} from "@/components/issue-properties/status-dropdown"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import { BoardPicker } from "@/components/issue-properties/board-picker"

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
  /** `YYYY-MM-DD`, or null for "no due date" — the wire format, never a
   *  `Date` (REV2-49: a due date has no time of day). */
  dueDate: string | null
  onDueDateSelect: (date: string | null) => void | Promise<void>
  /** EXP-630: story points. The chip renders only when a handler is given
   *  (the create form has none). */
  estimate?: number | null
  onEstimateChange?: (estimate: number | null) => void | Promise<void>
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

const DueDateGlyph = conceptIcon(`ui-due-date`)
const EstimateGlyph = conceptIcon(`ui-estimate`)

// EXP-630: story points as a chip + picker, the priority control's shape.
// Options are strings (the picker's currency); `""` clears.
function EstimateControl({
  disabled,
  estimate,
  onEstimateChange,
}: {
  disabled?: boolean
  estimate: number | null
  onEstimateChange: (estimate: number | null) => void | Promise<void>
}) {
  return (
    <Combobox
      searchable={false}
      value={estimate === null ? `` : String(estimate)}
      disabled={disabled}
      options={estimatePickerOptions(estimate)}
      width="sm"
      onChange={(next) => void onEstimateChange(parseEstimatePick(next))}
      mobileTitle="Estimate"
      renderTrigger={() => (
        <Pill mode="action" disabled={disabled}>
          <EstimateGlyph className="size-3" />
          {estimate === null ? `Estimate` : estimateShortLabel(estimate)}
        </Pill>
      )}
    />
  )
}

function DueDateControl({
  disabled,
  dueDate,
  onDueDateSelect,
}: Pick<
  IssuePropertiesPanelProps,
  `disabled` | `dueDate` | `onDueDateSelect`
>) {
  return (
    <DatePicker
      value={dueDate}
      onChange={(date) => void onDueDateSelect(date)}
      disabled={disabled}
      align="start"
      renderTrigger={({ label }) => (
        <Pill mode="action" disabled={disabled}>
          <DueDateGlyph className="size-3" />
          {label}
        </Pill>
      )}
    />
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
    <Pill
      className="font-normal text-muted-foreground"
      leading={<Icon className="size-3" />}
    >
      {source === `agent` ? `Agent` : `Feedback widget`}
    </Pill>
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

  // EXP-958: both chips draw the RESOLVED property the panel was handed,
  // never the picker's matched option — the resolvers already fall back for a
  // value the table does not carry (REV2-85), where a match would be empty.
  const statusTrigger = toStatusMenuOption(status)
  const StatusTriggerIcon = statusTrigger.icon
  const priorityTrigger = getPriorityConfig(priority)

  const statusControl = (
    <Combobox
      searchable={false}
      value={status.id}
      disabled={disabled}
      options={toStatusMenuOptions(teamStatusOptions)}
      width="sm"
      onChange={(id) => {
        if (!id) return
        const picked = statusById.get(id)
        if (picked) void onStatusChange(picked)
      }}
      mobileTitle="Status"
      renderTrigger={() => (
        <Pill mode="action" disabled={disabled}>
          <StatusTriggerIcon
            className={`!h-3 !w-3 ${statusTrigger.color}`}
            style={
              statusTrigger.colorHex
                ? { color: statusTrigger.colorHex }
                : undefined
            }
          />
          {statusTrigger.label}
        </Pill>
      )}
    />
  )

  const priorityControl = (
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
        <Pill mode="action" disabled={disabled}>
          <PriorityIcon priority={priorityTrigger.value} className="!h-3 !w-3" />
          {priorityTrigger.label}
        </Pill>
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

  const estimateControl = props.onEstimateChange ? (
    <EstimateControl
      disabled={disabled}
      estimate={props.estimate ?? null}
      onEstimateChange={props.onEstimateChange}
    />
  ) : null

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
      {estimateControl}
      {boardChip}
      {sourceChip}
    </div>
  )
}

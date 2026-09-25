import type { Issue, Label, Board, User } from "@/db/schema"
import { getIssuePriorityConfig, issuePriorityOptions } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusRowOption } from "@/lib/team-statuses"
import {
  StatusIcon,
  toStatusPickerStatuses,
} from "@/components/issue-properties/status-dropdown"
import {
  assigneePickerItems,
  boardPickerItems,
  ComboboxMenuItems,
  conceptIcon,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  labelPickerItems,
  pickerMenuRows,
  priorityPickerItems,
  statusPickerItems,
  UserAvatar,
} from "@exp/ui"
import { displayUserName } from "@/lib/user-display"
import type { IssueEstimation } from "@/lib/domain"
import {
  NO_ESTIMATE,
  estimateLabel,
  estimatePickerValues,
  estimateShortLabel,
  parseEstimatePick,
} from "@/lib/issue-estimate"

// EXP-957 — every submenu BODY here is the Combobox's menu arm: the rows are
// `ComboboxMenuItems`, so the selection glyph is the primitive's (a trailing
// `ui-check`, or the circle pair on the multi arm) and not the menu's own
// radio dot / checkbox tick. The triggers stay hand-drawn: they mirror the
// ROW's current value (EXP-59), which is not a picker concern.
//
// EXP-1021 — and the rows themselves are now the TYPED pickers' rows, bridged
// through `pickerMenuRows`: a Radix submenu is a shell the picker primitive
// cannot nest inside, but a board row, a label row, a member row and a status
// row are built in exactly ONE place for all of them.

// EXP-687: "Move to board" draws the SAME glyph on all four clients.
const NavBoardsIcon = conceptIcon(`nav-boards`)
const UnassignedIcon = conceptIcon(`ui-unassigned`)
const LabelsIcon = conceptIcon(`settings-labels`)
const EstimateIcon = conceptIcon(`ui-estimate`)

interface StatusSubmenuProps {
  // The RESOLVED team status row of this issue (EXP-314).
  status: StatusRowOption
  topLevelValueClass: string
  onSelect: (status: StatusRowOption) => void
}

export function StatusSubmenu({
  status,
  topLevelValueClass,
  onSelect,
}: StatusSubmenuProps) {
  // Trigger mirrors the row's status icon: the CURRENT status, not a generic
  // glyph (EXP-59).
  const { options } = useTeamStatusesContext()

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <StatusIcon option={status} />
        Status
        <DropdownMenuShortcut className={topLevelValueClass}>
          {status.name}
        </DropdownMenuShortcut>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-[14rem]">
        <ComboboxMenuItems
          menu="dropdown"
          {...pickerMenuRows(statusPickerItems(toStatusPickerStatuses(options)))}
          value={status.id}
          onChange={(id) => {
            const picked = options.find((option) => option.id === id)
            if (picked) {
              onSelect(picked)
            }
          }}
        />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

interface AssigneeSubmenuProps {
  assigneeId: Issue[`assigneeId`]
  orderedUsers: User[]
  selectedAssignee: User | null
  topLevelValueClass: string
  onSelect: (userId: string | null) => void
}

export function AssigneeSubmenu({
  assigneeId,
  orderedUsers,
  selectedAssignee,
  topLevelValueClass,
  onSelect,
}: AssigneeSubmenuProps) {
  // Literally the `AssigneePicker`'s rows (EXP-1021). `Unassigned` is the
  // menu arm's own `noneLabel` here rather than the picker's `allowsNone`
  // row — a menu reports `null` through the same single arm.
  const members = orderedUsers.map((user) => ({
    id: user.id,
    name: displayUserName(user, user.id),
    email: user.email,
    image: user.image,
  }))
  const membersById = new Map(members.map((member) => [member.id, member]))
  const rows = pickerMenuRows(assigneePickerItems(members))

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        {/* Current assignee's avatar; person placeholder when unassigned (EXP-59). */}
        {selectedAssignee ? (
          <UserAvatar
            size={16}
            user={{
              id: selectedAssignee.id,
              name: displayUserName(selectedAssignee, selectedAssignee.id),
              image: selectedAssignee.image,
            }}
          />
        ) : (
          <UnassignedIcon />
        )}
        Assignee
        <DropdownMenuShortcut className={topLevelValueClass}>
          {selectedAssignee
            ? displayUserName(selectedAssignee, selectedAssignee.id)
            : `Unassigned`}
        </DropdownMenuShortcut>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-[15rem]">
        <ComboboxMenuItems
          menu="dropdown"
          {...rows}
          value={assigneeId}
          onChange={onSelect}
          noneLabel="Unassigned"
          emptyText="No team members yet"
          renderOption={(option) => (
            <>
              <UserAvatar size={20} user={membersById.get(option.value)} />
              {rows.renderOption(option)}
            </>
          )}
        />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

interface PrioritySubmenuProps {
  priority: Issue[`priority`]
  topLevelValueClass: string
  onSelect: (priority: Issue[`priority`]) => void
}

export function PrioritySubmenu({
  priority,
  topLevelValueClass,
  onSelect,
}: PrioritySubmenuProps) {
  // Trigger mirrors the row's priority icon: the CURRENT priority, not a
  // generic glyph (EXP-59).
  const priorityConfig = getIssuePriorityConfig(priority)
  const PriorityIcon = priorityConfig.icon

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <PriorityIcon className={priorityConfig.color} />
        Priority
        <DropdownMenuShortcut className={topLevelValueClass}>
          {priorityConfig.label}
        </DropdownMenuShortcut>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-[14rem]">
        <ComboboxMenuItems
          menu="dropdown"
          {...pickerMenuRows(priorityPickerItems(issuePriorityOptions))}
          value={priority}
          onChange={(next) => {
            // There is no none row here, so the single arm never reports null.
            if (next) {
              onSelect(next as Issue[`priority`])
            }
          }}
        />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

interface BoardSubmenuProps {
  boardId: Issue[`boardId`]
  // Team boards (sorted by name, trashed excluded upstream by the
  // boards shape). Kept a plain prop (no live query here) so this stays
  // presentational like its siblings.
  boards: Board[]
  topLevelValueClass: string
  onSelect: (boardId: string) => void
}

// EXP-57: move the issue to another board in the same team. The issue
// is renumbered in the target board (EXP-42 → ABC-17) server-side.
export function BoardSubmenu({
  boardId,
  boards,
  topLevelValueClass,
  onSelect,
}: BoardSubmenuProps) {
  const currentName = boards.find((board) => board.id === boardId)?.name
  // The `BoardPicker`'s rows (icon + colour, EXP-449/EXP-1021); the issue's
  // own board is rendered and disabled — moving it there is a no-op.
  const rows = pickerMenuRows(
    boardPickerItems(boards).map((item) => ({
      ...item,
      disabled: item.value === boardId,
    }))
  )

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <NavBoardsIcon />
        Move to board
        <DropdownMenuShortcut className={topLevelValueClass}>
          {currentName ?? `Board`}
        </DropdownMenuShortcut>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-[15rem]">
        <ComboboxMenuItems
          menu="dropdown"
          {...rows}
          value={boardId}
          onChange={(next) => {
            if (next && next !== boardId) {
              onSelect(next)
            }
          }}
          emptyText="No boards yet"
        />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

interface LabelsSubmenuProps {
  labels: Label[]
  selectedLabelIds: Set<string>
  topLevelValueClass: string
  onToggle: (labelId: string) => void
}

export function LabelsSubmenu({
  labels,
  selectedLabelIds,
  topLevelValueClass,
  onToggle,
}: LabelsSubmenuProps) {
  const labelsLabel =
    selectedLabelIds.size > 0 ? `${selectedLabelIds.size} selected` : `None`
  const rows = pickerMenuRows(labelPickerItems(labels))

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <LabelsIcon />
        Labels
        <DropdownMenuShortcut className={topLevelValueClass}>
          {labelsLabel}
        </DropdownMenuShortcut>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-[15rem]">
        <ComboboxMenuItems
          menu="dropdown"
          multiple
          {...rows}
          value={[...selectedLabelIds]}
          onChange={(next) => {
            // The host toggles ONE label at a time (two tRPC calls, add and
            // remove), so read the single changed id back off the array the
            // multi arm reports.
            const added = next.find((id) => !selectedLabelIds.has(id))
            if (added !== undefined) {
              onToggle(added)
              return
            }
            const nextIds = new Set(next)
            const removed = [...selectedLabelIds].find(
              (id) => !nextIds.has(id)
            )
            if (removed !== undefined) {
              onToggle(removed)
            }
          }}
          emptyText="No labels yet"
        />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

interface EstimateSubmenuProps {
  estimate: Issue[`estimate`]
  /** The team's scale (`teams.estimation_type`); `none` renders nothing. */
  estimation: IssueEstimation
  topLevelValueClass: string
  onSelect: (estimate: number | null) => void
}

// EXP-1074: story points on the team's scale (EXP-630) — the detail panel's
// `EstimateControl` options as menu rows; "No estimate" is the arm's own
// none row, so a clear reports `null` like the assignee's does.
export function EstimateSubmenu({
  estimate,
  estimation,
  topLevelValueClass,
  onSelect,
}: EstimateSubmenuProps) {
  if (estimation === `none`) return null
  const options = estimatePickerValues(estimate, estimation).map((value) => ({
    value: String(value),
    label: estimateLabel(value, estimation),
  }))

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <EstimateIcon />
        Estimate
        <DropdownMenuShortcut className={topLevelValueClass}>
          {estimate === null ? `None` : estimateShortLabel(estimate, estimation)}
        </DropdownMenuShortcut>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-[14rem]">
        <ComboboxMenuItems
          menu="dropdown"
          options={options}
          value={estimate === null ? null : String(estimate)}
          onChange={(next) => onSelect(parseEstimatePick(next))}
          noneLabel={NO_ESTIMATE}
        />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

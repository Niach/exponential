import { Tag, UserX } from "lucide-react"
import type { Issue, Label, Board, User } from "@/db/schema"
import { getIssuePriorityConfig, issuePriorityOptions } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusRowOption } from "@/lib/team-statuses"
import {
  StatusIcon,
  toStatusMenuOptions,
} from "@/components/issue-properties/status-dropdown"
import {
  ComboboxMenuItems,
  conceptIcon,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  UserAvatar,
  type PickerOption,
} from "@exp/ui"
import { displayUserName } from "@/lib/user-display"
import { BoardGlyph } from "@/components/board-glyph"

// EXP-957 — every submenu BODY here is the Combobox's menu arm: the rows are
// `ComboboxMenuItems` over `PickerOption`s, so the selection glyph is the
// primitive's (a trailing `ui-check`, or the circle pair on the multi arm) and
// not the menu's own radio dot / checkbox tick. The triggers stay hand-drawn:
// they mirror the ROW's current value (EXP-59), which is not a picker concern.

// EXP-687: "Move to board" draws the SAME glyph on all four clients.
const NavBoardsIcon = conceptIcon(`nav-boards`)

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
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <StatusIcon option={status} />
        Status
        <ContextMenuShortcut className={topLevelValueClass}>
          {status.name}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[14rem]">
        <ComboboxMenuItems
          menu="context"
          options={toStatusMenuOptions(options)}
          value={status.id}
          onChange={(id) => {
            const picked = options.find((option) => option.id === id)
            if (picked) {
              onSelect(picked)
            }
          }}
        />
      </ContextMenuSubContent>
    </ContextMenuSub>
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
  // The same rows the `AssigneePicker` builds: the id is the identity, the
  // name and the email are only search terms.
  const options: PickerOption[] = orderedUsers.map((user) => ({
    value: user.id,
    label: displayUserName(user, user.id),
    keywords: [displayUserName(user, user.id), user.email ?? ``],
  }))
  const usersById = new Map(orderedUsers.map((user) => [user.id, user]))

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
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
          <UserX className="size-4" />
        )}
        Assignee
        <ContextMenuShortcut className={topLevelValueClass}>
          {selectedAssignee
            ? displayUserName(selectedAssignee, selectedAssignee.id)
            : `Unassigned`}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15rem]">
        <ComboboxMenuItems
          menu="context"
          options={options}
          value={assigneeId}
          onChange={onSelect}
          noneLabel="Unassigned"
          emptyText="No team members yet"
          renderOption={(option) => (
            <>
              <UserAvatar
                size={20}
                user={{
                  id: option.value,
                  name: String(option.label),
                  image: usersById.get(option.value)?.image ?? null,
                }}
              />
              <span className="min-w-0 flex-1 truncate text-sm">
                {option.label}
              </span>
            </>
          )}
        />
      </ContextMenuSubContent>
    </ContextMenuSub>
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
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <PriorityIcon className={`size-4 ${priorityConfig.color}`} />
        Priority
        <ContextMenuShortcut className={topLevelValueClass}>
          {priorityConfig.label}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[14rem]">
        <ComboboxMenuItems
          menu="context"
          options={issuePriorityOptions}
          value={priority}
          onChange={(next) => {
            // There is no none row here, so the single arm never reports null.
            if (next) {
              onSelect(next)
            }
          }}
        />
      </ContextMenuSubContent>
    </ContextMenuSub>
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
  const boardsById = new Map(boards.map((board) => [board.id, board]))
  const currentName = boardsById.get(boardId)?.name
  const options: PickerOption[] = boards.map((board) => ({
    value: board.id,
    label: board.name,
    disabled: board.id === boardId,
  }))

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <NavBoardsIcon className="size-4" />
        Move to board
        <ContextMenuShortcut className={topLevelValueClass}>
          {currentName ?? `Board`}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15rem]">
        <ComboboxMenuItems
          menu="context"
          options={options}
          value={boardId}
          onChange={(next) => {
            if (next && next !== boardId) {
              onSelect(next)
            }
          }}
          emptyText="No boards yet"
          renderOption={(option) => {
            const board = boardsById.get(option.value)
            return (
              <>
                {board && <BoardGlyph board={board} className="size-3.5" />}
                <span className="min-w-0 flex-1 truncate text-sm">
                  {option.label}
                </span>
              </>
            )
          }}
        />
      </ContextMenuSubContent>
    </ContextMenuSub>
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
  const options: PickerOption[] = labels.map((label) => ({
    value: label.id,
    label: label.name,
    dot: label.color,
  }))

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <Tag className="size-4" />
        Labels
        <ContextMenuShortcut className={topLevelValueClass}>
          {labelsLabel}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15rem]">
        <ComboboxMenuItems
          menu="context"
          multiple
          options={options}
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
      </ContextMenuSubContent>
    </ContextMenuSub>
  )
}

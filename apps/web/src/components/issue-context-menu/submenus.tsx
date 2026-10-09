import type { Issue, Label, Board, User } from "@/db/schema"
import { getIssuePriorityConfig, issuePriorityOptions } from "@/lib/domain"
import type { StatusRowOption } from "@/lib/team-statuses"
import {
  StatusIcon,
  toStatusPickerStatuses,
} from "@/components/issue-properties/status-dropdown"
import {
  assigneePickerItems,
  boardPickerItems,
  conceptIcon,
  labelPickerItems,
  PickerItemBody,
  PickerMenuRows,
  priorityPickerItems,
  statusPickerItems,
  UserAvatar,
  type IssueMenuSlot,
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

// EXP-957 → the UI cleanup batch — every submenu here is a SLOT of the shared
// issue menu layout (`@exp/ui` `issueMenuEntries`): the row's glyph and
// current value (it mirrors the ROW, EXP-59), and a body of the typed
// pickers' own rows through `PickerMenuRows` — so a board row, a label row,
// a member row and a status row are built in exactly ONE place, and the
// selection language is the picker's (single = check, multi = highlight) in
// the dropdown and in the phone sheet alike.

const UnassignedIcon = conceptIcon(`ui-unassigned`)

export function statusSlot({
  status,
  options,
  onSelect,
}: {
  /** The RESOLVED team status row of this issue (EXP-314). */
  status: StatusRowOption
  options: StatusRowOption[]
  onSelect: (status: StatusRowOption) => void
}): IssueMenuSlot {
  return {
    // The CURRENT status, not a generic glyph (EXP-59).
    icon: <StatusIcon option={status} />,
    value: status.name,
    contentClassName: `w-[14rem]`,
    body: (
      <PickerMenuRows
        mode="single"
        items={statusPickerItems(toStatusPickerStatuses(options))}
        value={status.id}
        onChange={(id) => {
          const picked = options.find((option) => option.id === id)
          if (picked) onSelect(picked)
        }}
      />
    ),
  }
}

export function assigneeSlot({
  assigneeId,
  orderedUsers,
  selectedAssignee,
  onSelect,
}: {
  assigneeId: Issue[`assigneeId`]
  orderedUsers: User[]
  selectedAssignee: User | null
  onSelect: (userId: string | null) => void
}): IssueMenuSlot {
  // Literally the `AssigneePicker`'s rows (EXP-1021), with its avatar.
  const members = orderedUsers.map((user) => ({
    id: user.id,
    name: displayUserName(user, user.id),
    email: user.email,
    image: user.image,
  }))
  const membersById = new Map(members.map((member) => [member.id, member]))
  return {
    // The current assignee's avatar; the person placeholder when unassigned.
    icon: selectedAssignee ? (
      <UserAvatar
        size={16}
        user={{
          id: selectedAssignee.id,
          name: displayUserName(selectedAssignee, selectedAssignee.id),
          image: selectedAssignee.image,
        }}
      />
    ) : (
      UnassignedIcon
    ),
    value: selectedAssignee
      ? displayUserName(selectedAssignee, selectedAssignee.id)
      : `Unassigned`,
    contentClassName: `w-[15rem]`,
    body: (
      <PickerMenuRows
        mode="single"
        items={assigneePickerItems(members)}
        value={assigneeId}
        onChange={onSelect}
        noneLabel="Unassigned"
        onNone={() => onSelect(null)}
        emptyText="No team members yet"
        renderItem={(item) => (
          <>
            <UserAvatar size={20} user={membersById.get(item.value)} />
            <PickerItemBody item={item} />
          </>
        )}
      />
    ),
  }
}

export function prioritySlot({
  priority,
  onSelect,
}: {
  priority: Issue[`priority`]
  onSelect: (priority: Issue[`priority`]) => void
}): IssueMenuSlot {
  const config = getIssuePriorityConfig(priority)
  const PriorityIcon = config.icon
  return {
    icon: <PriorityIcon aria-hidden className={config.color} />,
    value: config.label,
    contentClassName: `w-[14rem]`,
    body: (
      <PickerMenuRows
        mode="single"
        items={priorityPickerItems(issuePriorityOptions)}
        value={priority}
        onChange={(next) => onSelect(next as Issue[`priority`])}
      />
    ),
  }
}

// EXP-57: move the issue to another board in the same team; it is renumbered
// in the target board server-side. The issue's own board is rendered and
// disabled — moving it there is a no-op.
export function boardSlot({
  boardId,
  boards,
  onSelect,
}: {
  boardId: Issue[`boardId`]
  boards: Board[]
  onSelect: (boardId: string) => void
}): IssueMenuSlot {
  return {
    value: boards.find((board) => board.id === boardId)?.name ?? `Board`,
    contentClassName: `w-[15rem]`,
    body: (
      <PickerMenuRows
        mode="single"
        items={boardPickerItems(boards).map((item) => ({
          ...item,
          disabled: item.value === boardId,
        }))}
        value={boardId}
        onChange={(next) => {
          if (next !== boardId) onSelect(next)
        }}
        emptyText="No boards yet"
      />
    ),
  }
}

export function labelsSlot({
  labels,
  selectedLabelIds,
  onToggle,
}: {
  labels: Label[]
  selectedLabelIds: Set<string>
  onToggle: (labelId: string) => void
}): IssueMenuSlot {
  return {
    value:
      selectedLabelIds.size > 0 ? `${selectedLabelIds.size} selected` : `None`,
    contentClassName: `w-[15rem]`,
    body: (
      <PickerMenuRows
        mode="multi"
        items={labelPickerItems(labels)}
        value={[...selectedLabelIds]}
        onChange={(next) => {
          // The host toggles ONE label at a time (add and remove are two
          // calls), so read the single changed id back off the array.
          const added = next.find((id) => !selectedLabelIds.has(id))
          if (added !== undefined) {
            onToggle(added)
            return
          }
          const nextIds = new Set(next)
          const removed = [...selectedLabelIds].find((id) => !nextIds.has(id))
          if (removed !== undefined) onToggle(removed)
        }}
        emptyText="No labels yet"
      />
    ),
  }
}

// EXP-1074: story points on the team's scale (EXP-630); "No estimate" is the
// none row, so a clear reports `null` like the assignee's does. A team that
// does not estimate gets no slot (the layout's `estimation` condition).
export function estimateSlot({
  estimate,
  estimation,
  onSelect,
}: {
  estimate: Issue[`estimate`]
  estimation: IssueEstimation
  onSelect: (estimate: number | null) => void
}): IssueMenuSlot {
  const items = estimatePickerValues(estimate, estimation).map((value) => ({
    value: String(value),
    label: estimateLabel(value, estimation),
  }))
  return {
    value:
      estimate === null ? `None` : estimateShortLabel(estimate, estimation),
    contentClassName: `w-[14rem]`,
    body: (
      <PickerMenuRows
        mode="single"
        items={items}
        value={estimate === null ? null : String(estimate)}
        onChange={(next) => onSelect(parseEstimatePick(next))}
        noneLabel={NO_ESTIMATE}
        onNone={() => onSelect(null)}
      />
    ),
  }
}

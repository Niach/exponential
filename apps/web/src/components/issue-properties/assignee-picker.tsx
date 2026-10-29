import {
  AssigneePicker as UiAssigneePicker,
  assigneeFromPickerValue,
  assigneePickerValue,
  Pill,
  UserAvatar,
} from "@exp/ui"
import { conceptIcon } from "@exp/ui"
import type { User } from "@/db/schema"
import { cn } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"
import type { SyncedAgent } from "@/components/team/agents-section"
import type { IssueAssignee } from "@/lib/issue-assignee"
import { visibleAgent } from "@exp/domain-contract/agent-settings"
import { useSession } from "@/hooks/use-session"

const UnassignedGlyph = conceptIcon(`ui-unassigned`)

interface AssigneePickerProps {
  disabled?: boolean
  users: User[]
  /** #EXP-1383: the issue's TEAM's agents (`useAgentRows(teamId)`), listed
   *  after the members (S-3: another member's Assistant is dropped from the
   *  rows, still read as the selected assignee). */
  agents?: readonly (Pick<SyncedAgent, `id` | `name` | `icon` | `color` | `sortOrder`> &
    Partial<Pick<SyncedAgent, `role` | `ownerId`>>)[]
  selectedUserId: string | null
  /** #EXP-1383: the agent assignee (exclusive with `selectedUserId`). */
  selectedAgentId?: string | null
  /** The pick as both columns, the cleared one null. */
  onSelect: (assignee: IssueAssignee) => void
  // Replaces the default chip button (the mobile create form renders the
  // picker as a full-width property row, the issue list its 20px avatar cell).
  trigger?: React.ReactNode
  // Extra classes for the default trigger button (the detail sidebar passes
  // its row styling).
  triggerClassName?: string
  // The list row's avatar sits at the right edge of the row, so its popover
  // hangs from the trailing edge (EXP-941).
  align?: `start` | `end`
}

// EXP-1021: the shared `AssigneePicker` (@exp/ui `picker/assignee-picker.tsx`)
// — avatar + name + email rows, an `Unassigned` row that reports `null`, and
// the primitive's selection language. This file is now only the app's half:
// the team's `User` rows adapted to the picker's members, the team's agents
// after them (#EXP-1383), and the default chip trigger.
export function AssigneePicker({
  disabled,
  users,
  agents = [],
  selectedUserId,
  selectedAgentId = null,
  onSelect,
  trigger,
  triggerClassName,
  align = `start`,
}: AssigneePickerProps) {
  const { data: authSession } = useSession()
  const me = authSession?.user?.id ?? null
  const pickable = agents.filter((agent) => visibleAgent({ role: agent.role, ownerId: agent.ownerId ?? `` }, me))
  const members = users.map((user) => ({
    id: user.id,
    name: displayUserName(user, user.id),
    email: user.email,
    image: user.image,
  }))
  const selectedAgent = selectedAgentId
    ? agents.find((agent) => agent.id === selectedAgentId)
    : undefined
  const selectedUser = selectedAgentId
    ? undefined
    : members.find((member) => member.id === selectedUserId)

  return (
    <UiAssigneePicker
      members={members}
      agents={pickable}
      allowsNone
      value={assigneePickerValue({ assigneeId: selectedUserId, assigneeAgentId: selectedAgentId })}
      onChange={(value) => onSelect(assigneeFromPickerValue(value))}
      disabled={disabled}
      mobileTitle="Assignee"
      searchPlaceholder="Search people..."
      emptyText="No users found."
      width="sm"
      align={align}
      trigger={
        trigger ?? (
          <Pill
            mode="action"
            className={cn(`max-w-full`, triggerClassName)}
            disabled={disabled}
          >
            {selectedAgent ? (
              <>
                <UserAvatar size={16} user={null} agent={selectedAgent} />
                <span className="truncate">{selectedAgent.name}</span>
              </>
            ) : selectedUser ? (
              <>
                <UserAvatar size={16} user={selectedUser} />
                <span className="truncate">{selectedUser.name}</span>
              </>
            ) : (
              <>
                <UnassignedGlyph className="size-3" />
                Assignee
              </>
            )}
          </Pill>
        )
      }
    />
  )
}

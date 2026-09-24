import { AssigneePicker as UiAssigneePicker, Pill, UserAvatar } from "@exp/ui"
import { conceptIcon } from "@exp/ui"
import type { User } from "@/db/schema"
import { cn } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"

const UnassignedGlyph = conceptIcon(`ui-unassigned`)

interface AssigneePickerProps {
  disabled?: boolean
  users: User[]
  selectedUserId: string | null
  onSelect: (userId: string | null) => void
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
// the team's `User` rows adapted to the picker's members, and the default
// chip trigger.
export function AssigneePicker({
  disabled,
  users,
  selectedUserId,
  onSelect,
  trigger,
  triggerClassName,
  align = `start`,
}: AssigneePickerProps) {
  const members = users.map((user) => ({
    id: user.id,
    name: displayUserName(user, user.id),
    email: user.email,
    image: user.image,
  }))
  const selectedUser = members.find((member) => member.id === selectedUserId)

  return (
    <UiAssigneePicker
      members={members}
      allowsNone
      value={selectedUserId}
      onChange={onSelect}
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
            {selectedUser ? (
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

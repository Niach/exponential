import { Combobox, Pill, UserAvatar, type PickerOption } from "@exp/ui"
import { User as UserIcon } from "lucide-react"
import type { User } from "@/db/schema"
import { cn } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"

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

// EXP-941: the shared `Combobox` — "Unassign" is the primitive's `noneLabel`
// row (it reports `null`, so no `__unassign__` sentinel exists any more) and
// the picked row wears the primitive's trailing check.
export function AssigneePicker({
  disabled,
  users,
  selectedUserId,
  onSelect,
  trigger,
  triggerClassName,
  align = `start`,
}: AssigneePickerProps) {
  const options: PickerOption[] = users.map((user) => ({
    value: user.id,
    label: displayUserName(user, user.id),
    keywords: [displayUserName(user, user.id), user.email ?? ``],
  }))
  const usersById = new Map(users.map((user) => [user.id, user]))
  const selectedUser = selectedUserId ? usersById.get(selectedUserId) : undefined

  return (
    <Combobox
      options={options}
      value={selectedUserId}
      onChange={onSelect}
      noneLabel="Unassign"
      disabled={disabled}
      mobileTitle="Assignee"
      placeholder="Search people..."
      emptyText="No users found."
      width="sm"
      align={align}
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
      renderTrigger={() =>
        trigger ?? (
          <Pill
            mode="action"
            className={cn(`max-w-full`, triggerClassName)}
            disabled={disabled}
          >
            {selectedUser ? (
              <>
                <UserAvatar
                  size={16}
                  user={{
                    id: selectedUser.id,
                    name: displayUserName(selectedUser, selectedUser.id),
                    image: selectedUser.image,
                  }}
                />
                <span className="truncate">
                  {displayUserName(selectedUser, selectedUser.id)}
                </span>
              </>
            ) : (
              <>
                <UserIcon className="size-3" />
                Assignee
              </>
            )}
          </Pill>
        )
      }
    />
  )
}

import { useState } from "react"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  Button,
  UserAvatar,
} from "@exp/ui"
import { User as UserIcon, X } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import type { User } from "@/db/schema"
interface AssigneeDropdownProps {
  issueId: string
  assigneeId: string | null
  users: User[]
  userMap: Map<string, User>
  disabled?: boolean
  // Solo teams have no one else to assign to: render the avatar/placeholder
  // as a static, non-interactive cell instead of a pointless dropdown.
  readOnly?: boolean
}

export function AssigneeDropdown({
  issueId,
  assigneeId,
  users,
  userMap,
  disabled,
  readOnly,
}: AssigneeDropdownProps) {
  const [open, setOpen] = useState(false)
  const assignee = assigneeId ? userMap.get(assigneeId) : undefined

  const people = users

  const avatarVisual = assignee ? (
    <UserAvatar size={20} user={assignee} />
  ) : (
    <div className="size-5 rounded-full border border-dashed border-border flex items-center justify-center">
      <UserIcon className="size-2.5 text-muted-foreground/50" />
    </div>
  )

  if (readOnly) {
    return (
      <div className="flex h-5 w-5 items-center justify-center">
        {avatarVisual}
      </div>
    )
  }

  const handleSelect = async (userId: string | null) => {
    setOpen(false)
    await trpc.issues.update.mutate({ id: issueId, assigneeId: userId })
  }

  const renderUser = (user: User) => (
    <CommandItem
      key={user.id}
      value={user.id}
      keywords={[user.name]}
      onSelect={() => handleSelect(user.id)}
      className="flex items-center gap-2"
    >
      <UserAvatar size={20} user={user} />
      <span className="truncate text-sm">{user.name}</span>
    </CommandItem>
  )

  return (
    <Popover open={open} onOpenChange={disabled ? undefined : setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          className="h-5 w-5 p-0"
          disabled={disabled}
        >
          {avatarVisual}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[14rem] p-0" align="end">
        <Command>
          <CommandInput placeholder="Search people..." />
          <CommandList>
            <CommandEmpty>No users found.</CommandEmpty>
            <CommandGroup>
              {assigneeId && (
                <CommandItem
                  value="__unassign__"
                  onSelect={() => handleSelect(null)}
                  className="flex items-center gap-2"
                >
                  <X className="size-3.5 text-muted-foreground" />
                  <span className="text-sm">Unassign</span>
                </CommandItem>
              )}
              {people.map(renderUser)}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

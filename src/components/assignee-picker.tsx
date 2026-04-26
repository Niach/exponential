import { useState } from "react"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { User as UserIcon, X } from "lucide-react"
import type { User } from "@/db/schema"
import { getInitials } from "@/lib/utils"

interface AssigneePickerProps {
  disabled?: boolean
  users: User[]
  selectedUserId: string | null
  onSelect: (userId: string | null) => void
}

export function AssigneePicker({
  disabled,
  users,
  selectedUserId,
  onSelect,
}: AssigneePickerProps) {
  const [open, setOpen] = useState(false)

  const selectedUser = selectedUserId
    ? users.find((u) => u.id === selectedUserId)
    : undefined

  return (
    <Popover
      open={disabled ? false : open}
      onOpenChange={(nextOpen) => {
        if (disabled) {
          return
        }

        setOpen(nextOpen)
      }}
    >
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
          disabled={disabled}
        >
          {selectedUser ? (
            <>
              <Avatar className="size-4">
                {selectedUser.image && (
                  <AvatarImage
                    src={selectedUser.image}
                    alt={selectedUser.name}
                  />
                )}
                <AvatarFallback className="text-[8px]">
                  {getInitials(selectedUser.name)}
                </AvatarFallback>
              </Avatar>
              <span className="max-w-[100px] truncate">
                {selectedUser.name}
              </span>
            </>
          ) : (
            <>
              <UserIcon className="size-3" />
              Assignee
            </>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[220px] p-0" align="start">
        <Command>
          <CommandInput placeholder="Search people..." />
          <CommandList>
            <CommandEmpty>No users found.</CommandEmpty>
            <CommandGroup>
              {selectedUserId && (
                <CommandItem
                  value="__unassign__"
                  onSelect={() => {
                    onSelect(null)
                    setOpen(false)
                  }}
                  className="flex items-center gap-2"
                >
                  <X className="size-3.5 text-muted-foreground" />
                  <span className="text-sm">Unassign</span>
                </CommandItem>
              )}
              {users.map((user) => (
                <CommandItem
                  key={user.id}
                  value={user.name}
                  onSelect={() => {
                    onSelect(user.id)
                    setOpen(false)
                  }}
                  className="flex items-center gap-2"
                >
                  <Avatar className="size-5">
                    {user.image && (
                      <AvatarImage src={user.image} alt={user.name} />
                    )}
                    <AvatarFallback className="text-[9px]">
                      {getInitials(user.name)}
                    </AvatarFallback>
                  </Avatar>
                  <span className="truncate text-sm">{user.name}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

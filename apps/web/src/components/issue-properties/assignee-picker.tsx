import { useState } from "react"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Pill } from "@/components/ui/pill"
import { User as UserIcon, X } from "lucide-react"
import type { User } from "@/db/schema"
import { cn, getInitials } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"

interface AssigneePickerProps {
  disabled?: boolean
  users: User[]
  selectedUserId: string | null
  onSelect: (userId: string | null) => void
  // Replaces the default chip button (the mobile create form renders the
  // picker as a full-width property row).
  trigger?: React.ReactNode
  // Extra classes for the default trigger button (the detail sidebar passes
  // its row styling).
  triggerClassName?: string
}

export function AssigneePicker({
  disabled,
  users,
  selectedUserId,
  onSelect,
  trigger,
  triggerClassName,
}: AssigneePickerProps) {
  const [open, setOpen] = useState(false)

  const selectedUser = selectedUserId
    ? users.find((u) => u.id === selectedUserId)
    : undefined

  return (
    <MobilePopover
      open={disabled ? false : open}
      onOpenChange={(nextOpen) => {
        if (disabled) {
          return
        }

        setOpen(nextOpen)
      }}
    >
      <MobilePopoverTrigger asChild>
        {trigger ?? (
          <Pill
            mode="action"
            // max-w-full instead of a fixed name cap (EXP-427): the name
            // truncates at the container edge, not at an arbitrary width.
            className={cn(`max-w-full`, triggerClassName)}
            disabled={disabled}
          >
            {selectedUser ? (
              <>
                <Avatar className="size-4">
                  {selectedUser.image && (
                    <AvatarImage
                      src={selectedUser.image}
                      alt={displayUserName(selectedUser, selectedUser.id)}
                    />
                  )}
                  <AvatarFallback className="text-[0.5rem]">
                    {getInitials(
                      displayUserName(selectedUser, selectedUser.id)
                    )}
                  </AvatarFallback>
                </Avatar>
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
        )}
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[14rem] p-0"
        align="start"
        mobileTitle="Assignee"
      >
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
              {users.map((user) => {
                const name = displayUserName(user, user.id)
                return (
                  <CommandItem
                    key={user.id}
                    value={user.id}
                    keywords={[name, user.email ?? ``]}
                    onSelect={() => {
                      onSelect(user.id)
                      setOpen(false)
                    }}
                    className="flex items-center gap-2"
                  >
                    <Avatar className="size-5">
                      {user.image && (
                        <AvatarImage src={user.image} alt={name} />
                      )}
                      <AvatarFallback className="text-[0.5625rem]">
                        {getInitials(name)}
                      </AvatarFallback>
                    </Avatar>
                    <span className="truncate text-sm">{name}</span>
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}

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
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { User as UserIcon, X } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import type { User } from "@/db/schema"
import { getInitials } from "@/lib/utils"

interface AssigneeDropdownProps {
  issueId: string
  assigneeId: string | null
  users: User[]
  userMap: Map<string, User>
}

export function AssigneeDropdown({
  issueId,
  assigneeId,
  users,
  userMap,
}: AssigneeDropdownProps) {
  const [open, setOpen] = useState(false)
  const assignee = assigneeId ? userMap.get(assigneeId) : undefined

  const handleSelect = async (userId: string | null) => {
    setOpen(false)
    await trpc.issues.update.mutate({ id: issueId, assigneeId: userId })
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" className="h-5 w-5 p-0">
          {assignee ? (
            <Avatar className="size-5">
              {assignee.image && (
                <AvatarImage src={assignee.image} alt={assignee.name} />
              )}
              <AvatarFallback className="text-[10px]">
                {getInitials(assignee.name)}
              </AvatarFallback>
            </Avatar>
          ) : (
            <div className="size-5 rounded-full border border-dashed border-border flex items-center justify-center">
              <UserIcon className="size-2.5 text-muted-foreground/50" />
            </div>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[220px] p-0" align="end">
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
              {users.map((user) => (
                <CommandItem
                  key={user.id}
                  value={user.name}
                  onSelect={() => handleSelect(user.id)}
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

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
import { Bot, User as UserIcon, X } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import type { User } from "@/db/schema"
import { getInitials } from "@/lib/utils"

interface AssigneeDropdownProps {
  issueId: string
  assigneeId: string | null
  users: User[]
  userMap: Map<string, User>
  disabled?: boolean
}

export function AssigneeDropdown({
  issueId,
  assigneeId,
  users,
  userMap,
  disabled,
}: AssigneeDropdownProps) {
  const [open, setOpen] = useState(false)
  const assignee = assigneeId ? userMap.get(assigneeId) : undefined

  const people = users.filter((u) => !u.isAgent)
  const agents = users.filter((u) => u.isAgent)

  const handleSelect = async (userId: string | null) => {
    setOpen(false)
    await trpc.issues.update.mutate({ id: issueId, assigneeId: userId })
  }

  const renderUser = (user: User) => (
    <CommandItem
      key={user.id}
      value={`${user.isAgent ? `agent ` : ``}${user.name}`}
      onSelect={() => handleSelect(user.id)}
      className="flex items-center gap-2"
    >
      <Avatar className="size-5">
        {user.image && <AvatarImage src={user.image} alt={user.name} />}
        <AvatarFallback className="text-[0.5625rem]">
          {user.isAgent ? (
            <Bot className="size-3" />
          ) : (
            getInitials(user.name)
          )}
        </AvatarFallback>
      </Avatar>
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
          {assignee ? (
            <Avatar className="size-5">
              {assignee.image && (
                <AvatarImage src={assignee.image} alt={assignee.name} />
              )}
              <AvatarFallback className="text-[0.625rem]">
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
      <PopoverContent className="w-[14rem] p-0" align="end">
        <Command>
          <CommandInput placeholder="Search people..." />
          <CommandList>
            <CommandEmpty>No users found.</CommandEmpty>
            <CommandGroup heading={agents.length > 0 ? `People` : undefined}>
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
            {agents.length > 0 && (
              <CommandGroup heading="Agents">
                {agents.map(renderUser)}
                <div className="px-2 py-1 text-[0.6875rem] text-muted-foreground">
                  Assigning to an agent creates a plan request.
                </div>
              </CommandGroup>
            )}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

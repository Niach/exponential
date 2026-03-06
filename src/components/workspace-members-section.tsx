import { Crown, MoreHorizontal, Shield, UserMinus } from "lucide-react"
import type { User, WorkspaceMember } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

export function WorkspaceMembersSection({
  currentUserId,
  isOwner,
  members,
  userMap,
}: {
  currentUserId: string | undefined
  isOwner: boolean
  members: WorkspaceMember[]
  userMap: Map<string, User>
}) {
  const ownerCount = members.filter((member) => member.role === `owner`).length

  const handleUpdateRole = async (
    memberId: string,
    role: `owner` | `member`
  ) => {
    await trpc.workspaceMembers.updateRole.mutate({ memberId, role })
  }

  const handleRemove = async (memberId: string) => {
    await trpc.workspaceMembers.remove.mutate({ memberId })
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Members</CardTitle>
        <CardDescription>
          {members.length} member{members.length !== 1 ? `s` : ``} in this
          workspace
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {members.map((member) => {
            const isSelf = member.userId === currentUserId
            const user = userMap.get(member.userId)
            const displayName = user?.name ?? member.userId
            const roleIcon =
              member.role === `owner` ? (
                <Crown className="h-3.5 w-3.5" />
              ) : (
                <Shield className="h-3.5 w-3.5" />
              )

            return (
              <div
                key={member.id}
                className="flex items-center justify-between rounded-md border px-3 py-2"
              >
                <div className="flex items-center gap-3">
                  <Avatar className="h-8 w-8">
                    <AvatarFallback className="text-xs">
                      {getInitials(displayName)}
                    </AvatarFallback>
                  </Avatar>
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">
                        {displayName}
                        {isSelf && (
                          <span className="text-muted-foreground"> (you)</span>
                        )}
                      </span>
                      <Badge
                        variant="secondary"
                        className="flex items-center gap-1"
                      >
                        {roleIcon}
                        {member.role}
                      </Badge>
                    </div>
                    {user?.email && (
                      <div className="text-xs text-muted-foreground">
                        {user.email}
                      </div>
                    )}
                  </div>
                </div>

                {(isOwner || isSelf) &&
                  !(isSelf && member.role === `owner` && ownerCount <= 1) && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="icon" className="h-7 w-7">
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {isOwner && !isSelf && (
                          <>
                            <DropdownMenuItem
                              onClick={() => handleUpdateRole(member.id, `owner`)}
                              disabled={member.role === `owner`}
                            >
                              <Crown className="mr-2 h-4 w-4" />
                              Make owner
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              onClick={() => handleUpdateRole(member.id, `member`)}
                              disabled={member.role === `member`}
                            >
                              <Shield className="mr-2 h-4 w-4" />
                              Make member
                            </DropdownMenuItem>
                          </>
                        )}
                        {isSelf ? (
                          <DropdownMenuItem
                            onClick={() => handleRemove(member.id)}
                            className="text-destructive"
                          >
                            <UserMinus className="mr-2 h-4 w-4" />
                            Leave workspace
                          </DropdownMenuItem>
                        ) : (
                          isOwner && (
                            <DropdownMenuItem
                              onClick={() => handleRemove(member.id)}
                              className="text-destructive"
                            >
                              <UserMinus className="mr-2 h-4 w-4" />
                              Remove member
                            </DropdownMenuItem>
                          )
                        )}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  )}
              </div>
            )
          })}
        </div>
      </CardContent>
    </Card>
  )
}

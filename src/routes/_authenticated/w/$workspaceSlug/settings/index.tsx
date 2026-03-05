import { useState, useMemo } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { authClient } from "@/lib/auth-client"
import { trpc } from "@/lib/trpc-client"
import {
  workspaceCollection,
  workspaceMemberCollection,
  workspaceInviteCollection,
  userCollection,
} from "@/lib/collections"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Separator } from "@/components/ui/separator"
import {
  Copy,
  Link as LinkIcon,
  Loader2,
  MoreHorizontal,
  Shield,
  Crown,
  Trash2,
  UserMinus,
  Check,
} from "lucide-react"

export const Route = createFileRoute(
  `/_authenticated/w/$workspaceSlug/settings/`
)({
  component: WorkspaceSettings,
})

function WorkspaceSettings() {
  const { workspaceSlug } = Route.useParams()
  const { data: session } = authClient.useSession()

  const { data: workspaces } = useLiveQuery((q) =>
    q
      .from({ workspaces: workspaceCollection })
      .where(({ workspaces }) => eq(workspaces.slug, workspaceSlug))
  )
  const workspace = workspaces?.[0]

  const { data: members } = useLiveQuery(
    (q) =>
      workspace
        ? q
            .from({ members: workspaceMemberCollection })
            .where(({ members }) => eq(members.workspaceId, workspace.id))
        : undefined,
    [workspace?.id]
  )

  const { data: allUsers } = useLiveQuery((q) =>
    q.from({ users: userCollection })
  )
  const userMap = useMemo(
    () =>
      new Map(
        (allUsers ?? []).map((u) => [u.id, { id: u.id, name: u.name, email: u.email }])
      ),
    [allUsers]
  )

  const currentMember = members?.find((m) => m.userId === session?.user?.id)
  const isOwner = currentMember?.role === `owner`

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <h1 className="text-2xl font-bold">Workspace Settings</h1>
        <p className="text-sm text-muted-foreground">
          Manage members and invites for {workspace?.name}
        </p>
      </div>

      <Separator />

      {workspace && isOwner && <InviteSection workspaceId={workspace.id} />}

      <MembersSection
        members={members ?? []}
        userMap={userMap}
        currentUserId={session?.user?.id}
        isOwner={isOwner}
      />
    </div>
  )
}

function InviteSection({ workspaceId }: { workspaceId: string }) {
  const [generating, setGenerating] = useState(false)
  const [inviteUrl, setInviteUrl] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const { data: allInvites } = useLiveQuery(
    (q) =>
      q
        .from({ invites: workspaceInviteCollection })
        .where(({ invites }) => eq(invites.workspaceId, workspaceId)),
    [workspaceId]
  )
  const invites = allInvites?.filter((i) => !i.acceptedAt) ?? []

  const handleGenerate = async () => {
    setGenerating(true)
    try {
      const { token } = await trpc.workspaceInvites.create.mutate({
        workspaceId,
      })
      setInviteUrl(`${window.location.origin}/invite/${token}`)
    } catch {
      // Error handled silently
    } finally {
      setGenerating(false)
    }
  }

  const handleCopy = async () => {
    if (inviteUrl) {
      await navigator.clipboard.writeText(inviteUrl)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    }
  }

  const handleRevoke = async (id: string) => {
    await trpc.workspaceInvites.revoke.mutate({ id })
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Invite Members</CardTitle>
        <CardDescription>
          Generate an invite link to share with people you want to add
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {inviteUrl && (
          <div className="flex items-center gap-2">
            <Input value={inviteUrl} readOnly className="text-xs font-mono" />
            <Button
              variant="outline"
              size="icon"
              onClick={handleCopy}
              className="shrink-0"
            >
              {copied ? (
                <Check className="h-4 w-4" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
            </Button>
          </div>
        )}

        <Button onClick={handleGenerate} disabled={generating}>
          {generating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          <LinkIcon className="mr-2 h-4 w-4" />
          Generate invite link
        </Button>

        {invites.length > 0 && (
          <div className="space-y-2 pt-2">
            <div className="text-sm font-medium text-muted-foreground">
              Pending invites
            </div>
            {invites.map((invite) => (
              <div
                key={invite.id}
                className="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
              >
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">{invite.role}</Badge>
                  <span className="text-muted-foreground">
                    Expires{` `}
                    {new Date(invite.expiresAt).toLocaleDateString()}
                  </span>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => handleRevoke(invite.id)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function MembersSection({
  members,
  userMap,
  currentUserId,
  isOwner,
}: {
  members: Array<{
    id: string
    userId: string
    role: `owner` | `member`
    workspaceId: string
  }>
  userMap: Map<string, { id: string; name: string; email: string }>
  currentUserId: string | undefined
  isOwner: boolean
}) {
  const handleUpdateRole = async (
    memberId: string,
    role: `owner` | `member`
  ) => {
    await trpc.workspaceMembers.updateRole.mutate({ memberId, role })
  }

  const handleRemove = async (memberId: string) => {
    await trpc.workspaceMembers.remove.mutate({ memberId })
  }

  const ownerCount = members.filter((m) => m.role === `owner`).length

  const roleIcon = (role: string) => {
    switch (role) {
      case `owner`:
        return <Crown className="h-3.5 w-3.5" />
      default:
        return <Shield className="h-3.5 w-3.5" />
    }
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
            const initials = displayName.slice(0, 2).toUpperCase()
            return (
              <div
                key={member.id}
                className="flex items-center justify-between rounded-md border px-3 py-2"
              >
                <div className="flex items-center gap-3">
                  <Avatar className="h-8 w-8">
                    <AvatarFallback className="text-xs">
                      {initials}
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
                        {roleIcon(member.role)}
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
                              onClick={() =>
                                handleUpdateRole(member.id, `owner`)
                              }
                              disabled={member.role === `owner`}
                            >
                              <Crown className="mr-2 h-4 w-4" />
                              Make owner
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              onClick={() =>
                                handleUpdateRole(member.id, `member`)
                              }
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

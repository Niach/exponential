import { useEffect, useState } from "react"
import {
  Check,
  Copy,
  Crown,
  Link as LinkIcon,
  Loader2,
  MoreHorizontal,
  Shield,
  Trash2,
  UserMinus,
} from "lucide-react"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { toast } from "sonner"
import type { User, WorkspaceMember } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"
import { useWorkspaceInvites } from "@/hooks/use-workspace-data"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { getInitials } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"
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
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { UpgradeDialog } from "@/components/upgrade-dialog"

export function WorkspaceMembersSection({
  currentUserId,
  canManageMembers,
  members,
  userMap,
  workspaceId,
  showInvite,
  solo = false,
}: {
  currentUserId: string | undefined
  // Owner OR instance admin (mirrors assertCanManageMembers). Gates the
  // role-change + remove-member controls; self "Leave" stays available to all.
  canManageMembers: boolean
  members: WorkspaceMember[]
  userMap: Map<string, User>
  workspaceId?: string
  showInvite?: boolean
  // When the user is solo, frame this as the deliberate "invite a teammate"
  // path (inviting a second human is what reveals the workspace concept).
  solo?: boolean
}) {
  const ownerCount = members.filter((member) => member.role === `owner`).length

  const handleUpdateRole = async (
    memberId: string,
    role: `owner` | `member`
  ) => {
    await trpc.workspaceMembers.updateRole.mutate({ memberId, role })
  }

  const handleRemove = async (memberId: string, isSelf: boolean) => {
    await trpc.workspaceMembers.remove.mutate({ memberId })
    invalidateBillingCache()
    // Leaving the workspace you're looking at changes every shape's where
    // clause and drops your read access — hard-navigate home so all Electric
    // collections restart cleanly.
    if (isSelf) {
      window.location.assign(`/w/default`)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">
          {solo ? `Invite teammates` : `Members`}
        </CardTitle>
        <CardDescription>
          {solo
            ? `Invite someone to collaborate. Shared projects unlock team features.`
            : `${members.length} member${members.length !== 1 ? `s` : ``} in this workspace`}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          {members.map((member) => {
            const isSelf = member.userId === currentUserId
            const user = userMap.get(member.userId)
            const displayName = displayUserName(user, member.userId)
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

                {(canManageMembers || isSelf) &&
                  !(isSelf && member.role === `owner` && ownerCount <= 1) && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          aria-label={`Member actions for ${displayName}`}
                        >
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {canManageMembers && !isSelf && (
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
                            onClick={() => handleRemove(member.id, true)}
                            className="text-destructive"
                          >
                            <UserMinus className="mr-2 h-4 w-4" />
                            Leave workspace
                          </DropdownMenuItem>
                        ) : (
                          canManageMembers && (
                            <DropdownMenuItem
                              onClick={() => handleRemove(member.id, false)}
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

        {showInvite && workspaceId && (
          <>
            <Separator />
            <InviteControls workspaceId={workspaceId} />
          </>
        )}
      </CardContent>
    </Card>
  )
}

function InviteControls({ workspaceId }: { workspaceId: string }) {
  const [copied, setCopied] = useState(false)
  const [generating, setGenerating] = useState(false)
  const [inviteUrl, setInviteUrl] = useState<string | null>(null)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    pro: string | null
    business: string | null
    businessYearly: string | null
  }>({ pro: null, business: null, businessYearly: null })
  const invites = useWorkspaceInvites(workspaceId).filter(
    (invite) => !invite.acceptedAt
  )

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        pro: config.creemProProductId,
        business: config.creemBusinessProductId,
        businessYearly: config.creemBusinessYearlyProductId,
      })
    })
  }, [])

  const handleGenerate = async () => {
    setGenerating(true)

    try {
      const { token } = await trpc.workspaceInvites.create.mutate(
        { workspaceId },
        // The plan-limit (PRECONDITION_FAILED) case opens the upgrade dialog;
        // the global mutation-error toast would be redundant noise on top of it.
        { context: { skipErrorToast: true } }
      )

      setInviteUrl(`${window.location.origin}/invite/${token}`)
    } catch (err) {
      if (isPlanLimitError(err)) {
        setUpgradeOpen(true)
      } else {
        toast.error(`Couldn't create the invite`)
      }
    } finally {
      setGenerating(false)
    }
  }

  const handleCopy = async () => {
    if (!inviteUrl) {
      return
    }

    await navigator.clipboard.writeText(inviteUrl)
    setCopied(true)
    toast.success(`Invite link copied`)

    setTimeout(() => setCopied(false), 2000)
  }

  const handleRevoke = async (id: string) => {
    await trpc.workspaceInvites.revoke.mutate({ id })
  }

  return (
    <>
      <div className="space-y-4">
        <div>
          <div className="text-sm font-medium">Invite Members</div>
          <div className="text-xs text-muted-foreground">
            Generate an invite link to share with people you want to add
          </div>
        </div>

        {inviteUrl && (
          <div className="flex items-center gap-2">
            <Input
              value={inviteUrl}
              readOnly
              className="text-xs font-mono"
              data-testid="invite-url-input"
            />
            <Button
              variant="outline"
              size="icon"
              onClick={handleCopy}
              className="shrink-0"
              aria-label="Copy invite URL"
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
                  aria-label={`Revoke invite ${invite.id}`}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </div>

      <UpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        title="Out of seats"
        description="Everyone on your plan's seats is already in this workspace. Add seats to invite more teammates."
        proProductId={productIds.pro}
        businessProductId={productIds.business}
        businessYearlyProductId={productIds.businessYearly}
        workspaceId={workspaceId}
      />
    </>
  )
}

import { useEffect, useState } from "react"
import {
  Check,
  Copy,
  Crown,
  Link as LinkIcon,
  LoaderCircle,
  Mail,
  Ellipsis,
  ShieldCheck,
  Trash2,
} from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { toast } from "sonner"
import type { User, TeamMember } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"
import { useTeamInvites } from "@/hooks/use-team-data"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { getInitials } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { UpgradeDialog } from "@/components/upgrade-dialog"

// EXP-687: leaving a team is a sign-out, removing someone is a user-minus —
// both red, both the same concepts the natives draw.
const NavSignOutIcon = conceptIcon(`nav-sign-out`)
const UiRemoveMemberIcon = conceptIcon(`ui-remove-member`)

export function TeamMembersSection({
  currentUserId,
  canManageMembers,
  members,
  userMap,
  teamId,
  showInvite,
}: {
  currentUserId: string | undefined
  // Owner OR instance admin (mirrors assertCanManageMembers). Gates the
  // role-change + remove-member controls; self "Leave" stays available to all.
  canManageMembers: boolean
  members: TeamMember[]
  userMap: Map<string, User>
  teamId?: string
  showInvite?: boolean
}) {
  const ownerCount = members.filter((member) => member.role === `owner`).length
  // Removal confirms first (REV-50): losing team access is instant and has
  // no undo, and the menu stacks the destructive item right under the
  // role toggles — matching every other destructive settings action.
  const [removeTarget, setRemoveTarget] = useState<{
    memberId: string
    isSelf: boolean
    displayName: string
  } | null>(null)
  const [removing, setRemoving] = useState(false)

  const handleUpdateRole = async (
    memberId: string,
    role: `owner` | `member`
  ) => {
    await trpc.teamMembers.updateRole.mutate({ memberId, role })
  }

  const handleRemove = async () => {
    if (!removeTarget) return
    setRemoving(true)
    try {
      await trpc.teamMembers.remove.mutate({ memberId: removeTarget.memberId })
      invalidateBillingCache()
      // Leaving the team you're looking at changes every shape's where
      // clause and drops your read access — hard-navigate home so all Electric
      // collections restart cleanly.
      if (removeTarget.isSelf) {
        window.location.assign(`/t/default`)
        return
      }
      setRemoveTarget(null)
    } finally {
      setRemoving(false)
    }
  }

  return (
    <div>
      <GlassSectionHeader label="Members" />
      <div className="space-y-4">
        <div className="space-y-2">
          {members.map((member) => {
            const isSelf = member.userId === currentUserId
            const user = userMap.get(member.userId)
            const displayName = displayUserName(user, member.userId)
            const roleIcon =
              member.role === `owner` ? (
                <Crown className="size-3" />
              ) : (
                <ShieldCheck className="size-3" />
              )

            return (
              <GlassRow
                key={member.id}
                className="justify-between gap-3 px-3 py-2"
              >
                <div className="flex min-w-0 items-center gap-3">
                  <Avatar className="h-8 w-8 shrink-0">
                    {user?.image && <AvatarImage src={user.image} />}
                    <AvatarFallback className="text-xs">
                      {getInitials(displayName)}
                    </AvatarFallback>
                  </Avatar>
                  <div className="min-w-0">
                    <div className="flex min-w-0 items-center gap-3">
                      <span className="truncate text-sm font-medium">
                        {displayName}
                        {isSelf && (
                          <span className="text-muted-foreground"> (you)</span>
                        )}
                      </span>
                      <Pill leading={roleIcon}>{member.role}</Pill>
                    </div>
                    {user?.email && user.email !== displayName && (
                      <div className="truncate text-xs text-muted-foreground">
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
                          variant="glass"
                          size="icon-sm"
                          aria-label={`Member actions for ${displayName}`}
                        >
                          <Ellipsis />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {canManageMembers && !isSelf && (
                          <>
                            {member.role !== `owner` && (
                              <DropdownMenuItem
                                onClick={() =>
                                  handleUpdateRole(member.id, `owner`)
                                }
                              >
                                <Crown className="mr-2 h-4 w-4" />
                                Make owner
                              </DropdownMenuItem>
                            )}
                            {member.role !== `member` && (
                              <DropdownMenuItem
                                onClick={() =>
                                  handleUpdateRole(member.id, `member`)
                                }
                              >
                                <ShieldCheck className="mr-2 h-4 w-4" />
                                Make member
                              </DropdownMenuItem>
                            )}
                          </>
                        )}
                        {isSelf ? (
                          <DropdownMenuItem
                            onClick={() =>
                              setRemoveTarget({
                                memberId: member.id,
                                isSelf: true,
                                displayName,
                              })
                            }
                            variant="destructive"
                          >
                            <NavSignOutIcon className="mr-2 h-4 w-4" />
                            Leave team
                          </DropdownMenuItem>
                        ) : (
                          canManageMembers && (
                            <DropdownMenuItem
                              onClick={() =>
                                setRemoveTarget({
                                  memberId: member.id,
                                  isSelf: false,
                                  displayName,
                                })
                              }
                              variant="destructive"
                            >
                              <UiRemoveMemberIcon className="mr-2 h-4 w-4" />
                              Remove member
                            </DropdownMenuItem>
                          )
                        )}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  )}
              </GlassRow>
            )
          })}
        </div>

        {showInvite && teamId && (
          <>
            <Separator />
            <InviteControls teamId={teamId} />
          </>
        )}
      </div>

      <Dialog
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open && !removing) setRemoveTarget(null)
        }}
      >
        <DialogContent mobile="alert" className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>
              {removeTarget?.isSelf ? `Leave team` : `Remove member`}
            </DialogTitle>
            <DialogDescription>
              {removeTarget?.isSelf
                ? `Leave this team? You lose access to its boards and issues immediately and need a new invite to rejoin.`
                : `Remove ${removeTarget?.displayName} from the team? They lose access to its boards and issues immediately.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              disabled={removing}
              onClick={() => setRemoveTarget(null)}
            />
            <Button
              variant="destructive"
              disabled={removing}
              onClick={() => void handleRemove()}
            >
              {removing && <LoaderCircle className="animate-spin" />}
              {removeTarget?.isSelf ? `Leave team` : `Remove`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function InviteControls({ teamId }: { teamId: string }) {
  const [copied, setCopied] = useState(false)
  const [generating, setGenerating] = useState(false)
  const [inviteUrl, setInviteUrl] = useState<string | null>(null)
  const [email, setEmail] = useState(``)
  const [sending, setSending] = useState(false)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    team: string | null
    teamYearly: string | null
  }>({ team: null, teamYearly: null })
  const invites = useTeamInvites(teamId).filter(
    (invite) => !invite.acceptedAt
  )

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        team: config.creemTeamProductId,
        teamYearly: config.creemTeamYearlyProductId,
      })
    })
  }, [])

  const handleGenerate = async () => {
    setGenerating(true)

    try {
      const { token } = await trpc.teamInvites.create.mutate(
        { teamId },
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

  // Invite by email (EXP-188): the server persists the address on the invite
  // and mails the link itself. Delivery is best-effort — when no transport is
  // configured (or the send fails) we fall back to showing the link so the
  // owner can share it by hand.
  const handleSendEmail = async () => {
    const to = email.trim()
    if (!to) return
    setSending(true)

    try {
      const { token, emailDelivered } = await trpc.teamInvites.create.mutate(
        { teamId, email: to },
        // The plan-limit (PRECONDITION_FAILED) case opens the upgrade dialog;
        // the global mutation-error toast would be redundant noise on top of it.
        { context: { skipErrorToast: true } }
      )

      if (emailDelivered) {
        toast.success(`Invite sent to ${to}`)
        setEmail(``)
      } else {
        setInviteUrl(`${window.location.origin}/invite/${token}`)
        toast.error(
          `Couldn't email the invite. Copy the link below and share it instead.`
        )
      }
    } catch (err) {
      if (isPlanLimitError(err)) {
        setUpgradeOpen(true)
      } else {
        toast.error(`Couldn't create the invite`)
      }
    } finally {
      setSending(false)
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
    await trpc.teamInvites.revoke.mutate({ id })
  }

  return (
    <>
      <div className="space-y-4">
        <div>
          <div className="text-sm font-medium">Invite Members</div>
          <div className="text-xs text-muted-foreground">
            Send an invite by email, or generate a link to share yourself
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="teammate@example.com"
            aria-label="Invite email address"
          />
          <Button
            className="shrink-0"
            onClick={handleSendEmail}
            disabled={sending || !email.trim()}
          >
            {sending ? (
              <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <Mail className="mr-2 h-4 w-4" />
            )}
            Send invite
          </Button>
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

        <Button
          variant="outline"
          onClick={handleGenerate}
          disabled={generating}
        >
          {generating && <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />}
          <LinkIcon className="mr-2 h-4 w-4" />
          Generate invite link
        </Button>

        {invites.length > 0 && (
          <div className="pt-2">
            <GlassSectionHeader label="Pending invites" />
            {invites.map((invite) => (
              <GlassRow
                key={invite.id}
                className="mb-2 justify-between px-3 py-2 text-sm last:mb-0"
              >
                <div className="flex min-w-0 items-center gap-3">
                  <Pill leading={<Mail className="size-3" />}>
                    {invite.role}
                  </Pill>
                  {invite.email && (
                    <span className="min-w-0 truncate font-medium">
                      {invite.email}
                    </span>
                  )}
                  <span className="shrink-0 text-muted-foreground">
                    Expires{` `}
                    {new Date(invite.expiresAt).toLocaleDateString()}
                  </span>
                </div>
                <Button
                  variant="glass"
                  size="icon-sm"
                  onClick={() => handleRevoke(invite.id)}
                  aria-label={`Revoke invite ${invite.id}`}
                >
                  <Trash2 />
                </Button>
              </GlassRow>
            ))}
          </div>
        )}
      </div>

      <UpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        title="Out of seats"
        description="Everyone on your plan's seats is already in this team. Add seats to invite more teammates."
        teamProductId={productIds.team}
        teamYearlyProductId={productIds.teamYearly}
        teamId={teamId}
      />
    </>
  )
}

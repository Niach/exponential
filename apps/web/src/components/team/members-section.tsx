import { useEffect, useMemo, useState } from "react"
import {
  Crown,
  LoaderCircle,
  Ellipsis,
  ShieldCheck,
  Trash2,
} from "lucide-react"
import {
  conceptIcon,
  Pill,
  Button,
  GlassRow,
  GlassSectionHeader,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Separator,
  UserAvatar,
} from "@exp/ui"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { toast } from "sonner"
import type { User, TeamMember } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"
import { useTeamInvites } from "@/hooks/use-team-data"
import { displayUserName } from "@/lib/user-display"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { InviteLinkRow, InviteMemberForm } from "./invite-member-form"
import { PLACEHOLDER_LABELS, placeholderStatuses } from "@/lib/placeholder-status"

// EXP-687: leaving a team is a sign-out, removing someone is a user-minus —
// both red, both the same concepts the natives draw.
const NavSignOutIcon = conceptIcon(`nav-sign-out`)
const UiRemoveMemberIcon = conceptIcon(`ui-remove-member`)
// EXP-774: the invite glyphs are registry concepts so the IDE draws the same.
const UiMailIcon = conceptIcon(`ui-mail`)
const UiLinkIcon = conceptIcon(`ui-link`)


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
  // "Resend invite" for a placeholder member: the shared invite form,
  // prefilled and editable, bound to that member's row.
  const [resendTarget, setResendTarget] = useState<{
    userId: string
    name: string
    email: string
  } | null>(null)
  const invites = useTeamInvites(teamId)
  const placeholders = useMemo(() => placeholderStatuses(invites), [invites])

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
            const placeholder = placeholders.get(member.userId)
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
                  <UserAvatar
                    size={32}
                    className="shrink-0"
                    user={{
                      id: member.userId,
                      name: displayName,
                      image: user?.image,
                    }}
                  />
                  <div className="min-w-0">
                    <div className="flex min-w-0 items-center gap-3">
                      <span className="truncate text-sm font-medium">
                        {displayName}
                        {isSelf && (
                          <span className="text-muted-foreground"> (you)</span>
                        )}
                      </span>
                      <Pill leading={roleIcon}>{member.role}</Pill>
                      {placeholder && (
                        <Pill
                          leading={<UiMailIcon className="size-3" />}
                          className="text-muted-foreground"
                        >
                          {PLACEHOLDER_LABELS[placeholder]}
                        </Pill>
                      )}
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
                          variant="ghost"
                          size="icon-sm"
                          aria-label={`Member actions for ${displayName}`}
                        >
                          <Ellipsis />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {canManageMembers && !isSelf && (
                          <>
                            {placeholder && (
                              <DropdownMenuItem
                                onClick={() =>
                                  setResendTarget({
                                    userId: member.userId,
                                    name: user?.name ?? ``,
                                    email: user?.email ?? ``,
                                  })
                                }
                              >
                                <UiMailIcon className="mr-2 h-4 w-4" />
                                Resend invite
                              </DropdownMenuItem>
                            )}
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
        open={resendTarget !== null}
        onOpenChange={(open) => {
          if (!open) setResendTarget(null)
        }}
      >
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Resend invite</DialogTitle>
            <DialogDescription>
              {resendTarget?.name} is on the team but has not joined yet. Send
              a fresh link — fix the address first if it was wrong.
            </DialogDescription>
          </DialogHeader>
          {resendTarget && teamId && (
            <InviteMemberForm
              teamId={teamId}
              defaultName={resendTarget.name}
              defaultEmail={resendTarget.email}
              placeholderUserId={resendTarget.userId}
              layout="stack"
              autoFocus="email"
              onInvited={(invited) => {
                if (invited.emailDelivered) setResendTarget(null)
              }}
            />
          )}
        </DialogContent>
      </Dialog>

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
  const [generating, setGenerating] = useState(false)
  const [inviteUrl, setInviteUrl] = useState<string | null>(null)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    team: string | null
    teamYearly: string | null
  }>({ team: null, teamYearly: null })
  const now = Date.now()
  // Pending = unaccepted AND unexpired (an expired placeholder invite is a
  // marker on the member row above, not a pending link).
  const invites = useTeamInvites(teamId).filter(
    (invite) => !invite.acceptedAt && new Date(invite.expiresAt).getTime() > now
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

  const handleRevoke = async (id: string) => {
    await trpc.teamInvites.revoke.mutate({ id })
  }

  return (
    <div className="space-y-4">
      <div>
        <div className="text-sm font-medium">Invite members</div>
        <div className="text-xs text-muted-foreground">
          Send an invite by email — they join the team right away and can be
          assigned work before they sign in — or generate a link to share
          yourself
        </div>
      </div>

      <InviteMemberForm teamId={teamId} />

      {inviteUrl && <InviteLinkRow url={inviteUrl} />}

      <Button
        variant="outline"
        onClick={handleGenerate}
        disabled={generating}
      >
        {generating && <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />}
        <UiLinkIcon className="mr-2 h-4 w-4" />
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
                <Pill leading={<UiMailIcon className="size-3" />}>
                  {invite.role}
                </Pill>
                {/* EXP-698: a link invite carries no address, and an empty
                    slot collapsed the row so the chips of a mixed list never
                    lined up. It says what it is instead. */}
                {invite.email ? (
                  <span className="min-w-0 truncate font-medium">
                    {invite.email}
                  </span>
                ) : (
                  <span className="min-w-0 truncate text-muted-foreground">
                    Link invite
                  </span>
                )}
                <span className="shrink-0 text-muted-foreground">
                  Expires{` `}
                  {new Date(invite.expiresAt).toLocaleDateString()}
                </span>
              </div>
              <Button
                variant="ghost"
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

      <UpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        title="Out of seats"
        description="Everyone on your plan's seats is already in this team. Add seats to invite more teammates."
        teamProductId={productIds.team}
        teamYearlyProductId={productIds.teamYearly}
        teamId={teamId}
      />
    </div>
  )
}

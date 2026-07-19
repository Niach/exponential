import { useMemo } from "react"
import { useSession } from "@/hooks/use-session"
import { isAdminUser } from "@/lib/auth/app-user"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useBillingPlan, type BillingPlan } from "@/hooks/use-billing"
import type { PlanTier } from "@/lib/billing"
import type { Issue, Team } from "@/db/schema"

export interface TeamPermissions {
  isAuthed: boolean
  isMember: boolean
  isAdmin: boolean
  isModerator: boolean
  isOwner: boolean
  canCreate: boolean
  canMutateIssue: (issue: Pick<Issue, `creatorId`>) => boolean
  // Capability contract mirrored by the native clients. Server mapping:
  //   canManageTeam = owner        (teams.update/delete)
  //   canDeleteBoard   = owner        (boards.delete; call sites also
  //                                      require !board.isProtected)
  //   canManageMembers   = owner||admin (assertCanManageMembers)
  //   canManageRepos     = owner||admin (assertCanManageRepos)
  //   canManageWidgets   = owner        (widgets.create/update/delete/list)
  // `admin` is the global/instance admin (session), not a team role —
  // team roles are only owner/member.
  canManageTeam: boolean
  canDeleteBoard: boolean
  canManageMembers: boolean
  canManageRepos: boolean
  canManageWidgets: boolean
  plan: PlanTier | null
  billingPlan: BillingPlan | null
  canAddMoreMembers: boolean
  canAddMoreBoards: boolean
  canAddMoreStorage: boolean
}

export function useTeamPermissions(
  team: Team | null | undefined
): TeamPermissions {
  const { data: session } = useSession()
  const { members } = useTeamUsers(team?.id)
  const billingPlan = useBillingPlan(team?.id)

  const currentUserId = session?.user?.id
  const isAuthed = Boolean(currentUserId)
  const isAdmin = Boolean(currentUserId) && isAdminUser(session?.user)

  return useMemo(() => {
    const currentMember = currentUserId
      ? members.find((m) => m.userId === currentUserId)
      : undefined
    const isMember = Boolean(currentMember)
    // v7: membership is the only capability gate (every membership is an
    // explicit invite — the public-team self-join and its moderation
    // clamp are gone). Mirrors the server's resolveTeamAccess /
    // assertIssueAccess rules exactly: non-members (including instance
    // admins) get no mutation affordances.
    const isModerator = isMember
    const canCreate = isMember
    const canMutateIssue = (_issue: Pick<Issue, `creatorId`>) => isMember

    const isOwner = currentMember?.role === `owner`
    const canManageTeam = isOwner
    const canDeleteBoard = isOwner
    const canManageMembers = isOwner || isAdmin
    const canManageRepos = isOwner || isAdmin
    const canManageWidgets = isOwner

    // Seats replaced the old member cap (per-seat model, §3.2); a non-agent
    // member can be added while usage is below the purchased seat count.
    const canAddMoreMembers = billingPlan
      ? billingPlan.usage.members < billingPlan.limits.seats
      : true
    // Boards are unlimited on every tier now — no cap to hit.
    const canAddMoreBoards = true
    const canAddMoreStorage = billingPlan
      ? billingPlan.limits.storageMb === Infinity ||
        billingPlan.usage.storageMb < billingPlan.limits.storageMb
      : true
    return {
      isAuthed,
      isMember,
      isAdmin,
      isModerator,
      isOwner,
      canCreate,
      canMutateIssue,
      canManageTeam,
      canDeleteBoard,
      canManageMembers,
      canManageRepos,
      canManageWidgets,
      plan: billingPlan?.plan ?? null,
      billingPlan,
      canAddMoreMembers,
      canAddMoreBoards,
      canAddMoreStorage,
    }
  }, [currentUserId, isAdmin, isAuthed, members, billingPlan])
}

import { useMemo } from "react"
import { useSession } from "@/hooks/use-session"
import { isAdminUser } from "@/lib/auth/app-user"
import { useWorkspaceUsers } from "@/hooks/use-workspace-data"
import { useBillingPlan, type BillingPlan } from "@/hooks/use-billing"
import type { PlanTier } from "@/lib/billing"
import type { Issue, Workspace } from "@/db/schema"

export interface WorkspacePermissions {
  isAuthed: boolean
  isMember: boolean
  isAdmin: boolean
  isModerator: boolean
  canCreate: boolean
  canMutateIssue: (issue: Pick<Issue, `creatorId`>) => boolean
  plan: PlanTier | null
  billingPlan: BillingPlan | null
  canAddMoreMembers: boolean
  canAddMoreProjects: boolean
  canAddMoreStorage: boolean
}

export function useWorkspacePermissions(
  workspace: Workspace | null | undefined
): WorkspacePermissions {
  const { data: session } = useSession()
  const { members } = useWorkspaceUsers(workspace?.id)
  const billingPlan = useBillingPlan(workspace?.id)

  const currentUserId = session?.user?.id
  const isAuthed = Boolean(currentUserId)
  const isAdmin = Boolean(currentUserId) && isAdminUser(session?.user)

  return useMemo(() => {
    const currentMember = currentUserId
      ? members.find((m) => m.userId === currentUserId)
      : undefined
    const isMember = Boolean(currentMember)
    // Public-workspace membership is an open self-service join, so a plain
    // member there is a participant, not a moderator — mirrors the server's
    // isWorkspaceModerator / assertIssueAccess rules.
    const isPrivilegedMember = Boolean(
      currentMember &&
        (!workspace?.isPublic || currentMember.role === `owner`)
    )
    const isModerator = isPrivilegedMember || isAdmin
    const canCreate = isAuthed
      ? isMember ||
        Boolean(workspace?.isPublic && workspace?.publicWritePolicy === `everyone`)
      : false
    const canMutateIssue = (issue: Pick<Issue, `creatorId`>) => {
      if (!isAuthed) return false
      if (isPrivilegedMember) return true
      if (workspace?.isPublic && issue.creatorId === currentUserId) return true
      if (isAdmin) return true
      return false
    }

    // Seats replaced the old member cap (per-seat model, §3.2); a non-agent
    // member can be added while usage is below the purchased seat count.
    const canAddMoreMembers = billingPlan
      ? billingPlan.usage.members < billingPlan.limits.seats
      : true
    // Projects are unlimited on every tier now — no cap to hit.
    const canAddMoreProjects = true
    const canAddMoreStorage = billingPlan
      ? billingPlan.limits.storageMb === Infinity ||
        billingPlan.usage.storageMb < billingPlan.limits.storageMb
      : true
    return {
      isAuthed,
      isMember,
      isAdmin,
      isModerator,
      canCreate,
      canMutateIssue,
      plan: billingPlan?.plan ?? null,
      billingPlan,
      canAddMoreMembers,
      canAddMoreProjects,
      canAddMoreStorage,
    }
  }, [
    currentUserId,
    isAdmin,
    isAuthed,
    members,
    workspace?.isPublic,
    workspace?.publicWritePolicy,
    billingPlan,
  ])
}

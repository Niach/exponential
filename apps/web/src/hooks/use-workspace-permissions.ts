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
  canUsePushNotifications: boolean
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
    const isMember = Boolean(
      currentUserId && members.some((m) => m.userId === currentUserId)
    )
    const isModerator = isMember || isAdmin
    const canCreate = isAuthed
      ? isMember ||
        Boolean(workspace?.isPublic && workspace?.publicWritePolicy === `everyone`)
      : false
    const canMutateIssue = (issue: Pick<Issue, `creatorId`>) => {
      if (!isAuthed) return false
      if (isMember) return true
      if (workspace?.isPublic && issue.creatorId === currentUserId) return true
      if (isAdmin) return true
      return false
    }

    const canAddMoreMembers = billingPlan
      ? billingPlan.usage.members < billingPlan.limits.members
      : true
    const canAddMoreProjects = billingPlan
      ? billingPlan.usage.projects < billingPlan.limits.projects
      : true
    const canAddMoreStorage = billingPlan
      ? billingPlan.limits.storageMb === Infinity ||
        billingPlan.usage.storageMb < billingPlan.limits.storageMb
      : true
    const canUsePushNotifications = billingPlan
      ? billingPlan.limits.push
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
      canUsePushNotifications,
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

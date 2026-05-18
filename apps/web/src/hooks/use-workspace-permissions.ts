import { useMemo } from "react"
import { authClient } from "@/lib/auth-client"
import { useWorkspaceUsers } from "@/hooks/use-workspace-data"
import type { Issue, Workspace } from "@/db/schema"

export interface WorkspacePermissions {
  isAuthed: boolean
  isMember: boolean
  isAdmin: boolean
  // Member OR admin. Only moderators can set issue status (other than backlog),
  // priority, assignee, due date, or recurrence in public workspaces.
  isModerator: boolean
  canCreate: boolean
  canMutateIssue: (issue: Pick<Issue, `creatorId`>) => boolean
}

export function useWorkspacePermissions(
  workspace: Workspace | null | undefined
): WorkspacePermissions {
  const { data: session } = authClient.useSession()
  const { members } = useWorkspaceUsers(workspace?.id)

  const currentUserId = session?.user?.id
  const isAuthed = Boolean(currentUserId)
  const isAdmin = Boolean(
    currentUserId && (session?.user as { isAdmin?: boolean })?.isAdmin
  )

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
    return {
      isAuthed,
      isMember,
      isAdmin,
      isModerator,
      canCreate,
      canMutateIssue,
    }
  }, [
    currentUserId,
    isAdmin,
    isAuthed,
    members,
    workspace?.isPublic,
    workspace?.publicWritePolicy,
  ])
}

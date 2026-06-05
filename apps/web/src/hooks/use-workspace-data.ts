import { useMemo } from "react"
import { eq } from "@tanstack/react-db"
import { useLiveQuery } from "@tanstack/react-db"
import {
  projectCollection,
  userCollection,
  workspaceInviteCollection,
  workspaceMemberCollection,
  workspaceCollection,
} from "@/lib/collections"
import type {
  Project,
  User,
  Workspace,
  WorkspaceInvite,
  WorkspaceMember,
} from "@/db/schema"

function isDefined<TValue>(value: TValue | undefined): value is TValue {
  return value !== undefined
}

export function useWorkspaceBySlug(workspaceSlug: string) {
  const { data } = useLiveQuery(
    (query) =>
      query
        .from({ workspaces: workspaceCollection })
        .where(({ workspaces }) => eq(workspaces.slug, workspaceSlug)),
    [workspaceSlug]
  )

  return (data?.[0] ?? null) as Workspace | null
}

// A workspace is "solo" when it is non-public and has at most one human member.
// Agents (role 'agent') are excluded so registering a coding agent — the hero
// flow — never reveals the workspace concept. Defaults to `true` while data
// loads to avoid a flash of workspace chrome in the common solo case.
export function useIsSolo(workspaceId?: string): boolean {
  const { data: members } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ members: workspaceMemberCollection })
            .where(({ members }) => eq(members.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
  )

  const { data: workspaces } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ workspaces: workspaceCollection })
            .where(({ workspaces }) => eq(workspaces.id, workspaceId))
        : undefined,
    [workspaceId]
  )

  return useMemo(() => {
    if (!members || !workspaces) return true
    if (workspaces[0]?.isPublic) return false
    const humanMembers = members.filter(
      (member) => member.role !== `agent`
    ).length
    return humanMembers <= 1
  }, [members, workspaces])
}

// Count of non-public workspaces the user OWNS (role 'owner'). Drives the
// per-plan workspace cap UI and the "reveal switcher when you have 2+" rule.
// The public feedback workspace is excluded (it's shared infra, not owned).
export function useOwnedWorkspaceCount(userId?: string): number {
  const { data: allWorkspaces } = useLiveQuery((query) =>
    query.from({ workspaces: workspaceCollection })
  )

  const { data: memberships } = useLiveQuery(
    (query) =>
      userId
        ? query
            .from({ members: workspaceMemberCollection })
            .where(({ members }) => eq(members.userId, userId))
        : undefined,
    [userId]
  )

  return useMemo(() => {
    if (!memberships || !allWorkspaces) return 0
    const publicIds = new Set(
      allWorkspaces.filter((workspace) => workspace.isPublic).map((w) => w.id)
    )
    return memberships.filter(
      (member) => member.role === `owner` && !publicIds.has(member.workspaceId)
    ).length
  }, [allWorkspaces, memberships])
}

// Whether to show workspace-level chrome (the switcher, "New workspace", the
// workspace name). Revealed when the current workspace is no longer solo, OR
// the user belongs to more than one non-public workspace (they clearly already
// reason about multiple workspaces). Biased to hidden while data loads.
export function useShowWorkspaceChrome(
  workspaceId?: string,
  userId?: string
): boolean {
  const isSolo = useIsSolo(workspaceId)
  const { myWorkspaces } = useWorkspaceMemberships(userId)
  const nonPublicCount = myWorkspaces.filter(
    (workspace) => !workspace.isPublic
  ).length
  return !isSolo || nonPublicCount > 1
}

export function useWorkspaceProjects(workspaceId?: string) {
  const { data } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ projects: projectCollection })
            .where(({ projects }) => eq(projects.workspaceId, workspaceId))
            .orderBy(({ projects }) => projects.sortOrder)
        : undefined,
    [workspaceId]
  )

  return (data ?? []) as Project[]
}

export function useWorkspaceMemberships(userId?: string) {
  const { data: allWorkspaces } = useLiveQuery((query) =>
    query.from({ workspaces: workspaceCollection })
  )

  const { data: memberships } = useLiveQuery(
    (query) =>
      userId
        ? query
            .from({ members: workspaceMemberCollection })
            .where(({ members }) => eq(members.userId, userId))
        : undefined,
    [userId]
  )

  const myWorkspaces = useMemo(() => {
    if (!memberships || !allWorkspaces) {
      return []
    }

    const explicit = memberships
      .map((membership) =>
        allWorkspaces.find(
          (workspace) => workspace.id === membership.workspaceId
        )
      )
      .filter(isDefined)
    // The public workspace is visible to all authed users without a
    // membership row. Append it once so it shows up in the switcher.
    const publicWorkspace = allWorkspaces.find((w) => w.isPublic)
    if (publicWorkspace && !explicit.some((w) => w.id === publicWorkspace.id)) {
      explicit.push(publicWorkspace)
    }
    return explicit
  }, [allWorkspaces, memberships])

  return {
    memberships: (memberships ?? []) as WorkspaceMember[],
    myWorkspaces,
  }
}

export function useWorkspaceUsers(workspaceId?: string) {
  const { data: members } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ members: workspaceMemberCollection })
            .where(({ members }) => eq(members.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
  )

  const { data: allUsers } = useLiveQuery((query) =>
    query.from({ users: userCollection })
  )

  const users = useMemo(() => {
    if (!members || !allUsers) {
      return []
    }

    const userIds = new Set(members.map((member) => member.userId))
    return allUsers.filter((user) => userIds.has(user.id))
  }, [allUsers, members])

  const userMap = useMemo(
    () => new Map(users.map((user) => [user.id, user])),
    [users]
  )

  return {
    members: (members ?? []) as WorkspaceMember[],
    userMap: userMap as Map<string, User>,
    users: users as User[],
  }
}

export function useWorkspaceInvites(workspaceId?: string) {
  const { data } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ invites: workspaceInviteCollection })
            .where(({ invites }) => eq(invites.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
  )

  return (data ?? []) as WorkspaceInvite[]
}

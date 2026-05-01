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
  const { data } = useLiveQuery((query) =>
    query
      .from({ workspaces: workspaceCollection })
      .where(({ workspaces }) => eq(workspaces.slug, workspaceSlug))
  )

  return (data?.[0] ?? null) as Workspace | null
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

    return memberships
      .map((membership) =>
        allWorkspaces.find(
          (workspace) => workspace.id === membership.workspaceId
        )
      )
      .filter(isDefined)
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

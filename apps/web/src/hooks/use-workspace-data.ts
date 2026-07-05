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
// Bot users (users.isAgent — the widget helpdesk bot) are excluded from the
// count so a widget config on a private workspace never makes it look shared.
// Defaults to `true` while data loads to avoid a flash of workspace chrome in
// the common solo case.
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

  const { data: allUsers } = useLiveQuery((query) =>
    query.from({ users: userCollection })
  )

  return useMemo(() => {
    if (!members || !workspaces) return true
    if (workspaces[0]?.isPublic) return false
    const botUserIds = new Set(
      (allUsers ?? []).filter((user) => user.isAgent).map((user) => user.id)
    )
    const humanMembers = members.filter(
      (member) => !botUserIds.has(member.userId)
    ).length
    return humanMembers <= 1
  }, [members, workspaces, allUsers])
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
// the user has explicit memberships in more than one workspace (they clearly
// already reason about multiple workspaces). The public workspace counts only
// for users with a membership row in it (e.g. its owner) — for everyone else
// it is just implicitly visible. Biased to hidden while data loads.
export function useShowWorkspaceChrome(
  workspaceId?: string,
  userId?: string
): boolean {
  const isSolo = useIsSolo(workspaceId)
  const { memberships, myWorkspaces } = useWorkspaceMemberships(userId)
  const membershipIds = new Set(
    memberships.map((member) => member.workspaceId)
  )
  const explicitCount = myWorkspaces.filter(
    (workspace) => !workspace.isPublic || membershipIds.has(workspace.id)
  ).length
  return !isSolo || explicitCount > 1
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

// Human workspace users, keyed for pickers + display. Bot users
// (users.isAgent — the widget helpdesk bot, only ever an issue *creator*,
// never an assignee/comment author/rendered name) are excluded at the source
// so every consumer (assignee pickers, row-menu, mentions, member lists) hides
// them at once. Pass `includeAgents` for the rare consumer that needs the bot
// rows (none today).
export function useWorkspaceUsers(
  workspaceId?: string,
  includeAgents = false
) {
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
    return allUsers.filter(
      (user) =>
        userIds.has(user.id) && (includeAgents || !user.isAgent)
    )
  }, [allUsers, members, includeAgents])

  const userMap = useMemo(
    () => new Map(users.map((user) => [user.id, user])),
    [users]
  )

  // Drop the bot's member row too so the members list + count match the human
  // user set (no phantom "2 members" on a fresh solo workspace). A member
  // whose user hasn't synced yet is kept (can't be proven a bot). Skipped
  // when includeAgents.
  const filteredMembers = useMemo(() => {
    const rows = (members ?? []) as WorkspaceMember[]
    if (includeAgents || !allUsers) return rows
    const agentUserIds = new Set(
      allUsers.filter((user) => user.isAgent).map((user) => user.id)
    )
    return rows.filter((member) => !agentUserIds.has(member.userId))
  }, [members, allUsers, includeAgents])

  return {
    members: filteredMembers,
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

import { useMemo } from "react"
import { eq } from "@tanstack/react-db"
import { useLiveQuery } from "@tanstack/react-db"
import {
  boardCollection,
  userCollection,
  teamInviteCollection,
  teamMemberCollection,
  teamCollection,
} from "@/lib/collections"
import type {
  Board,
  User,
  Team,
  TeamInvite,
  TeamMember,
} from "@/db/schema"

function isDefined<TValue>(value: TValue | undefined): value is TValue {
  return value !== undefined
}

export function useTeamBySlug(teamSlug: string) {
  const { data } = useLiveQuery(
    (query) =>
      query
        .from({ teams: teamCollection })
        .where(({ teams }) => eq(teams.slug, teamSlug)),
    [teamSlug]
  )

  return (data?.[0] ?? null) as Team | null
}

// A team is "solo" when it has at most one member. Defaults to `true` while
// data loads to avoid a flash of team chrome in the common solo case.
export function useIsSolo(teamId?: string): boolean {
  const { data: members } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ members: teamMemberCollection })
            .where(({ members }) => eq(members.teamId, teamId))
        : undefined,
    [teamId]
  )

  return useMemo(() => {
    if (!members) return true
    return members.length <= 1
  }, [members])
}

// Count of teams the user OWNS (role 'owner'). Drives the per-plan
// team cap UI and the "reveal switcher when you have 2+" rule. (v7:
// teams are always private; only members sync team rows at all.)
export function useOwnedTeamCount(userId?: string): number {
  const { data: memberships } = useLiveQuery(
    (query) =>
      userId
        ? query
            .from({ members: teamMemberCollection })
            .where(({ members }) => eq(members.userId, userId))
        : undefined,
    [userId]
  )

  return useMemo(() => {
    if (!memberships) return 0
    return memberships.filter((member) => member.role === `owner`).length
  }, [memberships])
}

// Whether to show team-level chrome (the switcher, "New team", the
// team name). Revealed when the current team is no longer solo, OR
// the user has memberships in more than one team (they clearly already
// reason about multiple teams). Biased to hidden while data loads.
export function useShowTeamChrome(
  teamId?: string,
  userId?: string
): boolean {
  const isSolo = useIsSolo(teamId)
  const { myTeams } = useTeamMemberships(userId)
  return !isSolo || myTeams.length > 1
}

export function useTeamBoards(teamId?: string) {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.teamId, teamId))
            .orderBy(({ boards }) => boards.sortOrder)
        : undefined,
    [teamId]
  )

  return (data ?? []) as Board[]
}

export function useTeamMemberships(userId?: string) {
  const { data: allTeams } = useLiveQuery((query) =>
    query.from({ teams: teamCollection })
  )

  const { data: memberships } = useLiveQuery(
    (query) =>
      userId
        ? query
            .from({ members: teamMemberCollection })
            .where(({ members }) => eq(members.userId, userId))
        : undefined,
    [userId]
  )

  const myTeams = useMemo(() => {
    if (!memberships || !allTeams) {
      return []
    }

    return memberships
      .map((membership) =>
        allTeams.find(
          (team) => team.id === membership.teamId
        )
      )
      .filter(isDefined)
  }, [allTeams, memberships])

  return {
    memberships: (memberships ?? []) as TeamMember[],
    myTeams,
  }
}

// Team users, keyed for pickers + display (assignee pickers, row-menu,
// mentions, member lists).
export function useTeamUsers(teamId?: string) {
  const { data: members } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ members: teamMemberCollection })
            .where(({ members }) => eq(members.teamId, teamId))
        : undefined,
    [teamId]
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
    members: (members ?? []) as TeamMember[],
    userMap: userMap as Map<string, User>,
    users: users as User[],
  }
}

export function useTeamInvites(teamId?: string) {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ invites: teamInviteCollection })
            .where(({ invites }) => eq(invites.teamId, teamId))
        : undefined,
    [teamId]
  )

  return (data ?? []) as TeamInvite[]
}

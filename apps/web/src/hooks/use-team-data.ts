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

// Boards plus the live query's readiness. A DISABLED live query reports
// `isReady: true` (it never ran), so the readiness signal carries its own
// enabling condition (REV2-59) — false until the team is resolved AND the
// boards snapshot landed, letting callers tell "no boards" from "still
// syncing".
export function useTeamBoardsWithReady(teamId?: string) {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.teamId, teamId))
            .orderBy(({ boards }) => boards.sortOrder)
        : undefined,
    [teamId]
  )

  return {
    boards: (data ?? []) as Board[],
    boardsReady: Boolean(teamId) && isReady,
  }
}

export function useTeamBoards(teamId?: string) {
  return useTeamBoardsWithReady(teamId).boards
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

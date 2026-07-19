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

// A team is "solo" when it has at most one human member. Bot users
// (users.isAgent — the widget helpdesk bot) are excluded from the count so a
// widget config on a private team never makes it look shared. Defaults to
// `true` while data loads to avoid a flash of team chrome in the common
// solo case.
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

  const { data: allUsers } = useLiveQuery((query) =>
    query.from({ users: userCollection })
  )

  return useMemo(() => {
    if (!members) return true
    const botUserIds = new Set(
      (allUsers ?? []).filter((user) => user.isAgent).map((user) => user.id)
    )
    const humanMembers = members.filter(
      (member) => !botUserIds.has(member.userId)
    ).length
    return humanMembers <= 1
  }, [members, allUsers])
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

// Human team users, keyed for pickers + display. Bot users
// (users.isAgent — the widget helpdesk bot, only ever an issue *creator*,
// never an assignee/comment author/rendered name) are excluded at the source
// so every consumer (assignee pickers, row-menu, mentions, member lists) hides
// them at once. Pass `includeAgents` for the rare consumer that needs the bot
// rows (none today).
export function useTeamUsers(
  teamId?: string,
  includeAgents = false
) {
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
  // user set (no phantom "2 members" on a fresh solo team). A member
  // whose user hasn't synced yet is kept (can't be proven a bot). Skipped
  // when includeAgents.
  const filteredMembers = useMemo(() => {
    const rows = (members ?? []) as TeamMember[]
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

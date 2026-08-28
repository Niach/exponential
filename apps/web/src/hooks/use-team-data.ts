import { useMemo } from "react"
import { eq } from "@tanstack/react-db"
import { useLiveQuery } from "@tanstack/react-db"
import {
  boardCollection,
  labelCollection,
  userCollection,
  teamInviteCollection,
  teamMemberCollection,
  teamCollection,
} from "@/lib/collections"
import type {
  Board,
  Label,
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

/**
 * EXP-525 — the CANONICAL board order, byte-mirrored by the desktop IDE's
 * `crates/sync/src/collections.rs`:
 *
 *   1. `sortOrder` ascending, a missing one sorting LAST (null → +∞);
 *   2. `createdAt` ascending, a missing one sorting FIRST (Rust `None < Some`);
 *   3. `id` ascending as the final tiebreak.
 *
 * Written out rather than expressed through `orderBy` on purpose: TanStack DB
 * compares strings with `localeCompare` by default, which is not the byte-wise
 * order Rust's `String: Ord` gives, so the id tiebreak could disagree between
 * the two clients. Comparing with `<`/`>` keeps them identical.
 *
 * The tiebreaks are what actually matter in practice: `sort_order` is
 * `NOT NULL DEFAULT 0`, so every board of an untouched team ties on it and the
 * sidebar order was whatever the collection happened to hand back.
 */
export function compareBoards(left: Board, right: Board): number {
  const leftOrder = left.sortOrder ?? Number.POSITIVE_INFINITY
  const rightOrder = right.sortOrder ?? Number.POSITIVE_INFINITY
  if (leftOrder !== rightOrder) return leftOrder - rightOrder

  // `null` and an unparseable value both mean "no created_at" — sorted first,
  // never folded in with epoch-0 rows.
  const leftCreated = createdAtMs(left.createdAt)
  const rightCreated = createdAtMs(right.createdAt)
  if (leftCreated !== rightCreated) {
    if (leftCreated === null) return -1
    if (rightCreated === null) return 1
    return leftCreated - rightCreated
  }

  return left.id < right.id ? -1 : left.id > right.id ? 1 : 0
}

function createdAtMs(value: Date | string | null | undefined): number | null {
  if (value == null) return null
  const ms = value instanceof Date ? value.getTime() : Date.parse(value)
  return Number.isNaN(ms) ? null : ms
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
        : undefined,
    [teamId]
  )

  const boards = useMemo(
    () => [...((data ?? []) as Board[])].sort(compareBoards),
    [data]
  )

  return {
    boards,
    boardsReady: Boolean(teamId) && isReady,
  }
}

export function useTeamBoards(teamId?: string) {
  return useTeamBoardsWithReady(teamId).boards
}

// The team's labels, for pickers and the filter popover. Cheap: a client-side
// filter over the already-synced labels collection, so a second caller costs
// nothing beyond the live query itself.
export function useTeamLabels(teamId?: string) {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.teamId, teamId))
        : undefined,
    [teamId]
  )

  return (data ?? []) as Label[]
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
            // Join order, then id (EXP-668). Without an ORDER BY at all this
            // list came back in whatever order Electric happened to hold the
            // rows in, so the team roster reshuffled itself between syncs —
            // the members settings screenshot was observed listing the same
            // four people in exactly reversed order on consecutive runs.
            .orderBy(({ members }) => members.createdAt)
            .orderBy(({ members }) => members.id)
        : undefined,
    [teamId]
  )

  const { data: allUsers } = useLiveQuery((query) =>
    // Name, then id (EXP-668) — `users` feeds the assignee and mention
    // pickers, which have no order of their own to fall back on.
    query
      .from({ users: userCollection })
      .orderBy(({ users }) => users.name)
      .orderBy(({ users }) => users.id)
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
            // Newest invite first, then id (EXP-668) — unordered, the pending
            // list reshuffled on every sync exactly like the roster did.
            .orderBy(({ invites }) => invites.createdAt, `desc`)
            .orderBy(({ invites }) => invites.id)
        : undefined,
    [teamId]
  )

  return (data ?? []) as TeamInvite[]
}

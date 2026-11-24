import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Team } from "@/db/schema"
import { sessionCollection, teamCollection } from "@/lib/collections"
import { chatTeamIds, sessionOwnDevices } from "@/lib/chat-teams"

// #EXP-2079 (agent-settings.json `assistant.chatTeams`): the teams the lists
// of a chat read. One team for every session / agent of a synced team; the
// turn's teams for the Assistant's conversation (its personal team never
// syncs).

export interface ChatTeams {
  /** The chat teams' ids, in the cross-team order. */
  teamIds: string[]
  /** Their synced rows, same order (a not-synced own team has none). */
  teams: Team[]
  /** The own team is one of the viewer's synced teams (the unchanged,
   *  one-team case). */
  ownSynced: boolean
}

/** The chat teams of a session / agent of `ownTeamId` whose conversation
 *  root's Teams pick is `pickedTeamIds`. */
export function useChatTeams(
  ownTeamId: string | null | undefined,
  pickedTeamIds: readonly string[] | null | undefined
): ChatTeams {
  const { data } = useLiveQuery((query) => query.from({ teams: teamCollection }))
  const pickKey = pickedTeamIds ? pickedTeamIds.join(`,`) : null
  return useMemo(() => {
    const synced = (data ?? []) as Team[]
    const teamIds = chatTeamIds(ownTeamId, pickKey === null ? null : pickKey.split(`,`).filter(Boolean), synced)
    const byId = new Map(synced.map((team) => [team.id, team]))
    return {
      teamIds,
      teams: teamIds.flatMap((id) => (byId.has(id) ? [byId.get(id)!] : [])),
      ownSynced: !!ownTeamId && byId.has(ownTeamId),
    }
  }, [data, ownTeamId, pickKey])
}

type ChatSession = { id: string; rootId?: string | null; teamId: string; scopeTeamIds?: string[] | null }

/** The chat teams of a synced session row: its ROOT's Teams pick decides
 *  (a child reads its conversation's). No row yet (an agent chat before its
 *  conversation) = the chat teams of `ownTeamId` with nothing picked. */
export function useSessionChatTeams(
  session: ChatSession | null | undefined,
  ownTeamId?: string | null,
  /** #EXP-2082: the composer's Teams pick while it is being edited (null =
   *  none: every team), read at once; undefined = the root's stored pick. */
  draftPick?: readonly string[] | null
): ChatTeams {
  const rootId = session ? (session.rootId ?? session.id) : null
  const { data } = useLiveQuery(
    (query) =>
      rootId && rootId !== session?.id
        ? query.from({ s: sessionCollection }).where(({ s }) => eq(s.id, rootId))
        : undefined,
    [rootId, session?.id]
  )
  const root = (data?.[0] as ChatSession | undefined) ?? session ?? null
  return useChatTeams(session?.teamId ?? ownTeamId ?? null, draftPick !== undefined ? draftPick : (root?.scopeTeamIds ?? null))
}

/** `deviceCases`: the device list of `session` is the viewer's OWN machines
 *  (no team rule): their own session in a team they do not sync. */
export function useSessionOwnDevices(
  session: { teamId: string; userId?: string | null } | null | undefined,
  viewerId: string | null | undefined
): boolean {
  const { data } = useLiveQuery((query) => query.from({ teams: teamCollection }))
  const teamId = session?.teamId
  const starter = session?.userId
  return useMemo(
    () => (teamId ? sessionOwnDevices({ teamId, userId: starter }, viewerId, (data ?? []) as { id: string }[]) : false),
    [data, teamId, starter, viewerId]
  )
}

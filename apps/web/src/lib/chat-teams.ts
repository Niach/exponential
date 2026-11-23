// #EXP-2079 (agent-settings.json `assistant.chatTeams`): THE TEAMS A CHAT'S
// LISTS READ. A session in a team the viewer syncs reads that one team; the
// Assistant's conversation sits in its member's personal team, which never
// syncs, so its lists (`#` refs, `@` members, the MCP pick, `/` prompts,
// Link / New issue, links) read the turn's teams: the root's Teams pick ∩
// the synced teams, else every synced team, in the cross-team order.
import { assistantChatTeams, assistantOwnDevices } from "@exp/domain-contract/agent-settings"
import { issueCollection, sessionCollection, teamCollection } from "@/lib/collections"

type TeamLike = { id: string; name: string }

/** The cross-team order: by name (case-insensitive), then id. */
export function syncedTeamOrder(teams: readonly TeamLike[]): string[] {
  return [...teams]
    .sort((a, b) => {
      const left = a.name.toLowerCase()
      const right = b.name.toLowerCase()
      if (left !== right) return left < right ? -1 : 1
      return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
    })
    .map((team) => team.id)
}

/** The chat teams of a session / agent of `ownTeamId` with the Teams pick
 *  `pickedTeamIds`, over the synced `teams`. */
export function chatTeamIds(
  ownTeamId: string | null | undefined,
  pickedTeamIds: readonly string[] | null | undefined,
  teams: readonly TeamLike[] = teamCollection.toArray as TeamLike[]
): string[] {
  return assistantChatTeams({ ownTeamId, syncedTeamIds: syncedTeamOrder(teams), pickedTeamIds })
}

type SessionLike = {
  id: string
  rootId?: string | null
  teamId: string
  scopeTeamIds?: string[] | null
}

/** The conversation root's Teams pick of `session` (its own while a root). */
function rootPick(session: SessionLike): readonly string[] | null {
  const root = session.rootId && session.rootId !== session.id ? sessionCollection.get(session.rootId) : undefined
  return (root as SessionLike | undefined)?.scopeTeamIds ?? session.scopeTeamIds ?? null
}

/** The chat teams of a synced session row (store read, no hook). `picked`
 *  overrides the stored pick (a send that carries a new one). */
export function sessionChatTeamIds(sessionId: string | null | undefined, picked?: readonly string[] | null): string[] {
  const session = sessionId ? (sessionCollection.get(sessionId) as SessionLike | undefined) : undefined
  if (!session) return chatTeamIds(null, picked ?? null)
  return chatTeamIds(session.teamId, picked !== undefined ? picked : rootPick(session))
}

/** The slug a team-less target of the chat opens through: the first chat
 *  team's (`chatTeams` (7)); null while no team is synced. */
export function chatTeamSlug(sessionId: string | null | undefined): string | null {
  for (const id of sessionChatTeamIds(sessionId)) {
    const slug = (teamCollection.get(id) as { slug?: string } | undefined)?.slug
    if (slug) return slug
  }
  return null
}

/** `#IDENT` → issue ids over `teamIds`: an identifier found in several teams
 *  is the first team's in order (`chatTeams` (1)). */
export function issueIdsInTeams(
  teamIds: readonly string[],
  identifiers: readonly string[],
  issues: Iterable<{ id: string; teamId: string; identifier: string }> = issueCollection.values() as Iterable<{
    id: string
    teamId: string
    identifier: string
  }>
): string[] {
  const rank = new Map(teamIds.map((id, index) => [id, index]))
  const found = new Map<string, { id: string; rank: number }>()
  const wanted = new Set(identifiers.map((identifier) => identifier.replace(/^#/, ``).toUpperCase()))
  for (const row of issues) {
    const at = rank.get(row.teamId)
    const key = row.identifier.toUpperCase()
    if (at === undefined || !wanted.has(key)) continue
    const held = found.get(key)
    if (!held || at < held.rank) found.set(key, { id: row.id, rank: at })
  }
  return [...wanted].flatMap((key) => (found.has(key) ? [found.get(key)!.id] : []))
}

/** `deviceCases`: the device list of a session in a team the viewer does
 *  not sync that they started = their own machines, no team rule. */
export function sessionOwnDevices(
  session: { teamId: string; userId?: string | null },
  viewerId: string | null | undefined,
  teams: readonly { id: string }[] = teamCollection.toArray as { id: string }[]
): boolean {
  return assistantOwnDevices({
    sessionTeamSynced: teams.some((team) => team.id === session.teamId),
    starterIsViewer: !!viewerId && session.userId === viewerId,
  })
}

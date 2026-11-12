import { useCallback, useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { needsYou, type NeedsYouMessageInput, type NeedsYouRow, type NeedsYouSessionInput } from "@exp/domain-contract/needs-you"
import { resultAge } from "@exp/domain-contract/results-tab"
import { offlineSince, stuckRow, type HoldReason, type StuckRow, type TeamMergeMode } from "@exp/domain-contract/merge-train"
import { useLiveQuery } from "@tanstack/react-db"
import { useTeamPullRequestsByRoot } from "@/components/chat/chats-list"
import { deviceCollection, pullRequestCollection, sessionCollection, teamCollection } from "@/lib/collections"
import { resolveSessionDevice } from "@/lib/session-device"
import { useNow } from "@/hooks/use-now"
import { useTeamById } from "@/hooks/use-team-data"
import { useAgents, useDigestMessages, useSessionTitles, useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamAgentRows } from "@/hooks/use-agent-row"
import { sessionTarget } from "@/lib/session-back"
import type { Device } from "@/db/schema"
import { prMergeState, type PullRequestRow, type SessionRow, type TreeMessage } from "@/lib/session-tree"

// #FEED-191 NEEDS YOU (fixtures/needs-you.json): every plan and question
// waiting on the viewer and (#EXP-1699) every PR ready to merge, the contract's
// `needsYou()` over the team's conversation DIGESTS (never a transcript:
// EXP-1380), sessions and pull requests. A session
// is named by THE session title (the chat list's), an agent conversation by its
// agent.
// #EXP-1710 (merge-train.json `stuckRow`): a PR whose auto-merge is stuck
// (held by the limits, or a fix waiting 2 h+ on the opener's offline device)
// is ONE `pr` row titled `Auto-merge stuck on #N`, meta = the reason, with
// its Retry / Merge now (owners) / Close PR actions; it replaces that PR's
// ready-to-merge row and sorts with the PR rows (newest first). An
// `approval`-mode ready row carries its PR: its press is Needs approval (THE
// pill's confirm), never the chat.

/** A Needs you row; a stuck PR's carries its PR and actions, an `approval`
 *  ready PR's its PR to approve. */
export type NeedsYouListRow = NeedsYouRow & {
  stuck?: { pullRequest: PullRequestRow; actions: StuckRow[`actions`] }
  approve?: PullRequestRow
}

const iso = (value: unknown): string => {
  const at = value instanceof Date ? value : new Date(value as string)
  return Number.isNaN(at.getTime()) ? new Date(0).toISOString() : at.toISOString()
}

const isWaitingKind = (card: { kind?: unknown } | null | undefined) =>
  card?.kind === `plan` || card?.kind === `question` || card?.kind === `ask`

export function useNeedsYou(
  teamId: string | null | undefined,
  me: string | null | undefined,
  owner = false
): { rows: NeedsYouListRow[]; isReady: boolean } {
  // #EXP-1714: the team's sessions + the viewer's global Assistant's (its
  // conversation lives in their personal team, never a synced team).
  const teamAgents = useTeamAgentRows(teamId ? [teamId] : [], me)
  const personal = teamAgents.find((agent) => agent.teamId !== teamId)?.teamId ?? null
  const teamIds = useMemo(() => (teamId ? (personal ? [teamId, personal] : [teamId]) : []), [teamId, personal])
  const { rows: sessions, isReady: sessionsReady } = useTeamSessions(teamIds)
  const { agents } = useAgents(teamId ?? undefined, me)
  const { messages, isReady: digestsReady } = useDigestMessages({ teamIds })
  const { all: pullRequests } = useTeamPullRequestsByRoot(teamId ?? undefined)
  const now = useNow(60_000).getTime()
  const { data: deviceRows } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  // #EXP-1710: `auto` (the default) = no ready-to-merge PR waits for a person.
  const mergeMode = (useTeamById(teamId)?.mergeMode ?? `auto`) as TeamMergeMode

  const inputs = useMemo(() => waitingInputs(messages), [messages])

  // THE session title, read only for the sessions holding a card (and roots).
  const byId = useMemo(() => new Map(sessions.map((row) => [row.id, row])), [sessions])
  const titled = useMemo(() => {
    const ids = new Set<string>()
    const add = (id: unknown) => {
      const row = typeof id === `string` ? byId.get(id) : undefined
      if (!row) return
      ids.add(row.id)
      ids.add(row.rootId ?? row.id)
    }
    for (const message of inputs) {
      add(message.sessionId)
      add(message.card?.sessionId)
    }
    for (const pr of pullRequests) add(pr.rootSessionId)
    return [...ids]
      .sort()
      .map((id) => byId.get(id))
      .filter((row): row is SessionRow => row !== undefined)
  }, [inputs, pullRequests, byId])
  const { titles } = useSessionTitles(titled)

  const rows = useMemo(() => {
    const agentNames = new Map(agents.map((agent) => [agent.agentId, agent.name]))
    return needsYouListRows({
      sessions,
      nameOf: (row) => (row.agentId ? agentNames.get(row.agentId) : undefined) ?? titles.get(row.id) ?? row.title ?? ``,
      messages: inputs,
      pullRequests,
      devices: (deviceRows ?? []) as Device[],
      now,
      me: me ?? null,
      owner,
      mergeMode,
    })
  }, [sessions, agents, titles, inputs, pullRequests, now, me, owner, deviceRows, mergeMode])

  return { rows, isReady: sessionsReady && digestsReady }
}

/** The digest messages that can wait on the viewer (plans, questions). */
function waitingInputs(messages: readonly TreeMessage[]): NeedsYouMessageInput[] {
  return messages
    .filter((message) => isWaitingKind(message.card as { kind?: unknown } | null))
    .map((message) => ({
      id: message.id,
      sessionId: message.sessionId,
      authorSessionId: message.authorSessionId ?? null,
      body: message.body ?? ``,
      createdAt: iso(message.createdAt),
      answeredAt: message.answeredAt ? iso(message.answeredAt) : null,
      card: message.card as Record<string, unknown> | null,
    }))
}

/** ONE team's Needs you rows, pure: `needsYou()` + the stuck PRs + the
 *  approval PRs over its sessions, waiting messages and PRs. */
export function needsYouListRows(input: {
  sessions: readonly SessionRow[]
  nameOf: (row: SessionRow) => string
  messages: readonly NeedsYouMessageInput[]
  pullRequests: readonly PullRequestRow[]
  devices: readonly Device[]
  now: number
  me: string | null
  owner: boolean
  mergeMode: TeamMergeMode
}): NeedsYouListRow[] {
  const { sessions, pullRequests, now, me, mergeMode } = input
  const byId = new Map(sessions.map((row) => [row.id, row]))
  const sessionInputs: NeedsYouSessionInput[] = sessions.map((row) => ({
    id: row.id,
    rootId: row.rootId ?? row.id,
    name: input.nameOf(row),
    status: row.status,
    agentId: row.agentId ?? null,
    userId: row.userId ?? null,
    busy: row.busy === true,
  }))
  const base = needsYou({
    now,
    me,
    sessions: sessionInputs,
    messages: [...input.messages],
    pullRequests: pullRequests.map((pr) => ({
      id: pr.id,
      rootSessionId: pr.rootSessionId ?? null,
      number: pr.number,
      title: pr.title ?? ``,
      state: pr.state,
      checksState: (pr.checksState as string | null) ?? null,
      basePullRequestId: pr.basePullRequestId ?? null,
      createdAt: iso(pr.createdAt),
    })),
    mergeMode,
  })
  const folded = withStuckRows(base, {
    pullRequests,
    sessions: byId,
    chats: new Map(sessionInputs.map((row) => [row.id, row.name])),
    devices: input.devices,
    now,
    owner: input.owner,
  })
  return withApprovals(folded, pullRequests, mergeMode)
}

/** #FEED-232 (nav-badges.json): every listed team's Needs you rows' length
 *  (the team picker) and their distinct total (the OS app icon), over the
 *  shapes every member team already syncs. Each team counts like
 *  `useNeedsYou` (its sessions + the viewer's global Assistant's). */
export function useNeedsYouCounts(
  teamIds: readonly string[],
  me: string | null | undefined
): { byTeam: ReadonlyMap<string, number>; total: number } {
  const agentRows = useTeamAgentRows(teamIds, me)
  const personal = agentRows.find((agent) => !teamIds.includes(agent.teamId))?.teamId ?? null
  const scope = useMemo(
    () => (personal ? [...teamIds, personal] : [...teamIds]),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [teamIds.join(`,`), personal]
  )
  const { rows: sessions } = useTeamSessions(scope)
  const { messages } = useDigestMessages({ teamIds: scope })
  const { data: prRows } = useLiveQuery((query) => query.from({ prs: pullRequestCollection }), [])
  const { data: teamRows } = useLiveQuery((query) => query.from({ teams: teamCollection }), [])
  const { data: deviceRows } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const now = useNow(60_000).getTime()
  const inputs = useMemo(() => waitingInputs(messages), [messages])
  return useMemo(() => {
    const byTeam = new Map<string, number>()
    const seen = new Set<string>()
    const prs = (prRows ?? []) as PullRequestRow[]
    const modes = new Map(((teamRows ?? []) as { id: string; mergeMode?: string | null }[]).map((t) => [t.id, t.mergeMode]))
    for (const teamId of teamIds) {
      const own = sessions.filter((row) => row.teamId === teamId || row.teamId === personal)
      const ids = new Set(own.map((row) => row.id))
      const rows = needsYouListRows({
        sessions: own,
        nameOf: () => ``,
        messages: inputs.filter((message) => ids.has(message.sessionId)),
        pullRequests: prs.filter((pr) => pr.teamId === teamId),
        devices: (deviceRows ?? []) as Device[],
        now,
        me: me ?? null,
        owner: false,
        mergeMode: (modes.get(teamId) ?? `auto`) as TeamMergeMode,
      })
      byTeam.set(teamId, rows.length)
      for (const row of rows) seen.add(`${row.kind}:${row.messageId}`)
    }
    return { byTeam, total: seen.size }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessions, inputs, prRows, teamRows, deviceRows, now, me, personal, teamIds.join(`,`)])
}

/** Fold the stuck PRs (merge-train `stuckRow`) into `needsYou()`'s rows: a
 *  stuck PR replaces its ready-to-merge row and sorts with the PR rows
 *  (newest first) under every decision. The opener's device is offline
 *  since its devices row's last heartbeat. */
export function withStuckRows(
  base: readonly NeedsYouRow[],
  input: {
    pullRequests: readonly PullRequestRow[]
    sessions: ReadonlyMap<string, Pick<SessionRow, `deviceId` | `userId`>>
    chats: ReadonlyMap<string, string>
    devices: readonly Device[]
    now: number
    owner: boolean
  }
): NeedsYouListRow[] {
  const { devices, now } = input
  const stuck: NeedsYouListRow[] = []
  for (const pr of input.pullRequests) {
    const rootId = pr.rootSessionId
    if (!rootId || (pr.state !== `open` && pr.state !== `draft`)) continue
    const deviceOfflineSince = prDeviceOfflineSince(pr, rootId, input.sessions, devices, now)
    const view = stuckRow({
      number: pr.number,
      state: prMergeState(pr),
      holdReason: (pr.mergeHold ?? null) as HoldReason | null,
      deviceOfflineSince,
      now: new Date(now).toISOString(),
      owner: input.owner,
    })
    if (!view) continue
    stuck.push({
      messageId: pr.id,
      sessionId: rootId,
      rootId,
      kind: `pr`,
      opens: `results`,
      title: view.title,
      chat: input.chats.get(rootId) ?? ``,
      meta: view.meta,
      age: resultAge(Date.parse(iso(pr.createdAt)), now),
      createdAt: iso(pr.createdAt),
      stuck: { pullRequest: pr, actions: view.actions },
    })
  }
  if (stuck.length === 0) return base as NeedsYouListRow[]
  const ids = new Set(stuck.map((row) => row.messageId))
  const decisions = base.filter((row) => row.kind !== `pr`)
  const prs = [...base.filter((row) => row.kind === `pr` && !ids.has(row.messageId)), ...stuck].sort(
    (a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt)
  )
  return [...decisions, ...prs]
}

/** merge-train `offlineSince` over the synced rows: the opener = the PR's
 *  `sessionId` session, else its root; its device = THE device presence
 *  (`resolveSessionDevice`) and that row's `lastSeenAt`, null when no devices
 *  row is synced (an unknown last-seen = since the PR's `updatedAt`). */
export function prDeviceOfflineSince(
  pr: Pick<PullRequestRow, `sessionId` | `updatedAt`>,
  rootId: string,
  sessions: ReadonlyMap<string, Pick<SessionRow, `deviceId` | `userId`>>,
  devices: readonly Device[],
  now: number
): string | null {
  const opener = (pr.sessionId ? sessions.get(pr.sessionId) : undefined) ?? sessions.get(rootId)
  const deviceId = opener?.deviceId || null
  let device: { online: boolean; lastSeenAt: string | null } | null = null
  if (deviceId) {
    const matches = devices.filter((d) => d.deviceId === deviceId)
    const row = matches.find((d) => d.userId === opener?.userId) ?? matches[0]
    if (row) {
      // A row without a heartbeat is never online.
      const online =
        row.lastSeenAt != null &&
        resolveSessionDevice({ deviceId, userId: opener?.userId ?? `` }, devices, new Date(now)).online === true
      device = { online, lastSeenAt: row.lastSeenAt != null ? iso(row.lastSeenAt) : null }
    }
  }
  return offlineSince({ opener: deviceId !== null, device, updatedAt: iso(pr.updatedAt) })
}

/** #EXP-1710: in `approval` mode a ready PR row (kind `pr`, not stuck) carries
 *  its PR, whose press asks THE pill's Needs approval confirm. */
export function withApprovals(
  rows: readonly NeedsYouListRow[],
  pullRequests: readonly PullRequestRow[],
  mergeMode: TeamMergeMode
): NeedsYouListRow[] {
  if (mergeMode !== `approval`) return rows as NeedsYouListRow[]
  const byId = new Map(pullRequests.map((pr) => [pr.id, pr]))
  return rows.map((row) => {
    const pr = row.kind === `pr` && !row.stuck ? byId.get(row.messageId) : undefined
    return pr ? { ...row, approve: pr } : row
  })
}

/** Where a row opens, pure (fixture `opens`): a plan = plan-page.json's deep
 *  link, which lands on the Plan tab of its chat; a card = its CHAT's
 *  Conversation (a child's card surfaces there too) with `?card=`, which
 *  scrolls to it the way THE waiting chip does; a PR = the Results tab, which
 *  holds the PR block; `chat` = an agent conversation, which has no such tab. */
export function needsYouDestination(
  row: Pick<NeedsYouRow, `messageId` | `sessionId` | `rootId` | `opens`>,
  teamSlug: string,
  lookup: Parameters<typeof sessionTarget>[1],
  me: string | null
): { to: string; params: Record<string, string>; search?: { card: string } | { tab: `results` } } {
  if (row.opens === `plan`) {
    return {
      to: `/t/$teamSlug/sessions/$sessionId/plan/$messageId`,
      params: { teamSlug, sessionId: row.sessionId, messageId: row.messageId },
    }
  }
  const target = sessionTarget(row.rootId, lookup, me)
  const search =
    row.opens === `card`
      ? { search: { card: row.messageId } }
      : row.opens === `results`
        ? { search: { tab: `results` as const } }
        : {}
  if (target.to === `agent`) {
    return { to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId: target.agentId }, ...search }
  }
  if (target.to === `chat`) {
    return { to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug, sessionId: target.chatId }, ...search }
  }
  return { to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug, sessionId: target.sessionId } }
}

export function useOpenNeedsYou(teamSlug: string, me: string | null | undefined): (row: NeedsYouRow) => void {
  const navigate = useNavigate()
  return useCallback(
    (row) =>
      void navigate(
        needsYouDestination(
          row,
          teamSlug,
          (id) => sessionCollection.get(id) as SessionRow | undefined,
          me ?? null
        ) as never
      ),
    [navigate, teamSlug, me]
  )
}

import { useCallback, useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { needsYou, type NeedsYouMessageInput, type NeedsYouRow, type NeedsYouSessionInput } from "@exp/domain-contract/needs-you"
import { resultAge } from "@exp/domain-contract/results-tab"
import { stuckRow, type HoldReason, type StuckRow } from "@exp/domain-contract/merge-train"
import { useLiveQuery } from "@tanstack/react-db"
import { useTeamPullRequestsByRoot } from "@/components/thread/threads-list"
import { deviceCollection, sessionCollection } from "@/lib/collections"
import { resolveSessionDevice } from "@/lib/session-device"
import { useNow } from "@/hooks/use-now"
import { useAgents, useDigestMessages, useSessionTitles, useTeamSessions } from "@/hooks/use-session-tree"
import { sessionTarget } from "@/lib/session-back"
import type { Device } from "@/db/schema"
import { prMergeState, type PullRequestRow, type SessionRow } from "@/lib/session-tree"

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
// ready-to-merge row and sorts with the PR rows (newest first).

/** A Needs you row; a stuck PR's carries its PR and actions. */
export type NeedsYouListRow = NeedsYouRow & {
  stuck?: { pullRequest: PullRequestRow; actions: StuckRow[`actions`] }
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
  const { rows: sessions, isReady: sessionsReady } = useTeamSessions(teamId ?? undefined)
  const { agents } = useAgents(teamId ?? undefined, me)
  const teamIds = useMemo(() => (teamId ? [teamId] : []), [teamId])
  const { messages, isReady: digestsReady } = useDigestMessages({ teamIds })
  const { all: pullRequests } = useTeamPullRequestsByRoot(teamId ?? undefined)
  const now = useNow(60_000).getTime()
  const { data: deviceRows } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])

  const inputs = useMemo<NeedsYouMessageInput[]>(
    () =>
      messages
        .filter((message) => isWaitingKind(message.card as { kind?: unknown } | null))
        .map((message) => ({
          id: message.id,
          sessionId: message.sessionId,
          authorSessionId: message.authorSessionId ?? null,
          body: message.body ?? ``,
          createdAt: iso(message.createdAt),
          answeredAt: message.answeredAt ? iso(message.answeredAt) : null,
          card: message.card as Record<string, unknown> | null,
        })),
    [messages]
  )

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
    const input: NeedsYouSessionInput[] = sessions.map((row) => ({
      id: row.id,
      rootId: row.rootId ?? row.id,
      name: (row.agentId ? agentNames.get(row.agentId) : undefined) ?? titles.get(row.id) ?? row.title ?? ``,
      status: row.status,
      agentId: row.agentId ?? null,
      userId: row.userId ?? null,
      busy: row.busy === true,
    }))
    const base = needsYou({
      now,
      me: me ?? null,
      sessions: input,
      messages: inputs,
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
    })
    return withStuckRows(base, {
      pullRequests,
      sessions: byId,
      chats: new Map(input.map((row) => [row.id, row.name])),
      devices: (deviceRows ?? []) as Device[],
      now,
      owner,
    })
  }, [sessions, agents, titles, inputs, pullRequests, now, me, owner, byId, deviceRows])

  return { rows, isReady: sessionsReady && digestsReady }
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
    const opener = input.sessions.get(pr.sessionId ?? rootId)
    let deviceOfflineSince: string | null = null
    if (opener?.deviceId && opener.userId) {
      const host = { deviceId: opener.deviceId, userId: opener.userId }
      if (resolveSessionDevice(host, devices, new Date(now)).online === false) {
        const row = devices.find((d) => d.deviceId === host.deviceId && d.userId === host.userId)
        deviceOfflineSince = iso((row ?? devices.find((d) => d.deviceId === host.deviceId))?.lastSeenAt)
      }
    }
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

/** Where a row opens, pure (fixture `opens`): a plan = plan-page.json's deep
 *  link, which lands on the Plan tab of its thread; a card = its CHAT's
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
  if (target.to === `thread`) {
    return { to: `/t/$teamSlug/threads/$threadId`, params: { teamSlug, threadId: target.threadId }, ...search }
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

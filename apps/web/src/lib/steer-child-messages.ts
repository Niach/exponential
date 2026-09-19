// EXP-700: parent↔child session messages. When a coding session starts
// another via `exponential_sessions_start`, the child reports back — its
// question (`exponential_sessions_ask_parent`) or its close-out summary —
// as text injected into the parent's live steer channel, exactly the rail a
// human uses to steer a run. The receiving agent must never mistake that for
// its human: every injected message carries a bracketed source prefix, and
// this module is the ONE home of that convention (formatters), the child+
// parent lookup, and the end-of-child notification both end paths call.
//
// It lives outside `lib/trpc/coding-sessions.ts` for the same reason as
// coding-session-end.ts: the MCP tool tests mock this one module.
import { eq, sql } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { codingSessions, issues } from "@/db/schema"
import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import type { Context } from "@/lib/trpc"

export const CHILD_RUN_TAG = `Exponential child run`

/** Statuses a parent can still receive messages in (same pair the agent end
 * path accepts — `in_review` = its PR is open but the run is live). */
export const PARENT_LIVE_STATUSES = [`running`, `in_review`] as const

/** Injected text must land as ONE message: the submit convention is a
 * separate `\r` frame, so a newline inside the text would submit early and
 * fragment a summary into several user messages. */
export function oneLine(text: string): string {
  return text.replace(/\s*[\r\n]+\s*/g, ` `).trim()
}

export interface ChildRunRef {
  id: string
  issueIdentifier: string | null
  actionName: string | null
}

/** `EXP-12 3f2a9c1b`, `Nightly build 3f2a9c1b`, or the bare short id. */
export function childRunLabel(child: ChildRunRef): string {
  const short = child.id.slice(0, 8)
  const subject = child.issueIdentifier ?? child.actionName
  return subject ? `${subject} ${short}` : short
}

export function formatChildFinished(
  child: ChildRunRef,
  summary: string
): string {
  return `[${CHILD_RUN_TAG} ${childRunLabel(child)} finished] ${oneLine(summary)}`
}

export function formatChildEndedSilently(
  child: ChildRunRef,
  endedBy: string
): string {
  return `[${CHILD_RUN_TAG} ${childRunLabel(child)} ended without a report (${endedBy})]`
}

/** Names the child's FULL uuid — that is what the parent passes back to
 * `exponential_sessions_message`. */
export function formatChildQuestion(
  child: ChildRunRef,
  question: string
): string {
  return `[${CHILD_RUN_TAG} ${childRunLabel(child)} asks — reply with exponential_sessions_message sessionId=${child.id}] ${oneLine(question)}`
}

/** EXP-804: the child hit its agent's usage wall. The row stays `running`
 * with a moving `updated_at`, so a parent polling `exponential_sessions_get`
 * sees a perfectly healthy run and waits forever (the 2026-09-09 incident) —
 * the wall has to be PUSHED. FEED-35/37: only a REAL refusal reaches here
 * now (the device no longer writes `blocked` for an informational usage
 * warning), and `window` names the window `resetsAt` belongs to (contract
 * `codingSessionBlocked.windows`: `session` | `weekly` | `model`) so the
 * parent knows whether it is waiting out five hours or a week. Names the
 * reset verbatim as the device reported it (an ISO stamp the parent can
 * compare against); with no reset time it says so in the same parenthetical
 * shape `formatChildEndedSilently` uses, rather than inventing a timestamp.
 * An unknown window (empty/null) falls back to the window-less wording. */
export function formatChildRateLimited(
  child: ChildRunRef,
  blocked: { resetsAt?: string | null; window?: string | null }
): string {
  const window = blocked.window ? ` (${oneLine(blocked.window)} window)` : ``
  const label = `${CHILD_RUN_TAG} ${childRunLabel(child)} is rate limited${window}`
  return blocked.resetsAt
    ? `[${label} until ${oneLine(blocked.resetsAt)}]`
    : `[${label} (no reset time reported)]`
}

/** A header-less caller (a plain `expu_`-key orchestrator) messaging a run
 * it started or owns. */
export function formatStarterMessage(text: string): string {
  return `[Message from your starter via exponential_sessions_message] ${oneLine(text)}`
}

/**
 * EXP-897: a question escalated PAST the immediate parent (`to: 'root'`). The
 * receiving run is not the asker's starter, so the prefix says how far down it
 * came from — and still names the full uuid, because the answer goes straight
 * back to the asker with `exponential_sessions_message`.
 */
export function formatDescendantQuestion(
  asker: ChildRunRef,
  question: string,
  depth: number
): string {
  const levels = depth === 1 ? `1 level down` : `${depth} levels down`
  return `[${CHILD_RUN_TAG} ${childRunLabel(asker)} (${levels}) asks — reply with exponential_sessions_message sessionId=${asker.id}] ${oneLine(question)}`
}

/** The parent answering its own child's ask — distinct prefix so the child
 * can match the reply to its question. */
export function formatParentAnswer(
  parentSessionId: string,
  text: string
): string {
  return `[Answer from your parent run ${parentSessionId.slice(0, 8)} via exponential_sessions_message] ${oneLine(text)}`
}

// The parent row reached through the child's parent_session_id self-FK.
const parentSessions = alias(codingSessions, `parent_sessions`)

export interface ChildParentContext {
  id: string
  userId: string
  hostUserId: string | null
  startedReason: string | null
  parentSessionId: string | null
  actionName: string | null
  issueIdentifier: string | null
  /** null = no parent row (never linked, or since deleted). */
  parentStatus: string | null
}

/** ONE select: the child, its issue identifier and its parent's status via
 * a self-join — single-query on purpose, like every MCP tool lookup. */
export async function loadChildParentContext(
  db: Context[`db`],
  childSessionId: string
): Promise<ChildParentContext | null> {
  const [row] = await db
    .select({
      id: codingSessions.id,
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      startedReason: codingSessions.startedReason,
      parentSessionId: codingSessions.parentSessionId,
      actionName: codingSessions.actionName,
      issueIdentifier: issues.identifier,
      parentStatus: parentSessions.status,
    })
    .from(codingSessions)
    .leftJoin(issues, eq(issues.id, codingSessions.issueId))
    .leftJoin(parentSessions, eq(parentSessions.id, codingSessions.parentSessionId))
    .where(eq(codingSessions.id, childSessionId))
    .limit(1)
  return row ?? null
}

/**
 * EXP-906: the LIVE run a child's messages go to. The immediate parent when
 * it is still live; otherwise the newest live run in the parent's RESUME
 * succession (`resumed_from_id` walked forward): an account switch or a
 * resume relaunches the parent under a new id, and `codingSessions.start`
 * re-stamps the children onto it — but a child that ends inside that window,
 * or one whose re-stamp failed, still points at the ended predecessor. Null
 * = nobody is listening (no parent, or every run in the succession ended).
 */
export async function resolveLiveParentSessionId(
  db: Context[`db`],
  child: Pick<ChildParentContext, `parentSessionId` | `parentStatus`>,
  maxDepth = MAX_SESSION_CHAIN_DEPTH
): Promise<string | null> {
  if (!child.parentSessionId) return null
  if (
    child.parentStatus &&
    (PARENT_LIVE_STATUSES as readonly string[]).includes(child.parentStatus)
  ) {
    return child.parentSessionId
  }
  const result = await db.execute(sql`
    with recursive succession as (
      select cs.id, cs.status, 0 as hops
      from coding_sessions cs
      where cs.id = ${child.parentSessionId}::uuid
      union all
      select s.id, s.status, succession.hops + 1
      from coding_sessions s
      join succession on s.resumed_from_id = succession.id
      where succession.hops < ${maxDepth}
    )
    select id from succession
    where status in (${sql.join(
      PARENT_LIVE_STATUSES.map((status) => sql`${status}`),
      sql`, `
    )})
    order by hops desc
    limit 1
  `)
  const row = (result.rows ?? [])[0]
  return row ? ((row.id as string | null) ?? null) : null
}

/** One row of a run's ancestry, `depth` counted from the run itself (0). */
export interface SessionChainRow {
  id: string
  userId: string
  hostUserId: string | null
  teamId: string | null
  status: string | null
  startedReason: string | null
  parentSessionId: string | null
  actionName: string | null
  issueIdentifier: string | null
  depth: number
}

export interface SessionChain {
  self: SessionChainRow
  /** Nearest parent first, root last. */
  ancestors: SessionChainRow[]
  /** How far down the tree `self` sits (0 = a root run). */
  depth: number
  rootSessionId: string
  /** The HIGHEST still-live ancestor — where `to: 'root'` delivers. */
  topLiveAncestorId: string | null
}

/** Nested runs deeper than this are a bug, not a workflow — the CTE stops. */
const MAX_SESSION_CHAIN_DEPTH = 20

/**
 * EXP-897: a run's whole ancestry in ONE recursive walk up
 * `parent_session_id`. `loadChildParentContext` answers the parent question;
 * this answers the ROOT question (escalation past a parent that cannot decide)
 * and the depth every session list indents by.
 */
export async function loadSessionChain(
  db: Context[`db`],
  sessionId: string,
  maxDepth = MAX_SESSION_CHAIN_DEPTH
): Promise<SessionChain | null> {
  const result = await db.execute(sql`
    with recursive chain as (
      select cs.id, cs.user_id, cs.host_user_id, cs.team_id, cs.status,
             cs.started_reason, cs.parent_session_id, cs.action_name,
             cs.issue_id, 0 as depth
      from coding_sessions cs
      where cs.id = ${sessionId}::uuid
      union all
      select p.id, p.user_id, p.host_user_id, p.team_id, p.status,
             p.started_reason, p.parent_session_id, p.action_name,
             p.issue_id, c.depth + 1
      from coding_sessions p
      join chain c on p.id = c.parent_session_id
      where c.depth < ${maxDepth}
    )
    select chain.id, chain.user_id, chain.host_user_id, chain.team_id,
           chain.status, chain.started_reason, chain.parent_session_id,
           chain.action_name, chain.depth, i.identifier as issue_identifier
    from chain
    left join issues i on i.id = chain.issue_id
    order by chain.depth asc
  `)
  const rows = (result.rows ?? []).map((row) => ({
    id: row.id as string,
    userId: row.user_id as string,
    hostUserId: (row.host_user_id as string | null) ?? null,
    teamId: (row.team_id as string | null) ?? null,
    status: (row.status as string | null) ?? null,
    startedReason: (row.started_reason as string | null) ?? null,
    parentSessionId: (row.parent_session_id as string | null) ?? null,
    actionName: (row.action_name as string | null) ?? null,
    issueIdentifier: (row.issue_identifier as string | null) ?? null,
    depth: Number(row.depth ?? 0),
  }))
  const self = rows.find((row) => row.depth === 0)
  if (!self) return null
  const ancestors = rows.filter((row) => row.depth > 0)
  const live = ancestors.filter(
    (row) =>
      row.status && (PARENT_LIVE_STATUSES as readonly string[]).includes(row.status)
  )
  return {
    self,
    ancestors,
    depth: ancestors.length,
    rootSessionId: ancestors.at(-1)?.id ?? self.id,
    // The highest live one: escalation goes as far UP as someone can still
    // read it, never to a dead root.
    topLiveAncestorId: live.at(-1)?.id ?? null,
  }
}

/**
 * EXP-897: how deep each of these runs sits in its tree (0 = a root run), in
 * ONE walk up from all of them at once. The session lists indent by it; a row
 * whose ancestry is unreadable simply reads 0.
 */
export async function loadSessionDepths(
  db: Context[`db`],
  sessionIds: string[],
  maxDepth = MAX_SESSION_CHAIN_DEPTH
): Promise<Map<string, number>> {
  const depths = new Map<string, number>()
  if (sessionIds.length === 0) return depths
  const result = await db.execute(sql`
    with recursive up as (
      select cs.id as origin_id, cs.id, cs.parent_session_id, 0 as depth
      from coding_sessions cs
      where cs.id in (${sql.join(
        sessionIds.map((id) => sql`${id}::uuid`),
        sql`, `
      )})
      union all
      select u.origin_id, p.id, p.parent_session_id, u.depth + 1
      from coding_sessions p
      join up u on p.id = u.parent_session_id
      where u.depth < ${maxDepth}
    )
    select origin_id, max(depth) as depth from up group by origin_id
  `)
  for (const row of result.rows ?? []) {
    depths.set(row.origin_id as string, Number(row.depth ?? 0))
  }
  return depths
}

/**
 * EXP-897: every run in the subtree rooted at `rootSessionId`, the root
 * INCLUDED. The caller still applies its own access filter to the rows — this
 * only narrows the candidate set.
 */
export async function loadSubtreeSessionIds(
  db: Context[`db`],
  rootSessionId: string,
  maxDepth = MAX_SESSION_CHAIN_DEPTH
): Promise<string[]> {
  const result = await db.execute(sql`
    with recursive down as (
      select cs.id, 0 as depth
      from coding_sessions cs
      where cs.id = ${rootSessionId}::uuid
      union all
      select c.id, d.depth + 1
      from coding_sessions c
      join down d on c.parent_session_id = d.id
      where d.depth < ${maxDepth}
    )
    select distinct id from down
  `)
  return (result.rows ?? []).map((row) => row.id as string)
}

/**
 * Tell a live parent that its agent-started child ended. `summary` null =
 * the child ended WITHOUT reporting (the client end path — a closed tab or
 * dead daemon); `endedBy` then names the ender. No-op unless the child is
 * agent-started with a linked, live parent and the relay is configured.
 * Best-effort and never throws — an end must never fail on its notification.
 */
export async function notifyParentOfChildEnd(
  db: Context[`db`],
  childSessionId: string,
  end: { summary: string | null; endedBy: string }
): Promise<{ delivered: boolean }> {
  try {
    const child = await loadChildParentContext(db, childSessionId)
    if (!child || child.startedReason !== `agent` || !child.parentSessionId) {
      return { delivered: false }
    }
    // EXP-906: the parent may have resumed under a new id since.
    const target = await resolveLiveParentSessionId(db, child)
    if (!target) return { delivered: false }
    const config = getSteerRelayConfig()
    if (!config) return { delivered: false }
    const message =
      end.summary !== null
        ? formatChildFinished(child, end.summary)
        : formatChildEndedSilently(child, end.endedBy)
    return await relayPostInput(config, target, message)
  } catch {
    return { delivered: false }
  }
}

/**
 * EXP-804: tell a live parent that its agent-started child ran into its
 * agent's usage wall. Same gating as `notifyParentOfChildEnd` (agent-started
 * child + a linked, live parent + a configured relay), same best-effort
 * contract: never throws, a failure just reads as not-delivered.
 *
 * The CALLER fires this only on the null → set transition
 * (`codingSessions.setBlocked`), so a device retrying the write sends exactly
 * one message per wall — the parent must not be nagged every heartbeat.
 * `window` rides through to the message (FEED-37) when the device named it.
 */
export async function notifyParentOfChildBlocked(
  db: Context[`db`],
  childSessionId: string,
  blocked: { resetsAt?: string | null; window?: string | null }
): Promise<{ delivered: boolean }> {
  try {
    const child = await loadChildParentContext(db, childSessionId)
    if (!child || child.startedReason !== `agent` || !child.parentSessionId) {
      return { delivered: false }
    }
    // EXP-906: the parent may have resumed under a new id since.
    const target = await resolveLiveParentSessionId(db, child)
    if (!target) return { delivered: false }
    const config = getSteerRelayConfig()
    if (!config) return { delivered: false }
    return await relayPostInput(
      config,
      target,
      formatChildRateLimited(child, blocked)
    )
  } catch {
    return { delivered: false }
  }
}

import { TRPCError } from "@trpc/server"
import { and, asc, eq, isNotNull, ne, sql } from "drizzle-orm"
import { sessions, type Session } from "@/db/schema"
import { postAgentMessage } from "@/lib/sessions/messages"
import { endRowsInTx } from "@/lib/sessions/lifecycle"
import { isSealedBody } from "@/lib/sessions/sealed"
import { treeIsKeyed } from "@/lib/sessions/send"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1731 (packages/eagent/fixtures/subagents.json): a CLI subagent
// (Claude's Task/Agent tool and its workflow agents, Codex's spawned
// threads) is a child `sessions` row the DEVICE writes (`sessions.subagent`):
// one below the session running the CLI, `subagent_key` = the CLI's own id
// of it. It shares its parent's device, repository, branch and runner, never
// queues, takes no brief and no input (a send goes to its parent: send.ts),
// holds no tree slot (send.ts `assertTreeRoom`), is never killed (lifecycle
// `relayKillSessionsBestEffort`), never posts into its parent (bus.ts
// `busListeners`) and is never looked at by the watchdog or the heartbeat
// (watchdog.ts). A parent whose turn is over ends its strays
// (`sweepSubagentsInTx`, sessions.report).

// The spec's copy (eagent rules.json `copy.subagent*`, drift-gated by
// eagent-spec.test.ts).
export const SUBAGENT_NO_KEY = `A subagent names its key`
export const SUBAGENT_NO_CHILDREN = `A subagent starts no sessions: start it under its parent`
export const SUBAGENT_FAILED_BARE = `Failed.`
export const subagentFailedText = (summary: string) => `Failed: ${summary}`
export const subagentRedirectText = (id: string, parentId: string) =>
  `${id} is a subagent: it takes no messages of its own, so this went to its parent ${parentId}.`

export const SUBAGENT_KEY_MAX = 200
export const SUBAGENT_TITLE_MAX = 200
export const SUBAGENT_SUMMARY_MAX = 4000

export const isSubagent = (session: Pick<Session, `subagentKey`>) => session.subagentKey != null

/** A subagent's last line: its summary, a failure saying so; `` = none. */
export function subagentEndText(summary: string | null | undefined, failed: boolean): string {
  const said = summary?.trim() ?? ``
  if (!failed) return said
  return said ? subagentFailedText(said) : SUBAGENT_FAILED_BARE
}

export interface SubagentEdge {
  /** The parent: the session whose CLI runs the subagent. */
  sessionId: string
  /** The reporting device (another device's edge is ignored, like a report). */
  deviceId?: string
  key: string
  status: `running` | `ended`
  title?: string | null
  summary?: string | null
  failed?: boolean
}

type SubagentRow = Pick<Session, `id` | `parentId` | `depth` | `agentId` | `status`>

const ROW = {
  id: sessions.id,
  parentId: sessions.parentId,
  depth: sessions.depth,
  agentId: sessions.agentId,
  status: sessions.status,
}

/** End a subagent row: its last line (non-waking, agent-authored on
 *  itself), then End as tree.json minus the kill and minus any report into
 *  its parent (the bus skips it). Its watchers hear the report. */
async function endSubagentInTx(tx: Tx, row: SubagentRow, text: string): Promise<string[]> {
  if (text) await postAgentMessage(tx, row.id, text)
  return endRowsInTx(tx, [row], { id: row.id, summary: text || null })
}

/** A parent whose turn is over (not busy: idle, a lost process, its end)
 *  runs no subagent any more: the live ones its device never closed end,
 *  oldest first. Call AFTER the parent's write, BEFORE the parent ends. */
export async function sweepSubagentsInTx(tx: Tx, parentId: string): Promise<string[]> {
  const rows = await tx
    .select(ROW)
    .from(sessions)
    .where(and(eq(sessions.parentId, parentId), isNotNull(sessions.subagentKey), ne(sessions.status, `ended`)))
    .orderBy(asc(sessions.createdAt), asc(sessions.id))
    .for(`update`)
  const ended: string[] = []
  for (const row of rows) ended.push(...(await endSubagentInTx(tx, row, ``)))
  return ended
}

/**
 * The device's subagent edge. `parent` = the loaded (device-authorized)
 * parent row. The first edge of a key under the parent writes the row (a
 * late first `ended` writes it and ends it); later `running` edges are
 * progress (an ended key is live again in place), `ended` its End. Every
 * edge stamps `last_activity_at` on the row AND the parent. → the subagent
 * row's id, null = ignored (the parent ended, or another device's edge).
 */
export async function subagentEdgeInTx(
  tx: Tx,
  parent: Pick<Session, `id` | `subagentKey`>,
  edge: SubagentEdge,
  now: Date = new Date()
): Promise<{ sessionId: string | null; endedIds: string[] }> {
  const key = edge.key.trim()
  if (!key) throw new TRPCError({ code: `BAD_REQUEST`, message: SUBAGENT_NO_KEY })
  if (isSubagent(parent)) throw new TRPCError({ code: `BAD_REQUEST`, message: SUBAGENT_NO_CHILDREN })
  // The parent's row lock serialises its edges (and a racing sweep); its
  // state is re-read under it.
  const [live] = await tx.select().from(sessions).where(eq(sessions.id, parent.id)).for(`update`)
  if (!live) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  if (live.status === `ended` || (edge.deviceId !== undefined && edge.deviceId !== live.deviceId)) {
    return { sessionId: null, endedIds: [] }
  }
  await tx.update(sessions).set({ lastActivityAt: now }).where(eq(sessions.id, live.id))
  // EXP-1458 / #FEED-192: a keyed tree stores no plaintext: the CLI's own
  // words (the title, the summary) are dropped there.
  const keyed = await treeIsKeyed(tx, live.rootId)
  const plain = (text: string | null | undefined) => (text && !keyed && !isSealedBody(text) ? text : null)
  const title = plain(edge.title?.trim())?.slice(0, SUBAGENT_TITLE_MAX) ?? null

  const [inserted] = await tx
    .insert(sessions)
    .values({
      teamId: live.teamId,
      parentId: live.id,
      rootId: live.rootId,
      depth: live.depth + 1,
      userId: live.userId,
      shared: live.shared,
      deviceId: live.deviceId,
      // It shares the parent's worktree and branch, never its own.
      repositoryId: live.repositoryId,
      branch: live.branch,
      cli: live.cli,
      account: live.account,
      model: live.model,
      effort: live.effort,
      subagentKey: key,
      title,
      status: `running`,
      busy: true,
      startedAt: now,
      lastActivityAt: now,
      createdAt: now,
      updatedAt: now,
    })
    // Two racing first edges make one row.
    .onConflictDoNothing({ target: [sessions.parentId, sessions.subagentKey], where: sql`subagent_key IS NOT NULL` })
    .returning(ROW)
  let row: SubagentRow
  if (inserted) {
    row = inserted
  } else {
    const [existing] = await tx
      .select(ROW)
      .from(sessions)
      .where(and(eq(sessions.parentId, live.id), eq(sessions.subagentKey, key)))
      .for(`update`)
      .limit(1)
    if (!existing) throw new TRPCError({ code: `CONFLICT`, message: `Subagent row raced` })
    row = existing
    const revive = edge.status !== `ended` && existing.status === `ended`
    await tx
      .update(sessions)
      .set({
        // A resumed subagent (its CLI ran the same key again) is live again in place.
        ...(revive ? { status: `running` as const, busy: true, endedAt: null, updatedAt: now } : {}),
        lastActivityAt: now,
      })
      .where(eq(sessions.id, existing.id))
    if (revive) row = { ...existing, status: `running` }
  }
  if (edge.status === `ended` && row.status !== `ended`) {
    const text = subagentEndText(plain(edge.summary), edge.failed === true)
    return { sessionId: row.id, endedIds: await endSubagentInTx(tx, row, text) }
  }
  return { sessionId: row.id, endedIds: [] }
}

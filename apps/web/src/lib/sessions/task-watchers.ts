// #NEW-147 (packages/emcp fixtures/tasks.json, D4 of #NEW-143): an external
// MCP host's TASK is the second kind of watcher on `session_watches`
// (`watcher_kind` mcp_task, `task_id`; packages/eagent rules.json
// `watcher`). ONE system: the bus that posts a session watcher's wake lines
// drives a task watcher instead (it never posts for it), and `watch.fire`
// completes one. What a live session task shows (working, its open cards
// as `input_required`) is derived live by `tasks/get` (lib/emcp/tasks.ts);
// what lands here is every change's `last_updated_at` stamp and the
// TERMINAL outcome (the report, a failure, an event firing), kept on the
// `mcp_tasks` row after the watched session's rows are gone.
import { and, asc, desc, eq, inArray, lte, sql, type SQL } from "drizzle-orm"
import { isTerminal, taskReduce, type SurfaceCard, type TaskEvent, type TaskState, type TaskStatus } from "@exp/emcp/tasks"
import { db } from "@/db/connection"
import { mcpElicitations, mcpRequestStates, mcpTasks, messages, sessionWatches, type SessionWatchKind } from "@/db/schema"
import { inSession } from "@/lib/sessions/messages"
import { isSealedCard } from "@/lib/sessions/sealed"
import type { Tx } from "@/lib/sessions/tx"

type TaskRow = typeof mcpTasks.$inferSelect

const LIVE: TaskStatus[] = [`working`, `input_required`]

/** The task row as the reference's state (no cards: a terminal step clears them). */
export function taskStateOf(row: TaskRow): TaskState {
  return {
    taskId: row.id,
    principal: row.principal,
    watch: row.kind,
    sessionId: row.kind === `session` ? row.ref : null,
    transferId: row.kind === `transfer` ? row.ref : null,
    status: row.status,
    statusMessage: row.statusMessage,
    createdAt: row.createdAt.getTime(),
    lastUpdatedAt: row.lastUpdatedAt.getTime(),
    cards: {},
    result: row.result ?? null,
    error: row.error ?? null,
  }
}

/** Persist the reduced state of `row` (terminal only: live state is derived). */
async function persist(tx: Tx, next: TaskState): Promise<void> {
  await tx
    .update(mcpTasks)
    .set({
      status: next.status,
      statusMessage: next.statusMessage,
      result: (next.result ?? null) as Record<string, unknown> | null,
      error: next.error,
      lastUpdatedAt: new Date(next.lastUpdatedAt),
    })
    .where(and(eq(mcpTasks.id, next.taskId), inArray(mcpTasks.status, LIVE)))
}

/** The live tasks watching `subjectId` with one of `kinds` (locked). */
async function liveTasksWatching(tx: Tx, subjectId: string, kinds: readonly SessionWatchKind[], extra?: SQL): Promise<TaskRow[]> {
  return tx
    .select()
    .from(mcpTasks)
    .where(
      and(
        inArray(mcpTasks.status, LIVE),
        sql`${mcpTasks.id} IN (SELECT sw.task_id FROM ${sessionWatches} sw WHERE sw.watcher_kind = 'mcp_task' AND sw.subject_id = ${subjectId} AND sw.kind IN (${sql.join(
          kinds.map((kind) => sql`${kind}`),
          sql`, `
        )})${extra ? sql` AND ${extra}` : sql``})`
      )
    )
    .orderBy(asc(mcpTasks.id))
    .for(`update`)
}

/** A transition of `subjectId` (any bus line about it): its session tasks'
 *  `lastUpdatedAt` moves (their status is derived live). */
export async function touchSessionTasks(tx: Tx, subjectId: string, at: Date = new Date()): Promise<void> {
  await tx
    .update(mcpTasks)
    .set({ lastUpdatedAt: at })
    .where(
      and(
        inArray(mcpTasks.status, LIVE),
        sql`${mcpTasks.id} IN (SELECT sw.task_id FROM ${sessionWatches} sw WHERE sw.watcher_kind = 'mcp_task' AND sw.kind = 'session' AND sw.subject_id = ${subjectId})`
      )
    )
}

async function dropTaskWatches(tx: Tx, taskIds: readonly string[]): Promise<void> {
  if (taskIds.length === 0) return
  await tx.delete(sessionWatches).where(inArray(sessionWatches.taskId, [...taskIds]))
}

/** The fixture's failure keys a report may carry (`failures`). */
export type TaskFailure = `stalled` | `killed` | `failed`

/** `subjectId` ended (its report): every live task on it turns `completed`
 *  with the summary as its result, or `failed` with a failure reason; its
 *  watcher rows go. */
export async function endSessionTasks(
  tx: Tx,
  subjectId: string,
  outcome: { summary: string; reason?: TaskFailure | null },
  at: Date = new Date()
): Promise<string[]> {
  const rows = await liveTasksWatching(tx, subjectId, [`session`])
  // #NEW-149 (tasks.json GENUI): a completed task carries the session's last
  // surface card for the host's genUI view (MCP Apps).
  const surface = rows.length > 0 && !outcome.reason ? await lastSurfaceCard(tx, subjectId) : null
  const event: TaskEvent = { kind: `session.end`, at: at.getTime(), summary: outcome.summary, reason: outcome.reason ?? null, surface }
  for (const row of rows) await persist(tx, taskReduce(taskStateOf(row), event))
  await dropTaskWatches(tx, rows.map((row) => row.id))
  return rows.map((row) => row.id)
}

/** The last surface card `sessionId` posted (never a sealed one: the server cannot read it), or null. */
export async function lastSurfaceCard(tx: Tx, sessionId: string): Promise<SurfaceCard | null> {
  const [row] = await tx
    .select({ card: messages.card })
    .from(messages)
    .where(and(inSession(sessionId), sql`${messages.card}->>'kind' = 'surface'`))
    .orderBy(desc(messages.createdAt), desc(messages.id))
    .limit(1)
  const card = row?.card as { kind?: unknown; surface?: unknown; title?: unknown } | null | undefined
  if (!card || isSealedCard(card) || card.kind !== `surface` || !card.surface || typeof card.surface !== `object`) return null
  return { kind: `surface`, surface: card.surface as SurfaceCard[`surface`], title: typeof card.title === `string` ? card.title : null }
}

/** `watch.fire` for task watchers: every live task holding a fired watch
 *  on `subjectId` turns `completed` with `text`; ALL its watches go. */
export async function fireTaskWatches(
  tx: Tx,
  args: { kinds: readonly SessionWatchKind[]; subjectId: string; text: string; statuses?: readonly string[] },
  at: Date = new Date()
): Promise<string[]> {
  const statuses = (args.statuses ?? []).map((status) => status.toLowerCase())
  const issueFilter = args.kinds.length === 1 && args.kinds[0] === `issue_status`
    ? statuses.length > 0
      ? sql`(sw.status IS NULL OR sw.status IN (${sql.join(statuses.map((s) => sql`${s}`), sql`, `)}))`
      : sql`sw.status IS NULL`
    : undefined
  const rows = await liveTasksWatching(tx, args.subjectId, args.kinds, issueFilter)
  const event: TaskEvent = { kind: `watch.fire`, at: at.getTime(), text: args.text }
  for (const row of rows) await persist(tx, taskReduce(taskStateOf(row), event))
  await dropTaskWatches(tx, rows.map((row) => row.id))
  return rows.map((row) => row.id)
}

/** #FEED-295: `transferId` ended: every live task watching it turns
 *  completed / failed / cancelled with its end line (tasks.json TRANSFERS). */
export async function endTransferTasks(
  tx: Tx,
  transferId: string,
  outcome: { status: `done` | `failed` | `cancelled`; text: string },
  at: Date = new Date()
): Promise<string[]> {
  const rows = await tx
    .select()
    .from(mcpTasks)
    .where(and(eq(mcpTasks.kind, `transfer`), eq(mcpTasks.ref, transferId), inArray(mcpTasks.status, LIVE)))
    .orderBy(asc(mcpTasks.id))
    .for(`update`)
  const event: TaskEvent = { kind: `transfer.end`, at: at.getTime(), status: outcome.status, text: outcome.text }
  for (const row of rows) await persist(tx, taskReduce(taskStateOf(row), event))
  return rows.map((row) => row.id)
}

/** The watchdog: the task watchers' time watches that came due complete their task. */
export async function fireDueTaskTimeWatches(now: Date, text: (label: string) => string): Promise<number> {
  return db.transaction(async (tx) => {
    const due = await tx
      .select({ taskId: sessionWatches.taskId, label: sessionWatches.label })
      .from(sessionWatches)
      .where(and(eq(sessionWatches.watcherKind, `mcp_task`), eq(sessionWatches.kind, `at`), lte(sessionWatches.at, now)))
      .orderBy(asc(sessionWatches.taskId), asc(sessionWatches.at))
      .limit(500)
      .for(`update`, { skipLocked: true })
    const first = new Map<string, string>()
    for (const row of due) if (row.taskId && !first.has(row.taskId)) first.set(row.taskId, row.label)
    if (first.size === 0) return 0
    const rows = await tx
      .select()
      .from(mcpTasks)
      .where(and(inArray(mcpTasks.id, [...first.keys()]), inArray(mcpTasks.status, LIVE)))
      .for(`update`)
    for (const row of rows) {
      await persist(tx, taskReduce(taskStateOf(row), { kind: `watch.fire`, at: now.getTime(), text: text(first.get(row.id)!) }))
    }
    await dropTaskWatches(tx, [...first.keys()])
    return rows.length
  })
}

/** A continuation (`handOverWatches`) carries the tasks watching `ended` over to `next`. */
export async function handOverTaskWatches(tx: Tx, ended: string, next: string): Promise<void> {
  const rows = await tx
    .select({ id: sessionWatches.id, taskId: sessionWatches.taskId, label: sessionWatches.label })
    .from(sessionWatches)
    .where(and(eq(sessionWatches.watcherKind, `mcp_task`), eq(sessionWatches.kind, `session`), eq(sessionWatches.subjectId, ended)))
  for (const row of rows) {
    await tx.delete(sessionWatches).where(eq(sessionWatches.id, row.id))
    await tx
      .insert(sessionWatches)
      .values({ watcherKind: `mcp_task`, taskId: row.taskId, sessionId: null, kind: `session`, subjectId: next, label: row.label })
      .onConflictDoNothing()
    await tx.update(mcpTasks).set({ ref: next, lastUpdatedAt: new Date() }).where(and(eq(mcpTasks.id, row.taskId!), eq(mcpTasks.ref, ended)))
  }
}

/** Is `status` terminal (the row's stored status). */
export const taskRowTerminal = (row: Pick<TaskRow, `status`>) => isTerminal(row.status)

/** The watchdog's sweep: expired tasks (their watcher rows cascade; a
 *  watched session keeps running, an ordinary session), spent
 *  requestStates and URL elicitations past their expiry. */
export async function sweepExpiredMcpRows(now: Date = new Date()): Promise<{ tasks: number; states: number; elicitations: number }> {
  const tasks = await db.delete(mcpTasks).where(lte(mcpTasks.expiresAt, now)).returning({ id: mcpTasks.id })
  const states = await db.delete(mcpRequestStates).where(lte(mcpRequestStates.expiresAt, now)).returning({ jti: mcpRequestStates.jti })
  const elicitations = await db.delete(mcpElicitations).where(lte(mcpElicitations.expiresAt, now)).returning({ id: mcpElicitations.id })
  return { tasks: tasks.length, states: states.length, elicitations: elicitations.length }
}

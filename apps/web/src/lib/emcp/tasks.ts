// #NEW-147 (packages/emcp fixtures/tasks.json, D4 of #NEW-143): MCP tasks
// (`io.modelcontextprotocol/tasks`) on `/api/mcp`, unified with the session
// watches: a task is an `mcp_tasks` row plus its WATCHER rows on
// `session_watches` (lib/sessions/task-watchers.ts). Only an external
// caller (not inside a session of its own) whose `tools/call` declared the
// extension gets one, from `exponential_sessions_send` (`taskWatchFor`):
// the call's ordinary work first, then the CreateTaskResult. `tasks/get` is
// derived LIVE from the watched session (its digest's last line, its open
// ask / plan cards as `input_required`); `tasks/update` answers a card
// through `sessions.send {sessionId, answer}` (card-answer: first wins);
// `tasks/cancel` stops the session. The watchdog sweeps expired rows.
import { randomBytes } from "node:crypto"
import { ProtocolError, type McpServer } from "@modelcontextprotocol/server"
import { z } from "zod"
import { and, asc, count, eq, gt, inArray, isNull, sql } from "drizzle-orm"
import { parseSessionWatchSpec, sessionsSendInputSchema, type MessageCard } from "@exp/db-schema/domain"
import { newBurstState, type BurstState } from "@exp/emcp/deletes"
import {
  TASK_TTL_MS,
  taskHandle,
  taskReduce,
  taskWatchFor,
  taskWire,
  type Card,
  type TaskEffect,
  type TaskState,
} from "@exp/emcp/tasks"
import { db } from "@/db/connection"
import { mcpTasks, messages, sessions, sessionWatches } from "@/db/schema"
import { getUserTeamIds } from "@/lib/auth/membership"
import { reportSummary } from "@/lib/sessions/bus"
import { stopSession } from "@/lib/sessions/lifecycle"
import { inSession } from "@/lib/sessions/messages"
import { isSealedCard } from "@/lib/sessions/sealed"
import { sendSession } from "@/lib/sessions/send"
import { endSessionTasks, taskStateOf } from "@/lib/sessions/task-watchers"
import { resolveWatchSpecs, sessionWatchLabels } from "@/lib/sessions/watches"
import { isTeamFullyGranted, type McpAccess } from "./scope"
import type { EmcpContext, Principal } from "./request-context"

/** `user:<id>|client:<grant>`: who a task belongs to (no other principal finds it). */
export const taskPrincipal = (principal: Principal) => `user:${principal.userId}|client:${principal.grant}`

/** `tsk_` + 32 random bytes base64url: unguessable. */
export const newTaskId = () => `tsk_${randomBytes(32).toString(`base64url`)}`

const LIVE = [`working`, `input_required`] as const

// ── Cards as the reference reads them ────────────────────────────────

/** A waiting question / plan card as tasks.json's `Card`. */
export function cardOf(card: MessageCard): Card | null {
  const sealed = isSealedCard(card)
  if (card.kind === `question`) {
    return {
      kind: `ask`,
      question: card.question ?? ``,
      options: (card.options ?? []).filter((option): option is string => typeof option === `string`),
      ...(card.multi ? { multi: true } : {}),
      ...(card.custom === false ? { custom: false } : {}),
      ...(sealed ? { sealed: true } : {}),
    }
  }
  if (card.kind === `plan`) {
    return {
      kind: `plan`,
      text: card.plan ?? ``,
      questions: (card.questions ?? []).map((q) => ({ id: q.id, prompt: q.prompt, options: q.options.map((option) => option.label) })),
      ...(sealed ? { sealed: true } : {}),
    }
  }
  return null
}

// ── The live state ────────────────────────────────────────────────────

/** The task as `tasks/get` answers it NOW: a terminal row as stored, a live
 *  session task derived from its session (digest, open cards), an ended
 *  session the bus missed completed on the spot. */
export async function loadTaskState(taskId: string): Promise<TaskState | null> {
  const [row] = await db.select().from(mcpTasks).where(eq(mcpTasks.id, taskId)).limit(1)
  if (!row) return null
  let state = taskStateOf(row)
  if (!(LIVE as readonly string[]).includes(row.status) || row.kind !== `session` || !row.ref) return state
  const [session] = await db
    .select({ id: sessions.id, status: sessions.status, digest: sessions.digest })
    .from(sessions)
    .where(eq(sessions.id, row.ref))
    .limit(1)
  if (!session) return state
  if (session.status === `ended`) {
    await db.transaction(async (tx) => endSessionTasks(tx, session.id, { summary: await reportSummary(tx, session) }))
    const [ended] = await db.select().from(mcpTasks).where(eq(mcpTasks.id, taskId)).limit(1)
    return ended ? taskStateOf(ended) : null
  }
  const at = row.lastUpdatedAt.getTime()
  state = taskReduce(state, { kind: `session.transition`, at, digest: session.digest?.preview?.body ?? `` })
  const open = await db
    .select({ id: messages.id, card: messages.card })
    .from(messages)
    .where(and(inSession(session.id), isNull(messages.answeredAt), sql`${messages.card}->>'kind' IN ('question', 'plan')`))
    .orderBy(asc(messages.createdAt), asc(messages.id))
  for (const message of open) {
    const card = message.card ? cardOf(message.card) : null
    if (card) state = taskReduce(state, { kind: `card.open`, at, messageId: message.id, card })
  }
  return state
}

// ── Creating one ──────────────────────────────────────────────────────

async function liveTaskCount(principal: string, now: Date): Promise<number> {
  const [row] = await db
    .select({ n: count() })
    .from(mcpTasks)
    .where(and(eq(mcpTasks.principal, principal), inArray(mcpTasks.status, [...LIVE]), gt(mcpTasks.expiresAt, now)))
  return Number(row?.n ?? 0)
}

export interface TaskCallSpec {
  ctx: EmcpContext
  /** The raw arguments of `exponential_sessions_send`. */
  args: Record<string, unknown>
  /** The caller runs inside a session of its own (our agents). */
  inSession: boolean
  access: McpAccess
}

/** What a sessions_send call's task would watch, or null = answer as today. */
export async function plannedTaskWatch(spec: TaskCallSpec) {
  if (spec.inSession || !spec.ctx.client.tasks) return null
  const principal = taskPrincipal(spec.ctx.principal)
  return taskWatchFor({
    tool: `exponential_sessions_send`,
    args: spec.args,
    inSession: spec.inSession,
    declared: spec.ctx.client.tasks,
    liveTasks: await liveTaskCount(principal, new Date()),
  })
}

async function insertTask(
  spec: TaskCallSpec,
  values: { kind: `session` | `events`; ref: string | null; teamId: string | null },
  watches: Array<Omit<typeof sessionWatches.$inferInsert, `taskId` | `watcherKind` | `sessionId`>>
): Promise<Record<string, unknown>> {
  const now = new Date()
  const id = newTaskId()
  const row = await db.transaction(async (tx) => {
    const [task] = await tx
      .insert(mcpTasks)
      .values({
        id,
        userId: spec.ctx.principal.userId,
        grant: spec.ctx.principal.grant,
        principal: taskPrincipal(spec.ctx.principal),
        teamId: values.teamId,
        kind: values.kind,
        ref: values.ref,
        createdAt: now,
        lastUpdatedAt: now,
        expiresAt: new Date(now.getTime() + TASK_TTL_MS),
      })
      .returning()
    if (watches.length > 0) {
      await tx
        .insert(sessionWatches)
        .values(watches.map((watch) => ({ ...watch, watcherKind: `mcp_task` as const, taskId: id, sessionId: null })))
        .onConflictDoNothing()
    }
    return task!
  })
  const state = values.kind === `session` ? ((await loadTaskState(row.id)) ?? taskStateOf(row)) : taskStateOf(row)
  return { ...taskWire(state), resultType: `task` }
}

/** The CreateTaskResult of a session the call messaged or started. */
export async function createSessionTask(spec: TaskCallSpec, sessionId: string): Promise<Record<string, unknown>> {
  const [session] = await db.select({ teamId: sessions.teamId }).from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  const labels = await sessionWatchLabels(db as never, [sessionId])
  return insertTask(spec, { kind: `session`, ref: sessionId, teamId: session?.teamId ?? null }, [
    { kind: `session`, subjectId: sessionId, label: labels.get(sessionId) ?? `a session` },
  ])
}

/** The CreateTaskResult of a `wakeOn` of event specs (an external caller
 *  has no session to watch from): resolved against the teams its grant
 *  reaches whole. An event that already happened completes it at once. */
export async function createEventTask(spec: TaskCallSpec, specs: string[]): Promise<Record<string, unknown>> {
  const teams = (await getUserTeamIds(spec.ctx.principal.userId)).filter((id) => isTeamFullyGranted(spec.access, id))
  const parsed = specs.map((entry) => {
    const watch = parseSessionWatchSpec(entry)
    if (!watch) throw new Error(`A watch names its pull request or issue, or a time (at).`)
    return watch
  })
  const resolved = await db.transaction((tx) => resolveWatchSpecs(tx, { rootId: ``, repositoryId: null }, parsed, new Date(), teams))
  const created = await insertTask(
    spec,
    { kind: `events`, ref: null, teamId: teams.length === 1 ? teams[0]! : null },
    resolved.watches.map((watch) => ({ kind: watch.kind, subjectId: watch.subjectId, status: watch.status, at: watch.at, label: watch.label }))
  )
  const first = resolved.immediate[0]
  if (!first) return created
  const taskId = created.taskId as string
  await db.transaction(async (tx) => {
    const [row] = await tx.select().from(mcpTasks).where(eq(mcpTasks.id, taskId)).limit(1).for(`update`)
    if (!row) return
    const next = taskReduce(taskStateOf(row), { kind: `watch.fire`, at: Date.now(), text: first.text })
    await tx
      .update(mcpTasks)
      .set({ status: next.status, statusMessage: next.statusMessage, result: next.result as Record<string, unknown>, lastUpdatedAt: new Date(next.lastUpdatedAt) })
      .where(eq(mcpTasks.id, taskId))
    await tx.delete(sessionWatches).where(eq(sessionWatches.taskId, taskId))
  })
  const state = await loadTaskState(taskId)
  return state ? { ...taskWire(state), resultType: `task` } : created
}

// ── tasks/get, tasks/update, tasks/cancel ─────────────────────────────

// Per server process (deletes.json's sliding window): one window per principal.
const rateWindows = new Map<string, BurstState>()

function rateOf(principal: string): BurstState {
  if (rateWindows.size > 4096) rateWindows.clear()
  let state = rateWindows.get(principal)
  if (!state) rateWindows.set(principal, (state = newBurstState()))
  return state
}

/** Tests only. */
export function resetTaskRateWindows(): void {
  rateWindows.clear()
}

async function applyEffects(userId: string, task: TaskState | null, effects: readonly TaskEffect[]): Promise<void> {
  for (const effect of effects) {
    if (effect.kind === `answer`) {
      // Card-answer (first wins): an answer that lost, or a card gone, changes nothing.
      await sendSession({ userId, input: sessionsSendInputSchema.parse({ sessionId: effect.sessionId, answer: effect.answer }), viaMcp: true }).catch(
        (err) => console.warn(`[emcp-tasks] answer not applied:`, err instanceof Error ? err.message : err)
      )
    } else if (effect.kind === `stop`) {
      await stopSession({ userId, sessionId: effect.sessionId }).catch((err) =>
        console.warn(`[emcp-tasks] stop not delivered:`, err instanceof Error ? err.message : err)
      )
    } else if (effect.kind === `unwatch` && task) {
      await db.transaction(async (tx) => {
        await tx
          .update(mcpTasks)
          .set({ status: task.status, statusMessage: task.statusMessage, lastUpdatedAt: new Date(task.lastUpdatedAt) })
          .where(and(eq(mcpTasks.id, effect.taskId), inArray(mcpTasks.status, [...LIVE])))
        await tx.delete(sessionWatches).where(eq(sessionWatches.taskId, effect.taskId))
      })
    }
  }
}

/** One tasks/* request of `ctx`'s principal. */
export async function handleTaskRequest(
  method: `tasks/get` | `tasks/update` | `tasks/cancel`,
  params: Record<string, unknown>,
  ctx: EmcpContext
): Promise<Record<string, unknown>> {
  const principal = taskPrincipal(ctx.principal)
  const taskId = typeof params.taskId === `string` && params.taskId ? params.taskId : null
  const loaded = ctx.client.tasks && taskId ? await loadTaskState(taskId) : null
  const outcome = taskHandle(
    { method, params, principal, declared: ctx.client.tasks, now: Date.now() },
    (id) => (id === taskId ? loaded : null),
    rateOf(principal)
  )
  if (!outcome.ok) throw new ProtocolError(outcome.error.code, outcome.error.message, outcome.error.data)
  await applyEffects(ctx.principal.userId, outcome.task, outcome.effects)
  return outcome.result
}

const looseParams = z.looseObject({})

/** Serves tasks/get, tasks/update and tasks/cancel on this request's server. */
export function registerTasks(server: McpServer, ctx: EmcpContext): void {
  for (const method of [`tasks/get`, `tasks/update`, `tasks/cancel`] as const) {
    // The SDK lifts `inputResponses` off the params into `ctx.mcpReq` (MRTR).
    server.server.setRequestHandler(method, { params: looseParams }, async (params: Record<string, unknown>, sdk: unknown) => {
      const lifted = (sdk as { mcpReq?: { inputResponses?: unknown } } | undefined)?.mcpReq?.inputResponses
      return handleTaskRequest(method, lifted !== undefined && params.inputResponses === undefined ? { ...params, inputResponses: lifted } : params, ctx)
    })
  }
}

import { and, asc, desc, eq, inArray, isNotNull, sql } from "drizzle-orm"
import type { ThreadItemSource } from "@exp/db-schema/domain"
import { boards, issues, threadItems, threads, type Thread } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"

// EXP-1284 — the ONE place threads and their children are written. A thread
// is the unit of work: n issues + m runs (`thread_items`), team-scoped. Every
// start opens or joins one (`joinThreadForStart`); the agent links further
// children over MCP (`exponential_threads_link`/`_unlink`, `issues_create
// {threadId}`), people through tRPC `threads.*`. Authorization stays with the
// callers (resolveTeamAccess / the MCP grant): these helpers only enforce
// the data rules — same team, a run sits in ONE thread, an issue at most once
// per thread.

type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]
// eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
type Db = typeof import("@/db/connection").db | Tx

export interface ThreadRef {
  id: string
  teamId: string
}

export interface ThreadLinkActor {
  /** The person linking (a run's owner when its agent links). */
  userId: string | null
  /** The run whose agent linked it (MCP), else null. */
  sessionId?: string | null
  source: ThreadItemSource
}

export async function getThread(db: Db, threadId: string): Promise<Thread | null> {
  const [row] = await db
    .select()
    .from(threads)
    .where(eq(threads.id, threadId))
    .limit(1)
  return row ?? null
}

/** The thread a run sits in (exactly one; null only for a row the start
 * could not place, e.g. written before the thread migration ran). */
export async function getThreadOfSession(
  db: Db,
  sessionId: string
): Promise<ThreadRef | null> {
  const [row] = await db
    .select({ id: threads.id, teamId: threads.teamId })
    .from(threadItems)
    .innerJoin(threads, eq(threads.id, threadItems.threadId))
    .where(eq(threadItems.sessionId, sessionId))
    .limit(1)
  return row ?? null
}

/** The thread's issues in link order, VISIBLE ones only (an issue on a
 * trashed or archived board drops out like it does from every list). */
export async function listThreadIssueIds(
  db: Db,
  threadId: string
): Promise<string[]> {
  const rows = await db
    .select({ issueId: threadItems.issueId })
    .from(threadItems)
    .innerJoin(issues, eq(issues.id, threadItems.issueId))
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(
      and(
        eq(threadItems.threadId, threadId),
        isNotNull(threadItems.issueId),
        boardVisible()
      )
    )
    .orderBy(asc(threadItems.sortOrder), asc(threadItems.createdAt))
  return rows.map((row) => row.issueId!)
}

/** The run ids of a thread, oldest first. */
export async function listThreadSessionIds(
  db: Db,
  threadId: string
): Promise<string[]> {
  const rows = await db
    .select({ sessionId: threadItems.sessionId })
    .from(threadItems)
    .where(and(eq(threadItems.threadId, threadId), isNotNull(threadItems.sessionId)))
    .orderBy(asc(threadItems.sortOrder), asc(threadItems.createdAt))
  return rows.map((row) => row.sessionId!)
}

/** The issue's "open" thread: the most recently active one holding it. An
 * issue start joins it, so the issue keeps ONE ongoing conversation. */
export async function findIssueThread(
  db: Db,
  issueId: string
): Promise<ThreadRef | null> {
  const [row] = await db
    .select({ id: threads.id, teamId: threads.teamId })
    .from(threadItems)
    .innerJoin(threads, eq(threads.id, threadItems.threadId))
    .where(eq(threadItems.issueId, issueId))
    .orderBy(desc(threads.updatedAt), desc(threads.createdAt))
    .limit(1)
  return row ?? null
}

export async function openThread(
  db: Db,
  args: { teamId: string; createdBy: string | null; title?: string | null }
): Promise<ThreadRef> {
  const [row] = await db
    .insert(threads)
    .values({
      teamId: args.teamId,
      createdBy: args.createdBy,
      title: args.title?.trim() ? args.title.trim().slice(0, 255) : null,
    })
    .returning({ id: threads.id, teamId: threads.teamId })
  return row!
}

/** Bumps the thread's `updated_at` (lists sort threads by activity). */
export async function touchThread(db: Db, threadId: string): Promise<void> {
  await db
    .update(threads)
    .set({ updatedAt: new Date() })
    .where(eq(threads.id, threadId))
}

async function nextSortOrder(db: Db, threadId: string): Promise<number> {
  const [row] = await db
    .select({ max: sql<number | null>`max(${threadItems.sortOrder})` })
    .from(threadItems)
    .where(eq(threadItems.threadId, threadId))
  return Number(row?.max ?? 0) + 1
}

/** Links an issue into a thread. Idempotent: an issue already in the thread
 * answers `added: false`. Refuses an issue of another team or one that is
 * not visible (trashed/archived board, deleted). */
export async function linkIssueToThread(
  db: Db,
  thread: ThreadRef,
  issueId: string,
  actor: ThreadLinkActor
): Promise<{ added: boolean }> {
  const [issue] = await db
    .select({ id: issues.id, teamId: boards.teamId, boardId: issues.boardId })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(and(eq(issues.id, issueId), boardVisible()))
    .limit(1)
  if (!issue || issue.teamId !== thread.teamId) {
    throw new ThreadRuleError(`Issue not found in this thread's team`)
  }
  const inserted = await db
    .insert(threadItems)
    .values({
      threadId: thread.id,
      kind: `issue`,
      issueId,
      // populate_thread_item_team_id/_board_id derive both from the issue.
      teamId: thread.teamId,
      boardId: issue.boardId,
      addedBy: actor.userId,
      addedBySessionId: actor.sessionId ?? null,
      source: actor.source,
      sortOrder: await nextSortOrder(db, thread.id),
    })
    .onConflictDoNothing()
    .returning({ id: threadItems.id })
  if (inserted.length) await touchThread(db, thread.id)
  return { added: inserted.length > 0 }
}

/** Drops an issue from a thread (the issue itself is untouched). */
export async function unlinkIssueFromThread(
  db: Db,
  threadId: string,
  issueId: string
): Promise<{ removed: boolean }> {
  const removed = await db
    .delete(threadItems)
    .where(and(eq(threadItems.threadId, threadId), eq(threadItems.issueId, issueId)))
    .returning({ id: threadItems.id })
  if (removed.length) await touchThread(db, threadId)
  return { removed: removed.length > 0 }
}

/** Puts a run into a thread. A run sits in exactly ONE thread, so a run
 * already elsewhere MOVES (its old thread is deleted when that leaves it
 * empty). The run must belong to the thread's team. */
export async function linkRunToThread(
  db: Db,
  thread: ThreadRef,
  run: { id: string; teamId: string },
  actor: ThreadLinkActor
): Promise<{ added: boolean; movedFrom: string | null }> {
  if (run.teamId !== thread.teamId) {
    throw new ThreadRuleError(`That run belongs to another team`)
  }
  const [existing] = await db
    .select({ id: threadItems.id, threadId: threadItems.threadId })
    .from(threadItems)
    .where(eq(threadItems.sessionId, run.id))
    .limit(1)
  if (existing?.threadId === thread.id) return { added: false, movedFrom: null }
  const sortOrder = await nextSortOrder(db, thread.id)
  if (existing) {
    await db
      .update(threadItems)
      .set({
        threadId: thread.id,
        addedBy: actor.userId,
        addedBySessionId: actor.sessionId ?? null,
        source: actor.source,
        sortOrder,
        updatedAt: new Date(),
      })
      .where(eq(threadItems.id, existing.id))
    await deleteThreadIfEmpty(db, existing.threadId)
  } else {
    await db
      .insert(threadItems)
      .values({
        threadId: thread.id,
        kind: `run`,
        sessionId: run.id,
        teamId: thread.teamId,
        addedBy: actor.userId,
        addedBySessionId: actor.sessionId ?? null,
        source: actor.source,
        sortOrder,
      })
      // A concurrent placement of the same run won; it sits somewhere.
      .onConflictDoNothing()
  }
  await touchThread(db, thread.id)
  return { added: true, movedFrom: existing?.threadId ?? null }
}

/** Detaches a run from a thread. A run is never threadless, so it moves into
 * a fresh thread of its own (holding nothing else). */
export async function unlinkRunFromThread(
  db: Db,
  thread: ThreadRef,
  run: { id: string; teamId: string },
  actor: ThreadLinkActor
): Promise<{ removed: boolean; newThreadId: string | null }> {
  const [existing] = await db
    .select({ id: threadItems.id })
    .from(threadItems)
    .where(and(eq(threadItems.sessionId, run.id), eq(threadItems.threadId, thread.id)))
    .limit(1)
  if (!existing) return { removed: false, newThreadId: null }
  const fresh = await openThread(db, {
    teamId: thread.teamId,
    createdBy: actor.userId,
  })
  await linkRunToThread(db, fresh, run, actor)
  return { removed: true, newThreadId: fresh.id }
}

/** A thread left with no children at all is noise in every list: delete it.
 * One holding only issues stays (the issue page shows it, a run can start
 * into it). */
export async function deleteThreadIfEmpty(db: Db, threadId: string): Promise<void> {
  await db
    .delete(threads)
    .where(
      and(
        eq(threads.id, threadId),
        sql`NOT EXISTS (SELECT 1 FROM ${threadItems} WHERE ${threadItems.threadId} = ${threadId})`
      )
    )
}

export class ThreadRuleError extends Error {}

export type StartSubject =
  | { kind: `issue`; issueId: string }
  | { kind: `batch`; issueIds: string[] }
  | { kind: `action` }
  | { kind: `chat` }

/** EXP-1284: every start opens or joins a thread, AFTER its row exists.
 *  1. A resume stays in its predecessor's thread (same team).
 *  2. A requested `threadId` (composer "start a run here", MCP
 *     `sessions_start{threadId}`) is joined when it is in the run's team; the
 *     start's own issue(s) are linked into it. A vanished or foreign thread
 *     falls through to the default (one stale id never fails a start).
 *  3. An issue start joins the issue's newest thread, else opens one holding
 *     the issue.
 *  4. A batch opens one holding its n issues; action and chat runs open an
 *     empty one.
 * Returns the thread the run sits in. */
export async function joinThreadForStart(
  db: Db,
  args: {
    session: { id: string; teamId: string; userId: string }
    subject: StartSubject
    predecessorId?: string | null
    requestedThreadId?: string | null
    title?: string | null
  }
): Promise<ThreadRef> {
  const { session, subject } = args
  const actor: ThreadLinkActor = { userId: session.userId, source: `user` }
  const run = { id: session.id, teamId: session.teamId }

  if (args.predecessorId) {
    const prior = await getThreadOfSession(db, args.predecessorId)
    if (prior && prior.teamId === session.teamId) {
      await linkRunToThread(db, prior, run, actor)
      return prior
    }
  }

  const subjectIssueIds =
    subject.kind === `issue`
      ? [subject.issueId]
      : subject.kind === `batch`
        ? subject.issueIds
        : []

  if (args.requestedThreadId) {
    const requested = await getThread(db, args.requestedThreadId)
    if (requested && requested.teamId === session.teamId) {
      const ref = { id: requested.id, teamId: requested.teamId }
      await linkRunToThread(db, ref, run, actor)
      await linkIssuesQuietly(db, ref, subjectIssueIds, actor)
      return ref
    }
  }

  if (subject.kind === `issue`) {
    const joined = await findIssueThread(db, subject.issueId)
    if (joined && joined.teamId === session.teamId) {
      await linkRunToThread(db, joined, run, actor)
      return joined
    }
  }

  const fresh = await openThread(db, {
    teamId: session.teamId,
    createdBy: session.userId,
    title: args.title ?? null,
  })
  await linkRunToThread(db, fresh, run, actor)
  await linkIssuesQuietly(db, fresh, subjectIssueIds, actor)
  return fresh
}

/** Links each issue, skipping ones the rules refuse (a start never fails on
 * a stale id). */
async function linkIssuesQuietly(
  db: Db,
  thread: ThreadRef,
  issueIds: string[],
  actor: ThreadLinkActor
): Promise<void> {
  for (const issueId of issueIds) {
    try {
      await linkIssueToThread(db, thread, issueId, actor)
    } catch (e) {
      if (!(e instanceof ThreadRuleError)) throw e
    }
  }
}

/** Every item of several threads, in link order (MCP/tRPC readers). */
export async function listItemsOfThreads(db: Db, threadIds: string[]) {
  if (!threadIds.length) return []
  return db
    .select()
    .from(threadItems)
    .where(inArray(threadItems.threadId, threadIds))
    .orderBy(asc(threadItems.sortOrder), asc(threadItems.createdAt))
}

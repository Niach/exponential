import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { router, authedProcedure, generateTxId, type Context } from "@/lib/trpc"
import { codingSessions, threads } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/auth/access"
import {
  getThread,
  linkIssueToThread,
  linkRunToThread,
  listItemsOfThreads,
  openThread,
  ThreadRuleError,
  unlinkIssueFromThread,
  unlinkRunFromThread,
  type ThreadLinkActor,
  type ThreadRef,
} from "@/lib/threads"

// EXP-1284 — the people's half of threads (the agent's is MCP
// `exponential_threads_*`). Every write goes through lib/threads.ts, which owns
// the data rules (same team, a run sits in ONE thread, an issue once per
// thread); this router only decides WHO may: membership via resolveTeamAccess,
// and a run moves only at its owner's (or its shared-device host's) hand — a
// run's place is part of its content, which stays owner-only (EXP-312). The
// thread shell and its items are member-visible and synced (`threads` +
// `thread-items` shapes); writes return the Electric `txid`.

const MAX_TITLE = 255

/** One target per link/unlink: an issue OR a run. */
const targetInput = z
  .object({
    id: z.string().uuid(),
    issueId: z.string().uuid().optional(),
    sessionId: z.string().uuid().optional(),
  })
  .refine((value) => Boolean(value.issueId) !== Boolean(value.sessionId), {
    message: `Exactly one of issueId/sessionId is required`,
  })

/** The thread row, or NOT_FOUND. Authorization is the caller's next step. */
async function loadThread(db: Context[`db`], id: string): Promise<ThreadRef> {
  const thread = await getThread(db, id)
  if (!thread) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Thread not found` })
  }
  return { id: thread.id, teamId: thread.teamId }
}

/** A run may change threads only at its owner's or its host's hand (the pair
 * every session-scoped write accepts). Same-team is lib/threads' rule. */
async function loadOwnRun(
  db: Context[`db`],
  sessionId: string,
  userId: string
): Promise<{ id: string; teamId: string }> {
  const [run] = await db
    .select({
      id: codingSessions.id,
      teamId: codingSessions.teamId,
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
    })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  if (!run) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Run not found` })
  }
  if (run.userId !== userId && run.hostUserId !== userId) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the run's owner can move it between threads`,
    })
  }
  return { id: run.id, teamId: run.teamId }
}

/** lib/threads' data-rule refusals are the caller's mistake, not ours. */
function asBadRequest(e: unknown): never {
  if (e instanceof ThreadRuleError) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: e.message })
  }
  throw e
}

function normalizeTitle(title: string | null | undefined): string | null {
  const trimmed = title?.trim() ?? ``
  return trimmed ? trimmed.slice(0, MAX_TITLE) : null
}

export const threadsRouter = router({
  // The thread and its VISIBLE items (an issue on a trashed or archived board
  // drops out exactly as it does from the `thread-items` shape), minus the
  // server-only board mirrors.
  get: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const thread = await getThread(ctx.db, input.id)
      if (!thread) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Thread not found` })
      }
      await resolveTeamAccess(ctx.session.user.id, thread.teamId, `read`)
      const rows = await listItemsOfThreads(ctx.db, [thread.id])
      const items = rows
        .filter((row) => !row.boardDeletedAt && !row.boardArchivedAt)
        .map(({ boardDeletedAt: _d, boardArchivedAt: _a, ...item }) => item)
      return { thread, items }
    }),

  // A thread opened by hand (the composer's "new thread", an issue's "start a
  // thread"), optionally holding issues from the start. Runs join it later.
  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        title: z.string().max(MAX_TITLE).nullable().optional(),
        issueIds: z.array(z.string().uuid()).max(30).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await resolveTeamAccess(userId, input.teamId, `create_issue`)
      const actor: ThreadLinkActor = { userId, source: `user` }
      return await ctx.db
        .transaction(async (tx) => {
          const txid = await generateTxId(tx)
          const ref = await openThread(tx, {
            teamId: input.teamId,
            createdBy: userId,
            title: normalizeTitle(input.title),
          })
          for (const issueId of new Set(input.issueIds ?? [])) {
            await linkIssueToThread(tx, ref, issueId, actor)
          }
          const thread = await getThread(tx, ref.id)
          return { thread: thread!, txid }
        })
        .catch(asBadRequest)
    }),

  // Title null or blank clears it (the clients derive one again).
  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        title: z.string().max(MAX_TITLE).nullable(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const ref = await loadThread(ctx.db, input.id)
      await resolveTeamAccess(ctx.session.user.id, ref.teamId, `mutate_resources`)
      return await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const [thread] = await tx
          .update(threads)
          .set({ title: normalizeTitle(input.title), updatedAt: new Date() })
          .where(eq(threads.id, ref.id))
          .returning()
        if (!thread) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Thread not found` })
        }
        return { thread, txid }
      })
    }),

  // An issue: any member who may comment. A run: its owner or host only — it
  // MOVES (a run sits in one thread; an emptied old thread is deleted).
  link: authedProcedure.input(targetInput).mutation(async ({ ctx, input }) => {
    const userId = ctx.session.user.id
    const ref = await loadThread(ctx.db, input.id)
    await resolveTeamAccess(userId, ref.teamId, `comment`)
    const run = input.sessionId
      ? await loadOwnRun(ctx.db, input.sessionId, userId)
      : null
    const actor: ThreadLinkActor = { userId, source: `user` }
    return await ctx.db
      .transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const { added } = run
          ? await linkRunToThread(tx, ref, run, actor)
          : await linkIssueToThread(tx, ref, input.issueId!, actor)
        return { added, txid }
      })
      .catch(asBadRequest)
  }),

  // An issue just leaves the thread. A run is never threadless: it moves into
  // a fresh thread of its own, named back as `newThreadId`.
  unlink: authedProcedure.input(targetInput).mutation(async ({ ctx, input }) => {
    const userId = ctx.session.user.id
    const ref = await loadThread(ctx.db, input.id)
    await resolveTeamAccess(userId, ref.teamId, `comment`)
    const run = input.sessionId
      ? await loadOwnRun(ctx.db, input.sessionId, userId)
      : null
    return await ctx.db
      .transaction(async (tx) => {
        const txid = await generateTxId(tx)
        if (run) {
          const { removed, newThreadId } = await unlinkRunFromThread(tx, ref, run, {
            userId,
            source: `user`,
          })
          return { removed, txid, newThreadId }
        }
        const { removed } = await unlinkIssueFromThread(tx, ref.id, input.issueId!)
        return { removed, txid, newThreadId: null }
      })
      .catch(asBadRequest)
  }),
})

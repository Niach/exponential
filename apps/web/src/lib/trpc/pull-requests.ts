import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { pullRequests } from "@/db/schema"
import { router, authedProcedure } from "@/lib/trpc"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import {
  closePullRequestTx,
  pullRequestFiles,
} from "@/lib/pull-requests"

const uuid = z.string().uuid()

// EXP-1291: the PR model (`pull_requests` + `pull_request_issues`, synced).
// PR state is written by `pr_open`, the merge paths, retarget and the GitHub
// webhook/poller and the merge train (EXP-1709); people act on a PR
// through these. Refusals THROW (the toast).
export const pullRequestsRouter = router({
  /** EXP-1709: a person's Merge (approval mode's Needs approval) queues the
   *  PR, its stack with it; it never merges directly. `stack` is accepted
   *  for old clients (a stack always lands whole). */
  merge: authedProcedure
    .input(z.object({ pullRequestId: uuid, stack: z.boolean().optional() }))
    .mutation(async ({ ctx, input }): Promise<{ merged: boolean; queued?: boolean; error?: string; txid?: number }> => {
      const pr = await prTeam(input.pullRequestId)
      await assertTeamMember(ctx.session.user.id, pr.teamId)
      const { approvePullRequest } = await import(`@/lib/merge-train/worker`)
      await approvePullRequest(input.pullRequestId)
      return { merged: false, queued: true }
    }),

  /** EXP-1709 (merge-train.json `menu`): an owner's Merge now: next, alone,
   *  untested (a stack member lands its whole stack). */
  mergeNow: authedProcedure.input(z.object({ pullRequestId: uuid })).mutation(async ({ ctx, input }) => {
    const pr = await prTeam(input.pullRequestId)
    await assertTeamOwner(ctx.session.user.id, pr.teamId)
    const { mergeNowPullRequest } = await import(`@/lib/merge-train/worker`)
    await mergeNowPullRequest(input.pullRequestId)
    return { ok: true as const }
  }),

  /** EXP-1709: a member's Hold auto-merge (`hold: true`) or Resume
   *  auto-merge / Needs you Retry (`hold: false`). */
  hold: authedProcedure
    .input(z.object({ pullRequestId: uuid, hold: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const pr = await prTeam(input.pullRequestId)
      await assertTeamMember(ctx.session.user.id, pr.teamId)
      const { holdPullRequest, resumePullRequest } = await import(`@/lib/merge-train/worker`)
      await (input.hold ? holdPullRequest : resumePullRequest)(input.pullRequestId)
      return { ok: true as const }
    }),

  close: authedProcedure
    .input(z.object({ pullRequestId: uuid }))
    .mutation(async ({ ctx, input }): Promise<{ txid?: number }> =>
      closePullRequestTx({ userId: ctx.session.user.id, pullRequestId: input.pullRequestId })
    ),

  /** The PR's changed files (the Guide's Changes rows). */
  files: authedProcedure
    .input(z.object({ pullRequestId: uuid }))
    .query(
      async ({
        ctx,
        input,
      }): Promise<{
        files: { path: string; additions: number; deletions: number; patch: string | null }[]
      }> => pullRequestFiles({ userId: ctx.session.user.id, pullRequestId: input.pullRequestId })
    ),
})

async function prTeam(pullRequestId: string): Promise<{ teamId: string }> {
  const [pr] = await db.select({ teamId: pullRequests.teamId }).from(pullRequests).where(eq(pullRequests.id, pullRequestId)).limit(1)
  if (!pr) throw new TRPCError({ code: `NOT_FOUND`, message: `Pull request not found` })
  return pr
}


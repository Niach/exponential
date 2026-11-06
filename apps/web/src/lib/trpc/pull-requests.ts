import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { pullRequests } from "@/db/schema"
import { router, authedProcedure } from "@/lib/trpc"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import { postMergeRefusal, withHandoff } from "@/lib/merge-refusal"
import { isMergeRefusal, MergeRefusal } from "@/lib/trpc/pr-merge-error"
import {
  closePullRequestTx,
  mergePullRequestOrThrow,
  pullRequestFiles,
} from "@/lib/pull-requests"

const uuid = z.string().uuid()

// EXP-1291: the PR model (`pull_requests` + `pull_request_issues`, synced).
// PR state is written by `pr_open`, the merge paths, retarget and the GitHub
// webhook/poller; people act on a PR through these. Refusals THROW (the
// toast); EXP-1391: a merge refusal is first posted into the PR's root
// session (it wakes and takes it over; the message then ends with the
// handoff). `txid` is absent when this call wrote nothing (the webhook
// already landed it, a queued merge, a merge a person did).
export const pullRequestsRouter = router({
  /** Squash-merge; `stack` = merge THROUGH this member of a native stack. */
  merge: authedProcedure
    .input(z.object({ pullRequestId: uuid, stack: z.boolean().optional() }))
    .mutation(
      async ({
        ctx,
        input,
      }): Promise<{ merged: boolean; queued?: boolean; error?: string; txid?: number }> => {
        // EXP-1709: with the merge train on, a person's Merge (approval
        // mode's Needs approval) queues the PR (its stack with it); it never
        // merges directly.
        if (await trainOn()) {
          const pr = await prTeam(input.pullRequestId)
          await assertTeamMember(ctx.session.user.id, pr.teamId)
          const { approvePullRequest } = await import(`@/lib/merge-train/worker`)
          await approvePullRequest(input.pullRequestId)
          return { merged: false, queued: true }
        }
        let result: Awaited<ReturnType<typeof mergePullRequestOrThrow>>
        try {
          result = await mergePullRequestOrThrow({
            userId: ctx.session.user.id,
            pullRequestId: input.pullRequestId,
            ...(input.stack !== undefined ? { stack: input.stack } : {}),
            callerSessionId: null,
          })
        } catch (err) {
          if (!isMergeRefusal(err)) throw err
          const posted = await postMergeRefusal({
            pullRequestId: input.pullRequestId,
            actorUserId: ctx.session.user.id,
            error: err,
          })
          throw new MergeRefusal({ kind: err.kind, message: withHandoff(err.message, posted) })
        }
        return {
          merged: result.merged,
          ...(result.queued ? { queued: true } : {}),
          ...(result.txid != null ? { txid: result.txid } : {}),
        }
      }
    ),

  /** EXP-1709 (merge-train.json `menu`): an owner's Merge now: next, alone,
   *  untested (a stack member lands its whole stack). */
  mergeNow: authedProcedure.input(z.object({ pullRequestId: uuid })).mutation(async ({ ctx, input }) => {
    const pr = await trainPr(input.pullRequestId)
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
      const pr = await trainPr(input.pullRequestId)
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

async function trainOn(): Promise<boolean> {
  return (await import(`@/lib/merge-train/worker`)).mergeTrainOn()
}

async function prTeam(pullRequestId: string): Promise<{ teamId: string }> {
  const [pr] = await db.select({ teamId: pullRequests.teamId }).from(pullRequests).where(eq(pullRequests.id, pullRequestId)).limit(1)
  if (!pr) throw new TRPCError({ code: `NOT_FOUND`, message: `Pull request not found` })
  return pr
}

/** A PR the train's controls act on (the switch on). */
async function trainPr(pullRequestId: string): Promise<{ teamId: string }> {
  if (!(await trainOn())) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Auto-merge is not on for this server` })
  return prTeam(pullRequestId)
}

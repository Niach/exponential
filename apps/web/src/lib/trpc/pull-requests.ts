import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
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
// handoff), and the check gate's PRECONDITION_REQUIRED is the one a person
// overrides with `force` (EXP-1378). `txid` is absent when this call wrote
// nothing (the webhook already landed it, a queued merge, a merge a person did).
export const pullRequestsRouter = router({
  /** Squash-merge; `stack` = merge THROUGH this member of a native stack. */
  merge: authedProcedure
    .input(
      z.object({
        pullRequestId: uuid,
        stack: z.boolean().optional(),
        /** Merge even while checks run or fail (GitHub's rules still apply). */
        force: z.boolean().optional(),
      })
    )
    .mutation(
      async ({
        ctx,
        input,
      }): Promise<{ merged: boolean; queued?: boolean; error?: string; txid?: number }> => {
        let result: Awaited<ReturnType<typeof mergePullRequestOrThrow>>
        try {
          result = await mergePullRequestOrThrow({
            userId: ctx.session.user.id,
            pullRequestId: input.pullRequestId,
            ...(input.stack !== undefined ? { stack: input.stack } : {}),
            ...(input.force ? { force: true } : {}),
            callerSessionId: null,
          })
        } catch (err) {
          if (!isMergeRefusal(err)) throw err
          const posted = await postMergeRefusal({
            pullRequestId: input.pullRequestId,
            actorUserId: ctx.session.user.id,
            error: err,
          })
          throw new MergeRefusal({
            kind: err.kind,
            code: err.code === `PRECONDITION_REQUIRED` ? `PRECONDITION_REQUIRED` : `PRECONDITION_FAILED`,
            message: withHandoff(err.message, posted),
          })
        }
        return {
          merged: result.merged,
          ...(result.queued ? { queued: true } : {}),
          ...(result.txid != null ? { txid: result.txid } : {}),
        }
      }
    ),

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

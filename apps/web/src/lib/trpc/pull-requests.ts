import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import {
  closePullRequestTx,
  mergePullRequestOrThrow,
  pullRequestFiles,
} from "@/lib/pull-requests"

const uuid = z.string().uuid()

// EXP-1291: the PR model (`pull_requests` + `pull_request_issues`, synced).
// PR state is written by `pr_open`, the merge paths, retarget and the GitHub
// webhook/poller; people act on a PR through these. Refusals THROW with the
// code as the signal (EXP-533: CONFLICT = a diagnosed content conflict, the
// "Fix conflicts" gate). `txid` is absent when this call wrote nothing (the
// webhook already landed it, a queued merge, a merge a person did).
export const pullRequestsRouter = router({
  /** Squash-merge; `stack` = merge THROUGH this member of a native stack. */
  merge: authedProcedure
    .input(z.object({ pullRequestId: uuid, stack: z.boolean().optional() }))
    .mutation(
      async ({
        ctx,
        input,
      }): Promise<{ merged: boolean; queued?: boolean; error?: string; txid?: number }> => {
        const result = await mergePullRequestOrThrow({
          userId: ctx.session.user.id,
          pullRequestId: input.pullRequestId,
          ...(input.stack !== undefined ? { stack: input.stack } : {}),
          callerSessionId: null,
        })
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

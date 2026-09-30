// EXP-637 decision 6 (corrected in EXP-639): a run that merges the PR IT
// opened survives its own merge. The durable `merged_own_pr` spare filters
// every merge-driven end (the in-tx sweep, GitHub's webhook, the outbound
// poller), and the run ends later through `exponential_sessions_end` or its
// own exit. `running` is restored with it so the badge reads "coding" again
// instead of staying parked in review.
//
// Two writers share the stamp: the MCP `exponential_pr_merge` closure (the
// caller's session header names the run) and the EXP-1146 yolo tree merge
// (the server lands a run's PR on its behalf once its tree is complete). The
// ORDER is forced by the issue path: `issues.mergePr` runs applyPrMergeState
// in the same call and that in-tx sweep filters on `merged_own_pr = false`,
// so the stamp has to be committed BEFORE the merge or the run is ended by
// its own success. Hence stamp-then-merge, reverted when the merge it was
// stamped for does not land.
import { and, eq, inArray } from "drizzle-orm"
import { codingSessions } from "@/db/schema"
import type { Context } from "@/lib/trpc"

/** What `revertMergedOwnPr` restores: the two live statuses the stamp ever
 *  applies to (an `ended` row is never stamped) and the picker flag. */
export interface MergedOwnPrPrior {
  status: `running` | `in_review`
  needsInput: boolean
}

export function priorOf(row: {
  status: string
  needsInput: boolean
}): MergedOwnPrPrior {
  return {
    status: row.status === `in_review` ? `in_review` : `running`,
    needsInput: row.needsInput,
  }
}

export async function stampMergedOwnPr(
  db: Context[`db`],
  sessionId: string
): Promise<void> {
  await db
    .update(codingSessions)
    .set({
      mergedOwnPr: true,
      status: `running`,
      needsInput: false,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(codingSessions.id, sessionId),
        inArray(codingSessions.status, [`running`, `in_review`])
      )
    )
}

/** Put the row back exactly as it was, so the run is still ended by the merge
 *  it did NOT perform. Guarded on the stamp itself and on the status this
 *  call wrote — a concurrent kill or close-out is never resurrected. */
export async function revertMergedOwnPr(
  db: Context[`db`],
  sessionId: string,
  prior: MergedOwnPrPrior
): Promise<void> {
  await db
    .update(codingSessions)
    .set({
      mergedOwnPr: false,
      status: prior.status,
      needsInput: prior.needsInput,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(codingSessions.id, sessionId),
        eq(codingSessions.mergedOwnPr, true),
        eq(codingSessions.status, `running`)
      )
    )
}

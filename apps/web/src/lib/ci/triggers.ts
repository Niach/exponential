// #EXP-1617: forge events → our CI runs. A push plans `push` (a branch),
// `tag` (a tag) or `merge_train` (a train ref); a pull request opened,
// pushed or reopened plans `pull_request` on its base. A PR from a fork
// plans to the spec's `fork` skip: its checks say so and nothing runs.
// Every repository row of the full name (several teams may connect one)
// gets its own run. Fire and forget: a webhook answers before the forge
// read of `.exponential/ci.json`.

import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { pullRequests, repositories } from "@/db/schema"
import { isTrainBranch } from "@/lib/merge-train/commit-checks"
import { startRun } from "@/lib/ci/runs"

const pending = new Set<Promise<unknown>>()

function fire(work: Promise<unknown>): void {
  const tracked = work.catch((err) => console.error(`[ci] trigger failed`, err)).finally(() => pending.delete(tracked))
  pending.add(tracked)
}

/** Tests: resolves once every fired trigger finished. */
export async function drainCiTriggers(): Promise<void> {
  while (pending.size > 0) await Promise.all([...pending])
}

async function repoIds(fullName: string): Promise<string[]> {
  const rows = await db.select({ id: repositories.id }).from(repositories).where(eq(repositories.fullName, fullName))
  return rows.map((row) => row.id)
}

/** How long a trigger waits for the PR row its own delivery adopts. */
export const PR_ROW_WAIT_MS = 3_000

const ZERO_SHA = /^0+$/

export function onForgePush(args: { fullName: string; ref: string; sha: string | null; deleted: boolean }): void {
  if (args.deleted || !args.sha || ZERO_SHA.test(args.sha)) return
  const branch = args.ref.startsWith(`refs/heads/`) ? args.ref.slice(`refs/heads/`.length) : null
  const tag = args.ref.startsWith(`refs/tags/`) ? args.ref.slice(`refs/tags/`.length) : null
  if (!branch && !tag) return
  fire(
    (async () => {
      for (const repositoryId of await repoIds(args.fullName)) {
        await startRun({
          repositoryId,
          sha: args.sha!,
          trigger: tag ? `tag` : isTrainBranch(branch) ? `merge_train` : `push`,
          ref: tag ?? branch,
        })
      }
    })()
  )
}

export function onForgePullRequest(args: {
  fullName: string
  number: number
  headSha: string
  headRef: string | null
  base: string
  fork: boolean
}): void {
  fire(
    (async () => {
      for (const repositoryId of await repoIds(args.fullName)) {
        const find = () =>
          db
            .select({ id: pullRequests.id })
            .from(pullRequests)
            .where(and(eq(pullRequests.repositoryId, repositoryId), eq(pullRequests.number, args.number)))
            .limit(1)
        // An out-of-band PR's row is adopted by the same delivery: give it a moment.
        let [pr] = await find()
        if (!pr) {
          await new Promise((resolve) => setTimeout(resolve, PR_ROW_WAIT_MS))
          ;[pr] = await find()
        }
        await startRun({
          repositoryId,
          sha: args.headSha,
          trigger: `pull_request`,
          ref: args.headRef,
          base: args.base,
          pullRequestId: pr?.id ?? null,
          fork: args.fork,
        })
      }
    })()
  )
}

// EXP-1709: `commit_checks`, CI per COMMIT, provider-neutral. The forge's
// webhooks write it (GitHub: check_run and status deliveries on a train ref
// `exp-train/<train_id>`), eCode's own CI later; the merge train reads the
// composed commit's checks here first and falls back to the forge
// (`readCommitChecks`) when no delivery arrived.
import { and, eq, inArray, lt } from "drizzle-orm"
import type { TrainCheck } from "@exp/domain-contract/merge-train"
import { db } from "@/db/connection"
import { commitChecks, repositories } from "@/db/schema"
import type { Forge, ForgeRepo } from "@/lib/ecode/forge"

export interface CommitCheckInput {
  name: string
  status: `queued` | `in_progress` | `completed`
  conclusion: string | null
  detailsUrl?: string | null
  startedAt?: Date | null
  completedAt?: Date | null
}

/** The branch prefix of every train ref (merge-train.json `pipeline.ref`). */
export const TRAIN_REF_PREFIX = `exp-train/`

/** Whether a delivery's branch is a train ref (only those are kept). */
export function isTrainBranch(branch: string | null | undefined): boolean {
  return !!branch && branch.startsWith(TRAIN_REF_PREFIX)
}

/** Upsert one check at `sha` for every repository row of `repoFullName`
 *  (several teams may connect the same repository). A completed row is
 *  never downgraded by a late queued/in_progress delivery. */
export async function writeCommitCheck(args: {
  repoFullName: string
  sha: string
  check: CommitCheckInput
}): Promise<number> {
  const repos = await db
    .select({ id: repositories.id })
    .from(repositories)
    .where(eq(repositories.fullName, args.repoFullName))
  if (repos.length === 0) return 0
  const now = new Date()
  const values = {
    sha: args.sha,
    name: args.check.name.slice(0, 255),
    status: args.check.status,
    conclusion: args.check.conclusion as (typeof commitChecks.$inferInsert)[`conclusion`],
    detailsUrl: args.check.detailsUrl ?? null,
    startedAt: args.check.startedAt ?? null,
    completedAt: args.check.completedAt ?? null,
  }
  for (const repo of repos) {
    const [existing] = await db
      .select({ id: commitChecks.id, status: commitChecks.status })
      .from(commitChecks)
      .where(
        and(eq(commitChecks.repositoryId, repo.id), eq(commitChecks.sha, args.sha), eq(commitChecks.name, values.name))
      )
      .limit(1)
    if (!existing) {
      await db.insert(commitChecks).values({ repositoryId: repo.id, ...values }).onConflictDoNothing()
      continue
    }
    if (existing.status === `completed` && values.status !== `completed`) continue
    await db.update(commitChecks).set({ ...values, updatedAt: now }).where(eq(commitChecks.id, existing.id))
  }
  return repos.length
}

/** The checks at `sha`: the webhook-written rows, else (none yet) the
 *  forge's own read, written back so the next read is local. Null =
 *  unreadable. */
export async function readCommitChecks(forge: Forge, repo: ForgeRepo, sha: string): Promise<TrainCheck[] | null> {
  const rows = await db
    .select({ name: commitChecks.name, status: commitChecks.status, conclusion: commitChecks.conclusion })
    .from(commitChecks)
    .where(and(eq(commitChecks.repositoryId, repo.id), eq(commitChecks.sha, sha)))
  const local: TrainCheck[] = rows.map((r) => ({ name: r.name, status: r.status, conclusion: r.conclusion }))
  const pending = local.length === 0 || local.some((c) => c.status !== `completed`)
  if (!pending) return local
  // A lost delivery never strands a run: re-read while anything is open.
  const remote = await forge.checksAt(repo, sha)
  if (!remote) return local.length > 0 ? local : null
  for (const check of remote) {
    await writeCommitCheck({ repoFullName: repo.fullName, sha, check })
  }
  return remote
}

/** The watchdog's prune: a train commit's checks matter for days, not weeks. */
export const COMMIT_CHECKS_KEEP_MS = 14 * 24 * 60 * 60_000

export async function pruneCommitChecks(now: Date = new Date()): Promise<void> {
  await db.delete(commitChecks).where(lt(commitChecks.updatedAt, new Date(now.getTime() - COMMIT_CHECKS_KEEP_MS)))
}

/** Tests / cleanup: drop a repository's rows at the given shas. */
export async function deleteCommitChecks(repositoryId: string, shas: string[]): Promise<void> {
  if (shas.length === 0) return
  await db.delete(commitChecks).where(and(eq(commitChecks.repositoryId, repositoryId), inArray(commitChecks.sha, shas)))
}

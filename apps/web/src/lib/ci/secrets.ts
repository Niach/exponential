// #EXP-1617: a repository's CI secrets, sealed with the eMCP vault. People
// write them (the repo's manager) and see only names; a runner reads the
// ones its job names ONCE, at job start, with the job's one-time token.
// #EXP-2180: only a trusted run gets them (`secretsReachRun`): tags, the
// merge train and push / manual runs of the default branch, unless the
// repository's owner allowed pull request runs (`ciConfig.pullRequestSecrets`).
// Every set and delete is an audit event in the team's stream.

import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { ciJobs, ciRuns, ciSecrets, repositories, type RepositoryCiConfig } from "@/db/schema"
import { emitEventInTx } from "@/lib/events"
import { ciSecretSlot, openSecret, sealSecret } from "@/lib/emcp/vault"
import { hashJobToken } from "@/lib/ci/runs"

/** Env var names (the pipeline's secret rule). */
export const CI_SECRET_NAME = /^[A-Z_][A-Z0-9_]*$/
export const CI_SECRET_VALUE_MAX = 32 * 1024

export async function listSecretNames(repositoryId: string) {
  return db
    .select({ name: ciSecrets.name, updatedAt: ciSecrets.updatedAt, updatedByUserId: ciSecrets.updatedByUserId })
    .from(ciSecrets)
    .where(eq(ciSecrets.repositoryId, repositoryId))
    .orderBy(asc(ciSecrets.name))
}

/** #EXP-2180: the audit events (team stream, names only, never a value). */
export const CI_SECRET_SET_EVENT = `ci.secret_set`
export const CI_SECRET_DELETED_EVENT = `ci.secret_deleted`
export const CI_PULL_REQUEST_SECRETS_EVENT = `ci.pull_request_secrets`

export async function setSecret(args: { teamId: string; repositoryId: string; name: string; value: string; userId: string }) {
  const id = crypto.randomUUID()
  await db.transaction(async (tx) => {
    await emitEventInTx(tx, {
      teamId: args.teamId,
      name: CI_SECRET_SET_EVENT,
      payload: { repositoryId: args.repositoryId, name: args.name },
      emittedBy: { userId: args.userId },
    })
    const [existing] = await tx
      .select({ id: ciSecrets.id })
      .from(ciSecrets)
      .where(and(eq(ciSecrets.repositoryId, args.repositoryId), eq(ciSecrets.name, args.name)))
      .limit(1)
    if (existing) {
      await tx
        .update(ciSecrets)
        .set({ valueCiphertext: sealSecret(args.value, ciSecretSlot(existing.id)), updatedByUserId: args.userId })
        .where(eq(ciSecrets.id, existing.id))
      return
    }
    await tx.insert(ciSecrets).values({
      id,
      teamId: args.teamId,
      repositoryId: args.repositoryId,
      name: args.name,
      valueCiphertext: sealSecret(args.value, ciSecretSlot(id)),
      updatedByUserId: args.userId,
    })
  })
}

export async function deleteSecret(args: { teamId: string; repositoryId: string; name: string; userId: string }): Promise<boolean> {
  return db.transaction(async (tx) => {
    const gone = await tx
      .delete(ciSecrets)
      .where(and(eq(ciSecrets.repositoryId, args.repositoryId), eq(ciSecrets.name, args.name)))
      .returning({ id: ciSecrets.id })
    if (gone.length === 0) return false
    await emitEventInTx(tx, {
      teamId: args.teamId,
      name: CI_SECRET_DELETED_EVENT,
      payload: { repositoryId: args.repositoryId, name: args.name },
      emittedBy: { userId: args.userId },
    })
    return true
  })
}

/** #EXP-2180: does a run of this repository get its secrets? Tags, the
 *  merge train and push / manual runs of the default branch do; any other
 *  run (a pull request, another branch) only when the repository allows
 *  pull request runs. */
export function secretsReachRun(
  run: { trigger: string; ref: string | null },
  repo: { defaultBranch: string; defaultBranchOverride?: string | null; ciConfig: RepositoryCiConfig | null }
): boolean {
  if (repo.ciConfig?.pullRequestSecrets === true) return true
  if (run.trigger === `tag` || run.trigger === `merge_train`) return true
  const defaultBranch = repo.defaultBranchOverride ?? repo.defaultBranch
  return (run.trigger === `push` || run.trigger === `manual`) && run.ref === defaultBranch
}

/** The one log line a runner writes when a job's secrets were withheld. */
export function withheldNotice(names: readonly string[], trigger: string): string {
  const what = trigger === `pull_request` ? `this pull request run` : `this run`
  return `Secrets withheld from ${what}: ${names.join(`, `)}. Only default-branch, tag and merge-train runs get them unless the repository allows pull requests.`
}

export interface JobSecrets {
  values: Record<string, string>
  /** Names the job asked for that this run may not have (#EXP-2180). */
  withheld: string[]
  /** One log line for the runner to write (null = nothing withheld). */
  notice: string | null
}

/** The running job's secrets, ONCE: the first call with the job's token
 *  marks them fetched; any later call (a replay, a leaked token) gets
 *  nothing. Only the names the job's plan lists, only this repo's. */
export async function takeJobSecrets(args: { jobId: string; token: string; repositoryId: string }): Promise<JobSecrets | null> {
  const [job] = await db
    .update(ciJobs)
    .set({ secretsFetchedAt: new Date() })
    .where(
      and(
        eq(ciJobs.id, args.jobId),
        eq(ciJobs.jobTokenHash, hashJobToken(args.token)),
        eq(ciJobs.status, `in_progress`),
        isNull(ciJobs.secretsFetchedAt)
      )
    )
    .returning({ spec: ciJobs.spec, runId: ciJobs.runId })
  if (!job) return null
  const names = job.spec.secrets
  if (names.length === 0) return { values: {}, withheld: [], notice: null }
  const [scope] = await db
    .select({
      trigger: ciRuns.trigger,
      ref: ciRuns.ref,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
      ciConfig: repositories.ciConfig,
    })
    .from(ciRuns)
    .innerJoin(repositories, eq(repositories.id, ciRuns.repositoryId))
    .where(and(eq(ciRuns.id, job.runId), eq(ciRuns.repositoryId, args.repositoryId)))
    .limit(1)
  if (!scope || !secretsReachRun(scope, scope)) {
    return { values: {}, withheld: [...names], notice: withheldNotice(names, scope?.trigger ?? ``) }
  }
  const rows = await db
    .select({ id: ciSecrets.id, name: ciSecrets.name, value: ciSecrets.valueCiphertext })
    .from(ciSecrets)
    .where(and(eq(ciSecrets.repositoryId, args.repositoryId), inArray(ciSecrets.name, names)))
  const values: Record<string, string> = {}
  for (const row of rows) {
    const plain = openSecret(row.value, ciSecretSlot(row.id))
    if (plain !== null) values[row.name] = plain
  }
  return { values, withheld: [], notice: null }
}

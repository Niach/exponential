// #EXP-1617: a repository's CI secrets, sealed with the eMCP vault. People
// write them (the repo's manager) and see only names; a runner reads the
// ones its job names ONCE, at job start, with the job's one-time token.

import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { ciJobs, ciSecrets } from "@/db/schema"
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

export async function setSecret(args: { teamId: string; repositoryId: string; name: string; value: string; userId: string }) {
  const id = crypto.randomUUID()
  await db.transaction(async (tx) => {
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

export async function deleteSecret(repositoryId: string, name: string): Promise<boolean> {
  const gone = await db
    .delete(ciSecrets)
    .where(and(eq(ciSecrets.repositoryId, repositoryId), eq(ciSecrets.name, name)))
    .returning({ id: ciSecrets.id })
  return gone.length > 0
}

/** The running job's secrets, ONCE: the first call with the job's token
 *  marks them fetched; any later call (a replay, a leaked token) gets
 *  nothing. Only the names the job's plan lists, only this repo's. */
export async function takeJobSecrets(args: { jobId: string; token: string; repositoryId: string }): Promise<Record<string, string> | null> {
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
    .returning({ spec: ciJobs.spec })
  if (!job) return null
  const names = job.spec.secrets
  if (names.length === 0) return {}
  const rows = await db
    .select({ id: ciSecrets.id, name: ciSecrets.name, value: ciSecrets.valueCiphertext })
    .from(ciSecrets)
    .where(and(eq(ciSecrets.repositoryId, args.repositoryId), inArray(ciSecrets.name, names)))
  const values: Record<string, string> = {}
  for (const row of rows) {
    const plain = openSecret(row.value, ciSecretSlot(row.id))
    if (plain !== null) values[row.name] = plain
  }
  return values
}

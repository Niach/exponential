// #EXP-1616: eCode's own git on the server, without git. The team bucket IS
// the remote: a repo is its `repo_refs` rows (ref → sha, moved ONLY by
// compare-and-swap here: the one serialization point) plus its packs in the
// bucket (`repo_packs`, every pack a device ever pushed, until a repack or a
// restore supersedes them). A fetch = every live pack in `seq` order; a push
// = one new pack + a CAS over its ref updates, in one transaction.
import { and, asc, desc, eq, inArray, isNull, lt, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { repoPacks, repoRefs, repositories, teams } from "@/db/schema"

type Db = typeof db
type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]

export const HEADS = `refs/heads/`

/** A ref name the store accepts (git's check-ref-format, the safe subset). */
export function validRef(ref: string): boolean {
  if (!ref.startsWith(`refs/`) || ref.length > 255) return false
  if (/[\s~^:?*[\\\x00-\x1f\x7f]/.test(ref)) return false
  if (ref.includes(`..`) || ref.includes(`//`) || ref.includes(`@{`) || ref.endsWith(`/`) || ref.endsWith(`.`) || ref.endsWith(`.lock`)) return false
  return ref.split(`/`).every((part) => part.length > 0 && !part.startsWith(`.`))
}

export const validSha = (sha: string) => /^[0-9a-f]{40}([0-9a-f]{24})?$/.test(sha)

/** The bucket prefix of one repo's objects. */
export function repoObjectPrefix(teamId: string, repositoryId: string): string {
  return `ecode/${teamId}/${repositoryId}/`
}

/** Every ref of a repo (full names). */
export async function listRefs(repositoryId: string, database: Pick<Db, `select`> = db): Promise<Record<string, string>> {
  const rows = await database
    .select({ ref: repoRefs.ref, sha: repoRefs.sha })
    .from(repoRefs)
    .where(eq(repoRefs.repositoryId, repositoryId))
    .orderBy(asc(repoRefs.ref))
  return Object.fromEntries(rows.map((row) => [row.ref, row.sha]))
}

/** One ref update: `old` null = must not exist, `new` null = delete. */
export interface RefUpdate {
  ref: string
  old: string | null
  new: string | null
}

class CasFailed extends Error {
  constructor(readonly ref: string) {
    super(`ref moved: ${ref}`)
  }
}

/** The CAS itself, inside the caller's transaction: every update applies or
 *  this throws (the caller's transaction rolls back). */
async function casInTx(tx: Tx, repositoryId: string, updates: RefUpdate[]): Promise<void> {
  for (const update of updates) {
    let moved: unknown[]
    if (update.old === null) {
      if (update.new === null) continue
      moved = await tx
        .insert(repoRefs)
        .values({ repositoryId, ref: update.ref, sha: update.new })
        .onConflictDoNothing()
        .returning({ ref: repoRefs.ref })
    } else if (update.new === null) {
      moved = await tx
        .delete(repoRefs)
        .where(and(eq(repoRefs.repositoryId, repositoryId), eq(repoRefs.ref, update.ref), eq(repoRefs.sha, update.old)))
        .returning({ ref: repoRefs.ref })
    } else {
      moved = await tx
        .update(repoRefs)
        .set({ sha: update.new, updatedAt: new Date() })
        .where(and(eq(repoRefs.repositoryId, repositoryId), eq(repoRefs.ref, update.ref), eq(repoRefs.sha, update.old)))
        .returning({ ref: repoRefs.ref })
    }
    if (moved.length === 0) throw new CasFailed(update.ref)
  }
}

/** All or nothing: true = every ref moved; false = one had moved meanwhile. */
export async function compareAndSwapRefs(repositoryId: string, updates: RefUpdate[]): Promise<boolean> {
  try {
    await db.transaction((tx) => casInTx(tx, repositoryId, updates))
    return true
  } catch (err) {
    if (err instanceof CasFailed) return false
    throw err
  }
}

export interface PackRow {
  id: string
  key: string
  bytes: number
  seq: number
}

/** The live packs of a repo, oldest first (`after` = only newer than that seq). */
export async function livePacks(repositoryId: string, after = 0): Promise<PackRow[]> {
  return db
    .select({ id: repoPacks.id, key: repoPacks.key, bytes: repoPacks.bytes, seq: repoPacks.seq })
    .from(repoPacks)
    .where(
      and(
        eq(repoPacks.repositoryId, repositoryId),
        eq(repoPacks.kind, `pack`),
        isNull(repoPacks.supersededAt),
        sql`${repoPacks.seq} > ${after}`
      )
    )
    .orderBy(asc(repoPacks.seq))
}

/** A push: its pack (if it brought objects) and its ref updates, atomically.
 *  `rejected` = the ref that had moved (the client fetches and retries). */
export async function recordPush(args: {
  teamId: string
  repositoryId: string
  pack: { key: string; bytes: number } | null
  updates: RefUpdate[]
}): Promise<{ ok: true } | { ok: false; rejected: string }> {
  try {
    await db.transaction(async (tx) => {
      if (args.pack) {
        await tx
          .insert(repoPacks)
          .values({ teamId: args.teamId, repositoryId: args.repositoryId, kind: `pack`, key: args.pack.key, bytes: args.pack.bytes })
          .onConflictDoNothing()
      }
      await casInTx(tx, args.repositoryId, args.updates)
    })
    return { ok: true }
  } catch (err) {
    if (err instanceof CasFailed) return { ok: false, rejected: err.ref }
    throw err
  }
}

/** A repack: one pack replaces `replaces` (the refs do not move). */
export async function swapPacks(args: {
  teamId: string
  repositoryId: string
  pack: { key: string; bytes: number }
  replaces: string[]
}): Promise<void> {
  await db.transaction(async (tx) => {
    await tx
      .insert(repoPacks)
      .values({ teamId: args.teamId, repositoryId: args.repositoryId, kind: `pack`, key: args.pack.key, bytes: args.pack.bytes })
      .onConflictDoNothing()
    if (args.replaces.length > 0) {
      await tx
        .update(repoPacks)
        .set({ supersededAt: new Date() })
        .where(and(eq(repoPacks.repositoryId, args.repositoryId), inArray(repoPacks.id, args.replaces), isNull(repoPacks.supersededAt)))
    }
  })
}

/** A restore: ONE pack (the bundle's objects) and the bundle's refs replace
 *  every pack and every ref of the repo. */
export async function replaceRepository(args: {
  teamId: string
  repositoryId: string
  pack: { key: string; bytes: number }
  refs: Record<string, string>
}): Promise<void> {
  await db.transaction(async (tx) => {
    await tx
      .update(repoPacks)
      .set({ supersededAt: new Date() })
      .where(and(eq(repoPacks.repositoryId, args.repositoryId), eq(repoPacks.kind, `pack`), isNull(repoPacks.supersededAt)))
    await tx
      .insert(repoPacks)
      .values({ teamId: args.teamId, repositoryId: args.repositoryId, kind: `pack`, key: args.pack.key, bytes: args.pack.bytes })
    await tx.delete(repoRefs).where(eq(repoRefs.repositoryId, args.repositoryId))
    const rows = Object.entries(args.refs).map(([ref, sha]) => ({ repositoryId: args.repositoryId, ref, sha }))
    if (rows.length > 0) await tx.insert(repoRefs).values(rows)
  })
}

/** How many bundle snapshots a repo keeps (older ones go). */
export const BUNDLES_KEPT = 5

export interface BundleRow {
  id: string
  key: string
  bytes: number
  refs: Record<string, string>
  createdAt: Date
}

/** The repo's backups, newest first. */
export async function listBundles(repositoryId: string): Promise<BundleRow[]> {
  const rows = await db
    .select({ id: repoPacks.id, key: repoPacks.key, bytes: repoPacks.bytes, refs: repoPacks.refs, createdAt: repoPacks.createdAt })
    .from(repoPacks)
    .where(and(eq(repoPacks.repositoryId, repositoryId), eq(repoPacks.kind, `bundle`), isNull(repoPacks.supersededAt)))
    .orderBy(desc(repoPacks.seq))
  return rows.map((row) => ({ ...row, refs: row.refs ?? {} }))
}

/** A backup landed: record it; the ones past BUNDLES_KEPT are superseded. */
export async function recordBundle(args: {
  teamId: string
  repositoryId: string
  key: string
  bytes: number
  refs: Record<string, string>
}): Promise<void> {
  await db.transaction(async (tx) => {
    await tx
      .insert(repoPacks)
      .values({ teamId: args.teamId, repositoryId: args.repositoryId, kind: `bundle`, key: args.key, bytes: args.bytes, refs: args.refs })
      .onConflictDoNothing()
    const kept = await tx
      .select({ id: repoPacks.id })
      .from(repoPacks)
      .where(and(eq(repoPacks.repositoryId, args.repositoryId), eq(repoPacks.kind, `bundle`), isNull(repoPacks.supersededAt)))
      .orderBy(desc(repoPacks.seq))
    const old = kept.slice(BUNDLES_KEPT).map((row) => row.id)
    if (old.length > 0) await tx.update(repoPacks).set({ supersededAt: new Date() }).where(inArray(repoPacks.id, old))
  })
}

/** Superseded objects older than this are deleted (a device mid-fetch may
 *  still hold a presigned URL to one). */
export const SUPERSEDED_GRACE_MS = 60 * 60_000

/** Deletes the superseded objects past the grace: rows and bucket objects. */
export async function pruneSuperseded(repositoryId: string, now = new Date()): Promise<number> {
  const rows = await db
    .select({ id: repoPacks.id, key: repoPacks.key })
    .from(repoPacks)
    .where(and(eq(repoPacks.repositoryId, repositoryId), lt(repoPacks.supersededAt, new Date(now.getTime() - SUPERSEDED_GRACE_MS))))
  if (rows.length === 0) return 0
  const { deleteObject } = await import(`@/lib/storage`)
  for (const row of rows) await deleteObject(row.key).catch(() => {})
  await db.delete(repoPacks).where(inArray(repoPacks.id, rows.map((row) => row.id)))
  return rows.length
}

/** Every bucket object of the given repos (a repo or team delete strands
 *  them otherwise: the cascade never touches the bucket). */
export async function collectRepoStorageKeys(
  database: Pick<Db, `select`>,
  filter: { repositoryIds?: string[]; teamIds?: string[] }
): Promise<string[]> {
  const where = filter.repositoryIds
    ? filter.repositoryIds.length > 0
      ? inArray(repoPacks.repositoryId, filter.repositoryIds)
      : null
    : filter.teamIds && filter.teamIds.length > 0
      ? inArray(repoPacks.teamId, filter.teamIds)
      : null
  if (!where) return []
  const rows = await database.select({ key: repoPacks.key }).from(repoPacks).where(where)
  return rows.map((row) => row.key)
}

/** The bytes a team's repos hold in the bucket (live packs and kept bundles). */
export async function teamRepoBytes(teamId: string, database: Pick<Db, `select`> = db): Promise<number> {
  const [row] = await database
    .select({ bytes: sql<string>`coalesce(sum(${repoPacks.bytes}), 0)::bigint` })
    .from(repoPacks)
    .where(and(eq(repoPacks.teamId, teamId), isNull(repoPacks.supersededAt)))
  return Number(row?.bytes ?? 0)
}

/** The `ecode` repo a `exp://<team>/<name>` remote names, null = none. The
 *  team by its CURRENT slug; the name = the full name's last segment (a
 *  renamed team keeps its repos reachable). */
export async function repositoryByRemote(teamSlug: string, name: string) {
  const [row] = await db
    .select({ id: repositories.id, teamId: repositories.teamId })
    .from(repositories)
    .innerJoin(teams, eq(teams.id, repositories.teamId))
    .where(
      and(
        eq(teams.slug, teamSlug),
        eq(repositories.provider, `ecode`),
        sql`split_part(${repositories.fullName}, '/', 2) = ${name}`,
        isNull(repositories.archivedAt)
      )
    )
    .limit(1)
  return row ?? null
}

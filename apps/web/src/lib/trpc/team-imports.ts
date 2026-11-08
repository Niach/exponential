// #EXP-1713: the TEAM IMPORT procedures of the `imports` router (Share a
// Local desktop as a team, #EDE-57; the offline IDE later), tRPC-only, never
// on MCP. `create` makes the team in state importing (no member row yet: no
// shape, list or access check sees it), `chunk` stages rows idempotently per
// (importId, seq) and mints the signed upload per attachments row, `commit`
// applies everything in one transaction (lib/team-import/apply.ts) and flips
// the team live. Rules: fixtures/team-import.json.
import { randomUUID } from "crypto"
import { and, asc, eq, inArray, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { authedProcedure } from "@/lib/trpc"
import { attachments, importIdMap, importRows, teamImports, teamMembers, teams } from "@/db/schema"
import { teamColumns } from "@/lib/team-columns"
import { uniqueSlug } from "@/lib/trpc/teams"
import { assertCanCreateAppRecord, assertCanCreateTeam, assertWithinStorageLimit, planLimitError } from "@/lib/billing"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { recordConversionEvent } from "@/lib/conversion/events"
import { seedBuiltinSkills } from "@/lib/skills/builtin"
import { setPreserveTimestamps } from "@/lib/import/apply-db"
import { mintAttachmentUploadToken } from "@/lib/storage/attachment-upload-token"
import { canonicalizeContentType, sanitizeUploadFilename } from "@/lib/storage/issue-attachments"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"
import {
  TEAM_IMPORT_ATTACHMENTS_CAP_BYTES,
  TEAM_IMPORT_CHUNK,
  TEAM_IMPORT_MANIFEST_VERSION,
  validateChunk,
  type ImportRow,
  type TeamImportManifest,
} from "@exp/domain-contract/team-import"
import { applyImport, IdMap, type StagedRow } from "@/lib/team-import/apply"

// Per-user bound on new imports: each one reserves a team row and a slug.
const createLimiter = new TokenBucketLimiter({ capacity: 5, refillPerHour: 20 })

export const teamImportManifestSchema = z.object({
  version: z.literal(TEAM_IMPORT_MANIFEST_VERSION),
  desktopId: z.string().min(1).max(128),
  name: z.string().min(1).max(255),
  identity: z.object({ userId: z.string().min(1).max(128), email: z.string().email().max(255) }),
  deviceId: z.string().min(1).max(128).optional(),
  counts: z.record(z.string(), z.number().int().nonnegative()),
  attachmentBytes: z.number().int().nonnegative(),
})

const rowSchema = z.object({
  table: z.string().min(1).max(32),
  id: z.string().min(1).max(128),
  row: z.record(z.string(), z.unknown()),
})

const importSelection = {
  id: teamImports.id,
  teamId: teamImports.teamId,
  createdByUserId: teamImports.createdByUserId,
  status: teamImports.status,
  manifest: teamImports.manifest,
  counts: teamImports.counts,
} as const

/** The caller's live import, or NOT_FOUND (someone else's reads the same). */
async function loadImport(dbx: { select: typeof import("@/db/connection").db.select }, userId: string, importId: string) {
  const [row] = await dbx.select(importSelection).from(teamImports).where(eq(teamImports.id, importId)).limit(1)
  if (!row || row.createdByUserId !== userId) throw new TRPCError({ code: `NOT_FOUND`, message: `Import not found` })
  if (row.status !== `importing`) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This import is already ${row.status}` })
  const manifest = teamImportManifestSchema.safeParse(row.manifest)
  if (!manifest.success) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This import's manifest is not readable` })
  return { ...row, manifest: manifest.data as TeamImportManifest, counts: (row.counts ?? {}) as Record<string, number> }
}

/** The signed upload for an attachments row: its server id (the chunk-time
 *  remap when the local id was taken) and the PUT path. */
function uploadFor(args: { importId: string; teamId: string; userId: string; row: ImportRow; serverId: string }) {
  const filename = sanitizeUploadFilename(String(args.row.row.filename ?? ``))
  const contentType = canonicalizeContentType(String(args.row.row.content_type ?? ``))
  const { token, expiresAt } = mintAttachmentUploadToken({ attachmentId: args.serverId, teamId: args.teamId, userId: args.userId, filename, contentType })
  return { id: args.row.id, serverId: args.serverId, uploadPath: `/api/attachment-uploads/${token}`, expiresAt }
}

export const teamImportProcedures = {
  // Reserves the team (invisible until the commit) and the slug.
  create: authedProcedure
    .input(
      z.object({
        name: z.string().min(1).max(255),
        slug: z
          .string()
          .regex(/^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$/)
          .optional(),
        manifest: teamImportManifestSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const limit = createLimiter.tryTake(userId)
      if (!limit.ok) {
        throw new TRPCError({ code: `TOO_MANY_REQUESTS`, message: `Too many imports started. Retry in ${limit.retryAfterSeconds}s.` })
      }
      await assertCanCreateTeam(userId)
      if (isCloudInstance() && input.manifest.attachmentBytes > TEAM_IMPORT_ATTACHMENTS_CAP_BYTES) {
        throw planLimitError(`up to ${Math.round(TEAM_IMPORT_ATTACHMENTS_CAP_BYTES / (1024 * 1024 * 1024))} GB of attachments in one import.`)
      }
      return await ctx.db.transaction(async (tx) => {
        const slug = await uniqueSlug(tx, input.slug ?? input.name)
        const [team] = await tx
          .insert(teams)
          .values({
            name: input.name,
            slug,
          })
          .returning({ id: teams.id, slug: teams.slug })
        const [row] = await tx
          .insert(teamImports)
          .values({ teamId: team.id, createdByUserId: userId, manifest: input.manifest })
          .returning({ id: teamImports.id })
        return { importId: row.id, teamId: team.id, slug: team.slug }
      })
    }),

  // Stages one chunk. A seq already received changes nothing and answers
  // the same counts (the uploads are minted afresh: a token is ten minutes).
  chunk: authedProcedure
    .input(
      z.object({
        importId: z.string().uuid(),
        seq: z.number().int().nonnegative(),
        rows: z.array(rowSchema).max(TEAM_IMPORT_CHUNK.maxRows + 1),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const imp = await loadImport(ctx.db, userId, input.importId)
      const refusal = validateChunk(input.rows)
      if (refusal) throw new TRPCError({ code: `BAD_REQUEST`, message: refusal })
      const attachmentRows = input.rows.filter((r) => r.table === `attachments`)
      const [seen] = await ctx.db
        .select({ seq: importRows.seq })
        .from(importRows)
        .where(and(eq(importRows.importId, imp.id), eq(importRows.seq, input.seq)))
        .limit(1)
      if (seen) {
        const mapped = await ctx.db
          .select({ localId: importIdMap.localId, serverId: importIdMap.serverId })
          .from(importIdMap)
          .where(and(eq(importIdMap.importId, imp.id), eq(importIdMap.tableName, `attachments`)))
        const serverIds = new Map(mapped.map((m) => [m.localId, m.serverId]))
        return {
          counts: imp.counts,
          uploads: attachmentRows.map((row) => uploadFor({ importId: imp.id, teamId: imp.teamId, userId, row, serverId: serverIds.get(row.id) ?? row.id })),
        }
      }
      const result = await ctx.db.transaction(async (tx) => {
        const values = input.rows.map((r, ordinal) => ({ importId: imp.id, seq: input.seq, ordinal, tableName: r.table, localId: r.id, row: r.row }))
        for (let i = 0; i < values.length; i += 200) {
          await tx.insert(importRows).values(values.slice(i, i + 200)).onConflictDoNothing()
        }
        const counts = { ...imp.counts }
        for (const r of input.rows) counts[r.table] = (counts[r.table] ?? 0) + 1
        await tx.update(teamImports).set({ counts }).where(eq(teamImports.id, imp.id))
        // An attachments id the server already holds is remapped NOW: the
        // signed upload lands the row under its final id.
        const serverIds = new Map<string, string>()
        if (attachmentRows.length > 0) {
          const taken = await tx
            .select({ id: attachments.id })
            .from(attachments)
            .where(
              inArray(
                attachments.id,
                attachmentRows.map((r) => r.id)
              )
            )
          const fresh = taken.map((t) => ({ importId: imp.id, tableName: `attachments`, localId: t.id, serverId: randomUUID() }))
          if (fresh.length > 0) await tx.insert(importIdMap).values(fresh).onConflictDoNothing()
          for (const f of fresh) serverIds.set(f.localId, f.serverId)
        }
        return { counts, serverIds }
      })
      return {
        counts: result.counts,
        uploads: attachmentRows.map((row) => uploadFor({ importId: imp.id, teamId: imp.teamId, userId, row, serverId: result.serverIds.get(row.id) ?? row.id })),
      }
    }),

  // Applies the staged rows and makes the team live. A failure keeps the
  // import open (its error is stored) so the client can repair and retry.
  commit: authedProcedure.input(z.object({ importId: z.string().uuid() })).mutation(async ({ ctx, input }) => {
    const userId = ctx.session.user.id
    const imp = await loadImport(ctx.db, userId, input.importId)
    const staged = await ctx.db
      .select({ tableName: importRows.tableName, localId: importRows.localId, row: importRows.row })
      .from(importRows)
      .where(eq(importRows.importId, imp.id))
      .orderBy(asc(importRows.seq), asc(importRows.ordinal))
    const rows: StagedRow[] = staged.map((r) => ({ tableName: r.tableName, localId: r.localId, row: (r.row ?? {}) as Record<string, unknown> }))
    // The plan's limits, before the transaction (a second pool connection
    // inside one deadlocks the pool): storage counts what already landed,
    // records what is about to be inserted.
    if (isCloudInstance()) {
      const [sum] = await ctx.db
        .select({ bytes: sql<string>`coalesce(sum(${attachments.sizeBytes}), 0)` })
        .from(attachments)
        .where(eq(attachments.teamId, imp.teamId))
      if (Number(sum?.bytes ?? 0) > TEAM_IMPORT_ATTACHMENTS_CAP_BYTES) {
        throw planLimitError(`up to ${Math.round(TEAM_IMPORT_ATTACHMENTS_CAP_BYTES / (1024 * 1024 * 1024))} GB of attachments in one import.`)
      }
      await assertWithinStorageLimit(imp.teamId, 0)
      await assertCanCreateAppRecord(imp.teamId, ctx.db, rows.filter((r) => r.tableName === `app_records`).length)
    }
    const ids = new IdMap()
    const mapped = await ctx.db
      .select({ tableName: importIdMap.tableName, localId: importIdMap.localId, serverId: importIdMap.serverId })
      .from(importIdMap)
      .where(eq(importIdMap.importId, imp.id))
    for (const m of mapped) ids.set(m.tableName, m.localId, m.serverId)
    const identity = {
      localUserId: imp.manifest.identity.userId,
      localEmail: imp.manifest.identity.email,
      callerId: userId,
      callerEmail: ctx.session.user.email,
    }
    const now = new Date()
    let result
    try {
      result = await ctx.db.transaction(async (tx) => {
        await setPreserveTimestamps(tx)
        const applied = await applyImport(tx, { importId: imp.id, teamId: imp.teamId, identity, deviceId: imp.manifest.deviceId ?? null, rows, ids, now })
        await tx.insert(teamMembers).values({ teamId: imp.teamId, userId, role: `owner` })
        await seedBuiltinSkills(tx, [imp.teamId])
        await tx
          .update(teams)
          .set({ importedFrom: { importId: imp.id, desktopId: imp.manifest.desktopId, name: imp.manifest.name, importedAt: now.toISOString() } })
          .where(eq(teams.id, imp.teamId))
        await tx.update(teamImports).set({ status: `committed`, committedAt: now, error: null, counts: applied.counts }).where(eq(teamImports.id, imp.id))
        await tx.delete(importRows).where(eq(importRows.importId, imp.id))
        const [team] = await tx.select(teamColumns).from(teams).where(eq(teams.id, imp.teamId)).limit(1)
        return { applied, team }
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      await ctx.db.update(teamImports).set({ error: message.slice(0, 2000) }).where(eq(teamImports.id, imp.id))
      throw error
    }
    // Post-commit, never inside the transaction (a concurrent shape renewal
    // would repopulate the cache with pre-commit membership).
    invalidateMembershipCaches()
    await recordConversionEvent(ctx.db, { name: `team_created`, userId, properties: { teamId: imp.teamId, imported: true } })
    return {
      team: result.team,
      remapped: result.applied.remapped,
      counts: result.applied.counts,
      idMap: ids.entries.filter((e) => e.localId !== e.serverId),
    }
  }),
}

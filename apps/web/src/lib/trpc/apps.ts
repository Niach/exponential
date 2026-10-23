import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, isNull, sql } from "drizzle-orm"
import { boardIconSchema } from "@exp/db-schema/domain"
import {
  APPS_COPY,
  APP_DESCRIPTION_MAX,
  APP_NAME_MAX,
  APP_NOTE_MAX,
  APP_SLUG_MAX,
  APP_SLUG_PATTERN,
  slugify,
} from "@exp/domain-contract/apps"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { appVersions, apps, sessions, type App } from "@/db/schema"
import { assertTeamMember, getTeamMember } from "@/lib/auth/membership"
import { validateAppPackage } from "@/lib/apps/package"
import { isUniqueViolation } from "@/lib/trpc/db-errors"
import type { Tx } from "@/lib/sessions/tx"

// VAPP-110: Exponential APPS. The rows sync (the `apps` shape carries the
// head package; `app_versions` their metadata); the writes here return a
// txId sync barrier. Reads and creates are member-gated; renaming and
// deleting = the creator or a team owner; publishing a version
// (`upsertPackage`, MCP `apps upsert` from the editor session) = any member;
// `restore` writes a NEW head copying an older version (no git).

const noNul = { message: `Must not contain NUL bytes` }
const hasNoNul = (value: string) => !value.includes(`\u0000`)
const nameSchema = z.string().trim().min(1).max(APP_NAME_MAX).refine(hasNoNul, noNul)
const slugSchema = z.string().trim().min(1).max(APP_SLUG_MAX).regex(APP_SLUG_PATTERN, `a slug is lowercase letters, digits and dashes`)
const descriptionSchema = z.string().max(APP_DESCRIPTION_MAX).refine(hasNoNul, noNul)
const noteSchema = z.string().trim().max(APP_NOTE_MAX).refine(hasNoNul, noNul)
const colorSchema = z.string().regex(/^#[0-9a-fA-F]{6}$/)
const packageSchema = z.record(z.string(), z.unknown())

const uuid = z.string().uuid()

/** One app by id or by (team, slug). */
const appRefSchema = z
  .object({ id: uuid.optional(), teamId: uuid.optional(), slug: slugSchema.optional() })
  .refine((v) => v.id || (v.teamId && v.slug), { message: `name an app by id, or by teamId + slug` })
type AppRef = z.infer<typeof appRefSchema>

async function loadApp(tx: Tx | typeof import("@/db/connection").db, ref: AppRef): Promise<App> {
  const where = ref.id
    ? eq(apps.id, ref.id)
    : and(eq(apps.teamId, ref.teamId!), eq(apps.slug, ref.slug!))
  const [app] = await tx.select().from(apps).where(and(where, isNull(apps.deletedAt))).limit(1)
  if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  return app
}

async function assertCanManage(userId: string, app: App) {
  const member = await getTeamMember(userId, app.teamId)
  if (!member) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  // Release train 2026-10-23: a NULL creator (a deleted account) = owner-only.
  if (member.role !== `owner` && (app.createdBy == null || app.createdBy !== userId)) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Only the app's creator or a team owner can do that` })
  }
}

/** A new version row + the app's head, in one transaction. */
export async function writeVersionInTx(
  tx: Tx,
  app: App,
  pkg: Record<string, unknown>,
  args: { note: string | null; sessionId: string | null; createdBy: string | null }
): Promise<{ number: number; versionId: string }> {
  const [locked] = await tx
    .select({ headVersion: apps.headVersion })
    .from(apps)
    .where(eq(apps.id, app.id))
    .for(`update`)
  const number = (locked?.headVersion ?? app.headVersion) + 1
  const [version] = await tx
    .insert(appVersions)
    .values({
      appId: app.id,
      teamId: app.teamId,
      number,
      package: pkg,
      note: args.note,
      sessionId: args.sessionId,
      createdBy: args.createdBy,
    })
    .returning({ id: appVersions.id })
  await tx
    .update(apps)
    .set({ package: pkg, headVersion: number, updatedAt: new Date() })
    .where(eq(apps.id, app.id))
  return { number, versionId: version!.id }
}

// Release train 2026-10-23: strip mode, so a newer client's extra field never 400s.
export const appCreateInputSchema = z.object({
  teamId: uuid,
  name: nameSchema,
  slug: slugSchema.optional(),
  icon: boardIconSchema.nullable().optional(),
  color: colorSchema.nullable().optional(),
  description: descriptionSchema.optional(),
  /** An initial package (MCP `apps upsert` on a missing slug). */
  package: packageSchema.optional(),
  note: noteSchema.optional(),
})

export const appUpdateInputSchema = z.object({
  id: uuid,
  name: nameSchema.optional(),
  icon: boardIconSchema.nullable().optional(),
  color: colorSchema.nullable().optional(),
  description: descriptionSchema.optional(),
})

async function takenSlugs(tx: Tx, teamId: string): Promise<string[]> {
  // Release train 2026-10-23: only live slugs count (the unique index is partial).
  const rows = await tx
    .select({ slug: apps.slug })
    .from(apps)
    .where(and(eq(apps.teamId, teamId), isNull(apps.deletedAt)))
  return rows.map((r) => r.slug)
}

export const appsRouter = router({
  list: authedProcedure
    .input(z.object({ teamId: uuid }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .select({
          id: apps.id,
          teamId: apps.teamId,
          slug: apps.slug,
          name: apps.name,
          icon: apps.icon,
          color: apps.color,
          description: apps.description,
          headVersion: apps.headVersion,
          createdBy: apps.createdBy,
          createdAt: apps.createdAt,
          updatedAt: apps.updatedAt,
        })
        .from(apps)
        .where(and(eq(apps.teamId, input.teamId), isNull(apps.deletedAt)))
        .orderBy(asc(apps.name))
      return { apps: rows }
    }),

  /** One app with its head package, or a named version's. */
  get: authedProcedure
    .input(appRefSchema.and(z.object({ version: z.number().int().min(1).optional() })))
    .query(async ({ ctx, input }) => {
      const app = await loadApp(ctx.db, input)
      await assertTeamMember(ctx.session.user.id, app.teamId)
      if (input.version === undefined || input.version === app.headVersion) return { app, version: app.headVersion, package: app.package }
      const [row] = await ctx.db
        .select({ package: appVersions.package, number: appVersions.number })
        .from(appVersions)
        .where(and(eq(appVersions.appId, app.id), eq(appVersions.number, input.version)))
        .limit(1)
      if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Version not found` })
      return { app, version: row.number, package: row.package }
    }),

  /** The versions list (metadata; the synced shape is the usual read). */
  versions: authedProcedure
    .input(z.object({ id: uuid }))
    .query(async ({ ctx, input }) => {
      const app = await loadApp(ctx.db, { id: input.id })
      await assertTeamMember(ctx.session.user.id, app.teamId)
      const rows = await ctx.db
        .select({
          id: appVersions.id,
          number: appVersions.number,
          note: appVersions.note,
          sessionId: appVersions.sessionId,
          createdBy: appVersions.createdBy,
          createdAt: appVersions.createdAt,
        })
        .from(appVersions)
        .where(eq(appVersions.appId, app.id))
        .orderBy(desc(appVersions.number))
      return { versions: rows }
    }),

  create: authedProcedure
    .input(appCreateInputSchema)
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamMember(userId, input.teamId)
      const pkg = input.package ? validateAppPackage(input.package) : null
      // Release train 2026-10-23: hoisted so the CONFLICT message names a derived slug too.
      let slug = input.slug
      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          slug = input.slug ?? slugify(input.name, await takenSlugs(tx, input.teamId))
          const [app] = await tx
            .insert(apps)
            .values({
              teamId: input.teamId,
              slug,
              name: input.name,
              icon: input.icon ?? null,
              color: input.color ?? null,
              description: input.description ?? ``,
              createdBy: userId,
            })
            .returning()
          let headVersion = 0
          if (pkg) {
            headVersion = (await writeVersionInTx(tx, app!, pkg as unknown as Record<string, unknown>, { note: input.note ?? null, sessionId: null, createdBy: userId })).number
          }
          return { txid: txId, app: { ...app!, headVersion, package: pkg ?? null } }
        })
      } catch (error) {
        if (isUniqueViolation(error)) {
          throw new TRPCError({ code: `CONFLICT`, message: `An app with the slug "${slug ?? input.slug}" already exists in this team` })
        }
        throw error
      }
    }),

  update: authedProcedure
    .input(appUpdateInputSchema)
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, { id: input.id })
      await assertCanManage(userId, app)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const { id, ...patch } = input
        const [updated] = await tx
          .update(apps)
          .set({ ...patch, updatedAt: new Date() })
          .where(eq(apps.id, id))
          .returning()
        return { txid: txId, app: updated! }
      })
    }),

  /** A NEW version of the package (the head follows). Any member. */
  upsertPackage: authedProcedure
    .input(appRefSchema.and(z.object({ package: packageSchema, note: noteSchema.optional(), sessionId: uuid.optional() })))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, input)
      await assertTeamMember(userId, app.teamId)
      const pkg = validateAppPackage(input.package)
      // Release train 2026-10-23: the version always names its author; the
      // session is kept only when it is the caller's own editor session for
      // THIS app (anything else is ignored, never a 400: old callers).
      let sessionId: string | null = null
      if (input.sessionId) {
        const [own] = await ctx.db
          .select({ id: sessions.id })
          .from(sessions)
          .where(and(eq(sessions.id, input.sessionId), eq(sessions.appId, app.id), eq(sessions.userId, userId)))
          .limit(1)
        sessionId = own?.id ?? null
      }
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const { number } = await writeVersionInTx(tx, app, pkg as unknown as Record<string, unknown>, {
          note: input.note?.trim() || null,
          sessionId,
          createdBy: userId,
        })
        return { txid: txId, version: number }
      })
    }),

  /** Restore = a new head copying version `number`. Any member. */
  restore: authedProcedure
    .input(z.object({ id: uuid, number: z.number().int().min(1) }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, { id: input.id })
      await assertTeamMember(userId, app.teamId)
      const [row] = await ctx.db
        .select({ package: appVersions.package })
        .from(appVersions)
        .where(and(eq(appVersions.appId, app.id), eq(appVersions.number, input.number)))
        .limit(1)
      if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Version not found` })
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const { number } = await writeVersionInTx(tx, app, row.package, {
          note: APPS_COPY.restoredNote.replace(`{n}`, String(input.number)),
          sessionId: null,
          createdBy: userId,
        })
        return { txid: txId, version: number, restored: input.number }
      })
    }),

  /** Trash (the shape's `deleted_at` mirror). The creator or an owner. */
  delete: authedProcedure
    .input(appRefSchema)
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, input)
      await assertCanManage(userId, app)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(apps)
          .set({ deletedAt: sql`now()`, updatedAt: new Date() })
          .where(eq(apps.id, app.id))
        return { txid: txId }
      })
    }),
})

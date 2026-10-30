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
import { appRecords, appVersions, apps, sessions, type App } from "@/db/schema"
import { assertTeamMember, getTeamMember } from "@/lib/auth/membership"
import { resolveTeamAccess } from "@/lib/auth/access"
import { appPackageIssues, packageBytes, recordKindSchema, validateAppPackage } from "@/lib/apps/package"
import { APP_EXPORT_COPY, APP_EXPORT_LIMITS, parseAppImportShape, type ImportIssue, type ParsedAppImport } from "@exp/domain-contract/app-export"
import { validateRecord, verifyPackageFile } from "@exponential_app/eui"
import { assertCanCreateAppRecord } from "@/lib/billing"
import { APP_RECORD_KIND_PATTERN, APP_RECORD_MAX_BYTES } from "@exp/domain-contract/apps"
import { APP_RECORDS_QUERY } from "@exp/domain-contract/app-records"
import { APP_ACTIONS, actionGrantable, canGrantActions, declaredActions } from "@exp/domain-contract/app-actions"
import { isUniqueViolation, uniqueViolationConstraint } from "@/lib/trpc/db-errors"
import type { Tx } from "@/lib/sessions/tx"

// VAPP-110: Exponential APPS. The rows sync (the `apps` shape carries the
// head package; `app_versions` their metadata); the writes here return a
// txId sync barrier. Reads and creates are member-gated; renaming and
// deleting = the creator or a team owner; publishing a version
// (`upsertPackage`, MCP `apps upsert` from the editor session) = any member;
// `restore` writes a NEW head copying an older version (no git). A version
// written by a non-owner clears the grants (release train 2026-10-25).

const noNul = { message: `Must not contain NUL bytes` }
const hasNoNul = (value: string) => !value.includes(`\u0000`)
const nameSchema = z.string().trim().min(1).max(APP_NAME_MAX).refine(hasNoNul, noNul)
const slugSchema = z.string().trim().min(1).max(APP_SLUG_MAX).regex(APP_SLUG_PATTERN, `a slug is lowercase letters, digits and dashes`)
const descriptionSchema = z.string().max(APP_DESCRIPTION_MAX).refine(hasNoNul, noNul)
const noteSchema = z.string().trim().max(APP_NOTE_MAX).refine(hasNoNul, noNul)
const colorSchema = z.string().regex(/^#[0-9a-fA-F]{6}$/)
const packageSchema = z.record(z.string(), z.unknown())

const uuid = z.string().uuid()
const kindSchema = z.string().trim().regex(APP_RECORD_KIND_PATTERN, `a kind is lowercase letters, digits and underscores`)
const recordDataSchema = z.record(z.string(), z.unknown())

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

/** VAPP-120 (the 2026-10-24 ruling): only a team OWNER sets or changes an
 *  app's grants; the creator gets no grant power from creating it. */
async function assertCanGrant(userId: string, app: App) {
  const member = await getTeamMember(userId, app.teamId)
  if (!member) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  if (!canGrantActions(member.role)) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner can change what an app runs without asking` })
  }
}

async function assertCanManage(userId: string, app: App) {
  const member = await getTeamMember(userId, app.teamId)
  if (!member) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  // Release train 2026-10-23: a NULL creator (a deleted account) = owner-only.
  if (member.role !== `owner` && (app.createdBy == null || app.createdBy !== userId)) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Only the app's creator or a team owner can do that` })
  }
}

/** VAPP-120: the typed actions a stored package's `functions` list declares
 *  (patterns expanded; a missing or malformed list declares none). */
export function packageActions(pkg: unknown): string[] {
  const functions = (pkg as { functions?: unknown } | null)?.functions
  return declaredActions(Array.isArray(functions) ? functions.filter((f): f is string => typeof f === `string`) : [])
}

/** A new version row + the app's head, in one transaction. VAPP-120: the
 *  grants follow the head: a name the new package no longer declares loses
 *  its grant, so a later version re-declaring it asks again (review of
 *  #1222: a grant is a person's choice about THIS package, never inherited).
 *  Release train 2026-10-25: only an OWNER's version keeps the still-declared
 *  grants; code written by anyone else (`byOwner: false`) starts with none,
 *  so a member's code never runs on an owner's grant without asking. */
export async function writeVersionInTx(
  tx: Tx,
  app: App,
  pkg: Record<string, unknown>,
  args: { note: string | null; sessionId: string | null; createdBy: string | null; byOwner: boolean }
): Promise<{ number: number; versionId: string }> {
  const [locked] = await tx
    .select({ headVersion: apps.headVersion, grantedTools: apps.grantedTools })
    .from(apps)
    .where(eq(apps.id, app.id))
    .for(`update`)
  const number = (locked?.headVersion ?? app.headVersion) + 1
  const declared = packageActions(pkg)
  // VAPP-158: a stored grant that is no longer grantable (an owner-only or
  // destructive action) is dropped on the way, never carried to a new head.
  const grantedTools = args.byOwner ? (locked?.grantedTools ?? app.grantedTools ?? []).filter((name) => declared.includes(name) && actionGrantable(name)) : []
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
    .set({ package: pkg, headVersion: number, grantedTools, updatedAt: new Date() })
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
  // VAPP-120: the typed host actions the app may run without asking (exact
  // names out of the contract's list; the creator or an owner).
  grantedTools: z
    .array(
      z
        .string()
        .refine((name) => APP_ACTIONS.includes(name), { message: `not an offered action` })
        // The ruling: a destructive or owner-only action (VAPP-158) can never
        // be granted; it always asks.
        .refine((name) => actionGrantable(name), { message: `a destructive or owner-only action always asks and cannot be granted` })
    )
    .max(200)
    .optional(),
})

/** VAPP-121: an import's text, checked end to end (the structure, then the
 *  package's rules, then every record against its kind's schema and the
 *  record cap): the issues the import step lists, and the file when there
 *  are none. A package's issue paths sit under `/package` in an app file. */
export async function checkAppImport(text: string): Promise<ParsedAppImport> {
  const { issues: structural, file, bare } = parseAppImportShape(text)
  if (!file) return { issues: structural, file: null }
  // VAPP-161: a signed file (the package format's `author` + `signature`)
  // must verify; an unsigned one imports as before.
  if (!bare) {
    const verified = await verifyPackageFile(JSON.parse(text))
    if (verified.status === `invalid`) return { issues: verified.issues, file: null }
  }
  const at = (path: string) => (bare ? path : `/package${path === `/` ? `` : path}`)
  const issues: ImportIssue[] = appPackageIssues(file.package).map((issue) => ({ path: at(issue.path), message: issue.message }))
  if (issues.length === 0) {
    file.records.forEach((record, i) => {
      const schema = recordKindSchema(file.package, record.kind)
      if (!schema) return void issues.push({ path: `/records/${i}/kind`, message: `the package declares no record kind "${record.kind}"` })
      for (const issue of validateRecord(schema, record.data)) issues.push({ path: `/records/${i}/data${issue.path === `/` ? `` : issue.path}`, message: issue.message })
      const bytes = packageBytes(record.data)
      if (bytes > APP_RECORD_MAX_BYTES) issues.push({ path: `/records/${i}/data`, message: `is ${bytes} bytes; the cap is ${APP_RECORD_MAX_BYTES}` })
    })
  }
  return issues.length > 0 ? { issues, file: null } : { issues: [], file }
}

const IMPORT_SLUG_ATTEMPTS = 5

const importInputSchema = z.object({ teamId: uuid, text: z.string().max(APP_EXPORT_LIMITS.importMaxBytes * 2) })

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
            headVersion = (await writeVersionInTx(tx, app!, pkg as unknown as Record<string, unknown>, { note: input.note ?? null, sessionId: null, createdBy: userId, byOwner: false })).number
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

  /** VAPP-121: the import step's live check (no write): `issues` = the
   *  rows it lists under the field; empty = Import is enabled. Any member. */
  validateImport: authedProcedure
    .input(importInputSchema)
    .mutation(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      const { issues, file } = await checkAppImport(input.text)
      return { issues, name: file?.app.name ?? null, records: file?.records.length ?? 0 }
    }),

  /** VAPP-121: a new app out of an app file (or a bare package): the
   *  identity from the file, the package as v1 noted "Imported", the records
   *  as the importer's within the team's quota. No grants: an owner grants
   *  on the settings page. Any member. */
  import: authedProcedure
    .input(importInputSchema)
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamMember(userId, input.teamId)
      const { issues, file } = await checkAppImport(input.text)
      if (!file) {
        const listed = issues.slice(0, 10).map((issue) => `${issue.path}: ${issue.message}`)
        throw new TRPCError({ code: `BAD_REQUEST`, message: `the app file is invalid: ${listed.join(`; `)}` })
      }
      const icon = boardIconSchema.safeParse(file.app.icon)
      const color = colorSchema.safeParse(file.app.color)
      const importInTx = () => ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const slug = slugify(file.app.name, await takenSlugs(tx, input.teamId))
        const [app] = await tx
          .insert(apps)
          .values({
            teamId: input.teamId,
            slug,
            name: file.app.name,
            icon: icon.success ? icon.data : null,
            color: color.success ? color.data : null,
            description: file.app.description,
            createdBy: userId,
          })
          .returning()
        const { number } = await writeVersionInTx(tx, app!, file.package, { note: APP_EXPORT_COPY.importNote, sessionId: null, createdBy: userId, byOwner: false })
        if (file.records.length > 0) {
          await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`app_records:${input.teamId}`}))`)
          await assertCanCreateAppRecord(input.teamId, tx, file.records.length)
          await tx.insert(appRecords).values(file.records.map((r) => ({ appId: app!.id, teamId: input.teamId, kind: r.kind, data: r.data, createdBy: userId })))
        }
        return { txid: txId, app: { ...app!, headVersion: number }, records: file.records.length }
      })
      // VAPP-154: two same-name imports at once derive the same slug; the
      // loser's insert hits the unique index and retries on a fresh read of
      // the taken slugs, like a sequential duplicate (`-2`). Out of
      // attempts = CONFLICT, never a 500.
      for (let attempt = 1; ; attempt++) {
        try {
          return await importInTx()
        } catch (error) {
          if (uniqueViolationConstraint(error) !== `uniq_apps_team_slug`) throw error
          if (attempt >= IMPORT_SLUG_ATTEMPTS) {
            throw new TRPCError({ code: `CONFLICT`, message: `An app named "${file.app.name}" is being imported at the same time; try again` })
          }
        }
      }
    }),

  update: authedProcedure
    .input(appUpdateInputSchema)
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, { id: input.id })
      await assertCanManage(userId, app)
      // VAPP-120: a grant names only what the HEAD package declares (review
      // of #1222: nothing is granted ahead of a version that would use it),
      // and only an OWNER grants (the ruling: the creator gets no grant power).
      if (input.grantedTools) {
        await assertCanGrant(userId, app)
        const declared = packageActions(app.package)
        const undeclared = input.grantedTools.filter((name) => !declared.includes(name))
        if (undeclared.length > 0) {
          throw new TRPCError({ code: `BAD_REQUEST`, message: `The app's package does not declare ${undeclared.join(`, `)}: a grant names only what the head version calls` })
        }
      }
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

  /** A NEW version of the package (the head follows). Any member; a
   *  non-owner's version clears the grants (`writeVersionInTx`). */
  upsertPackage: authedProcedure
    .input(appRefSchema.and(z.object({ package: packageSchema, note: noteSchema.optional(), sessionId: uuid.optional() })))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, input)
      const { member } = await resolveTeamAccess(userId, app.teamId)
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
          byOwner: canGrantActions(member.role),
        })
        return { txid: txId, version: number }
      })
    }),

  /** Restore = a new head copying version `number`. Any member; a
   *  non-owner's restore clears the grants like a new version. */
  restore: authedProcedure
    .input(z.object({ id: uuid, number: z.number().int().min(1) }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, { id: input.id })
      const { member } = await resolveTeamAccess(userId, app.teamId)
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
          byOwner: canGrantActions(member.role),
        })
        return { txid: txId, version: number, restored: input.number }
      })
    }),

  // VAPP-122: an app's RECORDS (fixtures/app-records.json). Any member reads
  // and writes them (team data the app owns); a write is validated against
  // the head package's kind schema, capped per record and per team.
  records: router({
    list: authedProcedure
      .input(
        appRefSchema.and(
          z.object({
            kind: kindSchema.optional(),
            limit: z.number().int().min(1).max(APP_RECORDS_QUERY.limitMax).default(APP_RECORDS_QUERY.limitDefault),
            offset: z.number().int().min(0).default(0),
          })
        )
      )
      .query(async ({ ctx, input }) => {
        const app = await loadApp(ctx.db, input)
        await assertTeamMember(ctx.session.user.id, app.teamId)
        const rows = await ctx.db
          .select({
            id: appRecords.id,
            kind: appRecords.kind,
            data: appRecords.data,
            createdBy: appRecords.createdBy,
            createdAt: appRecords.createdAt,
            updatedAt: appRecords.updatedAt,
          })
          .from(appRecords)
          .where(and(eq(appRecords.appId, app.id), isNull(appRecords.deletedAt), input.kind ? eq(appRecords.kind, input.kind) : undefined))
          .orderBy(desc(appRecords.createdAt), asc(appRecords.id))
          .limit(input.limit)
          .offset(input.offset)
        return { records: rows }
      }),

    /** Create a record, or (with `id`) patch that record's data (shallow). */
    upsert: authedProcedure
      .input(appRefSchema.and(z.object({ kind: kindSchema, data: recordDataSchema, recordId: uuid.optional() })))
      .mutation(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const app = await loadApp(ctx.db, input)
        await assertTeamMember(userId, app.teamId)
        const schema = recordKindSchema(app.package, input.kind)
        if (!schema) {
          throw new TRPCError({ code: `BAD_REQUEST`, message: `The app declares no record kind "${input.kind}": add it to the package's records first` })
        }
        const checked = (data: Record<string, unknown>) => {
          const issues = validateRecord(schema, data)
          if (issues.length > 0) {
            const listed = issues.slice(0, 10).map((issue) => `${issue.path || `/`}: ${issue.message}`)
            throw new TRPCError({ code: `BAD_REQUEST`, message: `record is invalid: ${listed.join(`; `)}` })
          }
          const bytes = packageBytes(data)
          if (bytes > APP_RECORD_MAX_BYTES) {
            throw new TRPCError({ code: `BAD_REQUEST`, message: `record is ${bytes} bytes; the cap is ${APP_RECORD_MAX_BYTES}` })
          }
          return data
        }
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          if (input.recordId) {
            // Release train 2026-10-25: the patch merges onto the row as
            // locked in THIS transaction, so two concurrent patches queue
            // here and neither loses the other's fields.
            const [existing] = await tx
              .select({ id: appRecords.id, data: appRecords.data })
              .from(appRecords)
              .where(and(eq(appRecords.id, input.recordId), eq(appRecords.appId, app.id), eq(appRecords.kind, input.kind), isNull(appRecords.deletedAt)))
              .limit(1)
              .for(`update`)
            if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
            const data = checked({ ...existing.data, ...input.data })
            await tx.update(appRecords).set({ data, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
            return { txid: txId, id: existing.id, created: false }
          }
          const data = checked(input.data)
          // The team's quota, counted in THIS transaction behind the team's
          // lock: concurrent creates at the limit queue here and the second
          // one sees the first's row (no race past `appRecords`).
          await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`app_records:${app.teamId}`}))`)
          await assertCanCreateAppRecord(app.teamId, tx)
          const [row] = await tx
            .insert(appRecords)
            .values({ appId: app.id, teamId: app.teamId, kind: input.kind, data, createdBy: userId })
            .returning({ id: appRecords.id })
          return { txid: txId, id: row!.id, created: true }
        })
      }),

    delete: authedProcedure
      .input(appRefSchema.and(z.object({ recordId: uuid })))
      .mutation(async ({ ctx, input }) => {
        const app = await loadApp(ctx.db, input)
        await assertTeamMember(ctx.session.user.id, app.teamId)
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          const deleted = await tx
            .update(appRecords)
            .set({ deletedAt: sql`now()`, updatedAt: new Date() })
            .where(and(eq(appRecords.id, input.recordId), eq(appRecords.appId, app.id), isNull(appRecords.deletedAt)))
            .returning({ id: appRecords.id })
          if (deleted.length === 0) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
          return { txid: txId }
        })
      }),
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

import { z } from "zod"
import { createAppAgentInTx } from "@/lib/apps/app-agent"
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
  isDraftApp,
  slugify,
} from "@exp/domain-contract/apps"
import { router, authedProcedure, generateTxId, type Context } from "@/lib/trpc"
import { appVersions, apps, mcpServers, sessions, type App, type AppPackageJson } from "@/db/schema"
import { assertTeamMember, getTeamMember } from "@/lib/auth/membership"
import { resolveTeamAccess } from "@/lib/auth/access"
import { appPackageIssues, packageBytes, recordKindSchema, upstreamIssues, validateAppPackage } from "@/lib/apps/package"
import { APP_TEMPLATES_COPY } from "@exp/domain-contract/app-templates"
import { APP_EXPORT_COPY, APP_EXPORT_LIMITS, parseAppImportShape, type ImportIssue, type ParsedAppImport } from "@exp/domain-contract/app-export"
import { ACTION_LIMITS, validateRecord, verifyPackageFile, type AppPackage } from "@exponential_app/eui"
import { teamAppLevels } from "@/lib/apps/app-access-level"
import { appAccessLevel, commitRecordBatch, deleteRecord, importRecords, listRecordEvents, listRecords, loadApp, queryRecords, restoreRecord, teamMemberIds, undoEvents, upsertRecord, type RecordWriteVia } from "@/lib/apps/records-service"
import { RECORD_POLICY_COPY, migrationRefusal, packageMigrations } from "@exponential_app/eui/host"
import { APP_RECORD_KIND_PATTERN, APP_RECORD_MAX_BYTES } from "@exp/domain-contract/apps"
import { APP_CSV_IMPORT, importAuthor } from "@exp/domain-contract/app-records"
import { VIA_APP_RECORDS_FUNCTION, type ViaAppClaim } from "@exp/domain-contract/via-app"
import { verifiedViaApp } from "@/lib/apps/via-app-claim"
import { RECORDS_QUERY } from "@exponential_app/eui/host/query"
import { APP_ACTIONS, actionGrantable, canGrantActions, declaredActions, flatAction, storedAction } from "@exp/domain-contract/app-actions"
import { isUniqueViolation, uniqueViolationConstraint } from "@/lib/trpc/db-errors"
import type { Tx } from "@/lib/sessions/tx"
import { readUpstreamBinding } from "@/lib/emcp/upstream-reads"
import { recordMcpCalls, type McpCallRow } from "@/lib/emcp/audit"
import { bindingsOf, declaredUpstreams, writeUpstreamMappings } from "@/lib/apps/upstream-mapping"
import { resolveUpstreamServer } from "@exp/emcp/upstream"
import { runVia, validateExposes, type AgentRun, type Expose } from "@exp/emcp/exposes"
import { handOff } from "@/lib/apps/agent-handoff"
import { templateImportText } from "@/lib/apps/templates"

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

/** VAPP-215: where a client says its write came from (the Data tab =
 *  editor, an app's own UI = app); the server's own context wins. */
const clientVia = z.enum([`editor`, `app`]).optional()
/** An MCP call = mcp, an app's MCP tool = app, else what the client said. */
function writeVia(ctx: Pick<Context, `viaMcp` | `viaAppId`>, via: `editor` | `app` | undefined): RecordWriteVia {
  if (ctx.viaAppId) return `app`
  if (ctx.viaMcp) return `mcp`
  return via ?? `app`
}

/** VAPP-231 (via-app.json `claim`): the host's claim that the write comes
 *  from this app's own UI; checked here, it only adds a caption. */
const viaAppClaimSchema = z.object({ appId: uuid, version: z.number().int() }).optional()
/** The app a record write reads "via": the broker's stamp, else a claim that
 *  names the app written to and holds. */
async function recordsViaApp(ctx: Pick<Context, `viaAppId` | `db`> & { session: { user: { id: string } } }, input: { id?: string; viaApp?: ViaAppClaim }): Promise<string | undefined> {
  if (ctx.viaAppId) return ctx.viaAppId
  if (!input.viaApp || input.viaApp.appId !== input.id) return undefined
  return (await verifiedViaApp(ctx.session.user.id, input.viaApp, [{ fn: VIA_APP_RECORDS_FUNCTION }], ctx.db)) ?? undefined
}

/** One app by id or by (team, slug). */
const appRefSchema = z
  .object({ id: uuid.optional(), teamId: uuid.optional(), slug: slugSchema.optional() })
  .refine((v) => v.id || (v.teamId && v.slug), { message: `name an app by id, or by teamId + slug` })

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

/** eApps V1 (#VAPP-210): the version a grant is made on and checked
 *  against: the published one, or the head while nothing is published. */
export function grantVersion(app: Pick<App, `headVersion` | `publishedVersion`>): number {
  return app.publishedVersion > 0 ? app.publishedVersion : app.headVersion
}

/** eApps V1: the package grants refer to (the published one, else the head). */
function grantPackage(app: Pick<App, `package` | `draftPackage`>): unknown {
  return app.package ?? app.draftPackage
}

/** VAPP-120: the grants follow the package that RUNS: a name it no longer
 *  declares loses its grant, so a later version re-declaring it asks again
 *  (review of #1222: a grant is a person's choice about THIS package, never
 *  inherited). Release train 2026-10-25: only an OWNER keeps the
 *  still-declared grants; anyone else (`byOwner: false`) starts with none,
 *  so a member's code never runs on an owner's grant without asking. */
function carriedGrants(current: readonly string[] | null | undefined, pkg: unknown, byOwner: boolean): string[] {
  const declared = packageActions(pkg)
  // VAPP-158: a stored grant that is no longer grantable (an owner-only or
  // destructive action) is dropped on the way.
  return byOwner ? (current ?? []).filter((name) => declared.includes(flatAction(name)) && actionGrantable(name)) : []
}

/** A new version row = the new HEAD (the editor's draft), in one
 *  transaction. eApps V1 (#VAPP-210): it never touches the published
 *  `package`; members keep running the published version until a Publish
 *  (`publishVersionInTx`). While nothing is published the grants follow the
 *  head as before (they refer to it, `grantVersion`). */
export async function writeVersionInTx(
  tx: Tx,
  app: App,
  pkg: Record<string, unknown>,
  args: { note: string | null; sessionId: string | null; createdBy: string | null; byOwner: boolean }
): Promise<{ number: number; versionId: string }> {
  const [locked] = await tx
    .select({ headVersion: apps.headVersion, publishedVersion: apps.publishedVersion, grantedTools: apps.grantedTools, package: apps.package })
    .from(apps)
    .where(eq(apps.id, app.id))
    .for(`update`)
  const number = (locked?.headVersion ?? app.headVersion) + 1
  const unpublished = (locked?.publishedVersion ?? app.publishedVersion) === 0
  // eApps R8 (#VAPP-233): a major bump that changes a published kind
  // incompatibly needs a migration that covers it (record-migrations.json).
  const refusal = unpublished ? null : migrationRefusal(locked?.package ?? app.package, pkg)
  if (refusal) throw new TRPCError({ code: `BAD_REQUEST`, message: refusal })
  const grantedTools = unpublished ? carriedGrants(locked?.grantedTools ?? app.grantedTools, pkg, args.byOwner) : (locked?.grantedTools ?? app.grantedTools ?? [])
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
    .set({ draftPackage: pkg, headVersion: number, grantedTools, updatedAt: new Date() })
    .where(eq(apps.id, app.id))
  return { number, versionId: version!.id }
}

/** eApps V1 (#VAPP-210): PUBLISH version `number` (default: the head; an
 *  older one = a rollback): `package` becomes its package and members,
 *  exposed tools and automations run it from now on; `draft_package` keeps
 *  the head's package while the head is ahead of it. The grants follow the
 *  published package (`carriedGrants`: an owner's publish keeps the
 *  still-declared ones, anyone else's clears them). */
export async function publishVersionInTx(
  tx: Tx,
  app: Pick<App, `id`>,
  args: { number?: number; byOwner: boolean; userId: string | null }
): Promise<{ published: number; head: number }> {
  const [locked] = await tx
    .select({ headVersion: apps.headVersion, publishedVersion: apps.publishedVersion, grantedTools: apps.grantedTools, package: apps.package, draftPackage: apps.draftPackage })
    .from(apps)
    .where(eq(apps.id, app.id))
    .for(`update`)
  if (!locked || locked.headVersion === 0) throw new TRPCError({ code: `BAD_REQUEST`, message: APPS_COPY.publishNothing })
  const number = args.number ?? locked.headVersion
  const headPackage = locked.draftPackage ?? locked.package
  let pkg: AppPackageJson | null = number === locked.headVersion ? headPackage : null
  if (!pkg) {
    const [row] = await tx
      .select({ package: appVersions.package })
      .from(appVersions)
      .where(and(eq(appVersions.appId, app.id), eq(appVersions.number, number)))
      .limit(1)
    if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Version not found` })
    pkg = row.package
  }
  await tx
    .update(apps)
    .set({
      package: pkg,
      publishedVersion: number,
      // #VAPP-220: the automations' default sponsor.
      publishedBy: args.userId,
      draftPackage: number === locked.headVersion ? null : headPackage,
      grantedTools: carriedGrants(locked.grantedTools, pkg, args.byOwner),
      updatedAt: new Date(),
    })
    .where(eq(apps.id, app.id))
  // eApps R8 (#VAPP-233): a version with migrations sweeps the app's records
  // into its shape (lib/apps/record-sweep.ts); a sweep still running for an
  // earlier publish stops there (superseded).
  await tx
    .update(appVersions)
    .set({ migrationState: `superseded` })
    .where(and(eq(appVersions.appId, app.id), eq(appVersions.migrationState, `running`)))
  if (locked.publishedVersion > 0 && number !== locked.publishedVersion && packageMigrations(pkg).length > 0)
    await tx
      .update(appVersions)
      .set({ migrationState: `running`, migrationFrom: locked.publishedVersion, migrationTotal: 0, migrationDone: 0, migrationError: null, migrationBy: args.userId })
      .where(and(eq(appVersions.appId, app.id), eq(appVersions.number, number)))
  return { published: number, head: locked.headVersion }
}

// Release train 2026-10-23: strip mode, so a newer client's extra field never 400s.
export const appCreateInputSchema = z.object({
  teamId: uuid,
  /** VAPP-176: none = a DRAFT named "Untitled app" (New app ×4). */
  name: nameSchema.optional(),
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
        // Round 9 §14: judged flat (a client older than the flag day sends
        // dotted names), stored dotted (it reads no other form).
        .transform(flatAction)
        .refine((name) => APP_ACTIONS.includes(name), { message: `not an offered action` })
        // The ruling: a destructive or owner-only action (VAPP-158) can never
        // be granted; it always asks.
        .refine((name) => actionGrantable(name), { message: `a destructive or owner-only action always asks and cannot be granted` })
    )
    .max(200)
    .optional(),
  // VAPP-156: the version the owner was looking at when they flipped a
  // switch (eApps V1: `grantVersion` = the published one, else the head; the
  // field keeps its name); a grant must name it, a newer one = CONFLICT.
  headVersion: z.number().int().min(0).optional(),
}).refine((v) => !v.grantedTools || v.headVersion !== undefined, { message: `a grant names the version it was made on (headVersion)`, path: [`headVersion`] })

/** VAPP-121: an import's text, checked end to end (the structure, then the
 *  package's rules, then every record against its kind's schema and the
 *  record cap): the issues the import step lists, and the file when there
 *  are none. A package's issue paths sit under `/package` in an app file. */
export async function checkAppImport(
  text: string,
  importer?: { userId: string; role: string | null; members: readonly string[] }
): Promise<ParsedAppImport> {
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
      // VAPP-188/192: a record's `author` resolves against the importing
      // team (app-records.json `onBehalf.importCases`): a stranger = the
      // importer; another member = them for an owner, else the importer.
      if (importer) record.author = importAuthor(importer.userId, importer.role, record.author, importer.members)
    })
  }
  return issues.length > 0 ? { issues, file: null } : { issues: [], file }
}

/** VAPP-180: a written package's `upstreams` are MCP servers of the app's
 *  team (an import keeps its names: an unknown one answers upstreamMissing
 *  until an owner adds the server or maps one). #VAPP-254: `known` = names
 *  the app already maps or declares. */
async function assertTeamUpstreams(db: Pick<Tx, `select`>, teamId: string, pkg: AppPackage, known: readonly string[] = []): Promise<void> {
  if (!pkg.upstreams?.length) return
  const rows = await db.select({ name: mcpServers.name }).from(mcpServers).where(eq(mcpServers.teamId, teamId))
  const issue = upstreamIssues(pkg, rows.map((r) => r.name), known)[0]
  if (issue) throw new TRPCError({ code: `BAD_REQUEST`, message: issue.message })
}

const IMPORT_SLUG_ATTEMPTS = 5

/** VAPP-121: a new app out of an app file (or a bare package): the
 *  identity from the file, the package as v1 noted `note` (published at
 *  once), the records as the importer's within the team's quota. No grants:
 *  an owner grants on the settings page. Any member. `import` and
 *  `importTemplate` (VAPP-234) share it. */
async function importAppText(ctx: Context & { session: NonNullable<Context[`session`]> }, teamId: string, text: string, note: string, upstreamMappings: readonly { upstream: string; serverId: string }[] = []) {
  const userId = ctx.session.user.id
  const member = await assertTeamMember(userId, teamId)
  const { issues, file } = await checkAppImport(text, { userId, role: member.role, members: await teamMemberIds(ctx.db, teamId) })
  if (!file) {
    const listed = issues.slice(0, 10).map((issue) => `${issue.path}: ${issue.message}`)
    throw new TRPCError({ code: `BAD_REQUEST`, message: `the app file is invalid: ${listed.join(`; `)}` })
  }
  // #VAPP-254 (upstream.json `mapping`): an owner's picks, for names the file declares.
  if (upstreamMappings.length > 0 && member.role !== `owner`) throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner picks an app's servers` })
  const declared = declaredUpstreams(file.package)
  const undeclared = upstreamMappings.find((m) => !declared.includes(m.upstream))
  if (undeclared) throw new TRPCError({ code: `BAD_REQUEST`, message: `the app does not declare ${undeclared.upstream}` })
  const icon = boardIconSchema.safeParse(file.app.icon)
  const color = colorSchema.safeParse(file.app.color)
  let mappingAudit: McpCallRow[] = []
  const importInTx = () => ctx.db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    const slug = slugify(file.app.name, await takenSlugs(tx, teamId))
    const [app] = await tx
      .insert(apps)
      .values({
        teamId,
        slug,
        name: file.app.name,
        icon: icon.success ? icon.data : null,
        color: color.success ? color.data : null,
        description: file.app.description,
        createdBy: userId,
      })
      .returning()
    // #EXP-1730: an app comes with its agent.
    app!.agentId = await createAppAgentInTx(tx, app!, userId)
    const { number } = await writeVersionInTx(tx, app!, file.package, { note, sessionId: null, createdBy: userId, byOwner: false })
    // eApps V1: an imported app file is a finished app: v1 is published at once.
    await publishVersionInTx(tx, app!, { number, byOwner: false, userId })
    mappingAudit = await writeUpstreamMappings(tx, app!, userId, upstreamMappings)
    // #VAPP-193: last in the transaction (it switches to role eapps).
    await importRecords(tx, { ...app!, package: file.package as App[`package`] }, userId, member.role, file.records)
    return { txid: txId, app: { ...app!, headVersion: number, publishedVersion: number }, records: file.records.length }
  })
  // VAPP-154: two same-name imports at once derive the same slug; the
  // loser's insert hits the unique index and retries on a fresh read of
  // the taken slugs, like a sequential duplicate (`-2`). Out of
  // attempts = CONFLICT, never a 500.
  for (let attempt = 1; ; attempt++) {
    try {
      const imported = await importInTx()
      await recordMcpCalls(mappingAudit)
      return imported
    } catch (error) {
      if (uniqueViolationConstraint(error) !== `uniq_apps_team_slug`) throw error
      if (attempt >= IMPORT_SLUG_ATTEMPTS) {
        throw new TRPCError({ code: `CONFLICT`, message: `An app named "${file.app.name}" is being imported at the same time; try again` })
      }
    }
  }
}

const importInputSchema = z.object({ teamId: uuid, text: z.string().max(APP_EXPORT_LIMITS.importMaxBytes * 2) })

async function takenSlugs(tx: Tx, teamId: string): Promise<string[]> {
  // Release train 2026-10-23: only live slugs count (the unique index is partial).
  const rows = await tx
    .select({ slug: apps.slug })
    .from(apps)
    .where(and(eq(apps.teamId, teamId), isNull(apps.deletedAt)))
  return rows.map((r) => r.slug)
}

/** eApps R3 (§9.2): the member's app access. `none` hides the app (NOT_FOUND,
 *  as if it did not exist); `read` refuses a package write (FORBIDDEN). */
async function assertAppAccess(db: Pick<typeof import("@/db/connection").db, `select`>, app: { id: string }, userId: string, need: `read` | `write`) {
  const level = await appAccessLevel(db, app.id, userId)
  if (level === `none`) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  if (need === `write` && level === `read`) throw new TRPCError({ code: `FORBIDDEN`, message: RECORD_POLICY_COPY.readOnly })
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
          publishedVersion: apps.publishedVersion,
          createdBy: apps.createdBy,
          createdAt: apps.createdAt,
          updatedAt: apps.updatedAt,
        })
        .from(apps)
        .where(and(eq(apps.teamId, input.teamId), isNull(apps.deletedAt)))
        .orderBy(asc(apps.name))
      // eApps R3: an app at access none is hidden (#FEED-242: the row, else
      // creator/owner write, else the app's default).
      const levels = await teamAppLevels(ctx.db, ctx.session.user.id, input.teamId)
      return { apps: rows.filter((r) => levels.get(r.id) !== `none`) }
    }),

  /** One app with its head package, or a named version's. */
  get: authedProcedure
    .input(appRefSchema.and(z.object({ version: z.number().int().min(1).optional() })))
    .query(async ({ ctx, input }) => {
      const app = await loadApp(ctx.db, input)
      await assertTeamMember(ctx.session.user.id, app.teamId)
      await assertAppAccess(ctx.db, app, ctx.session.user.id, `read`)
      // eApps V1: no version = the HEAD (the editor's draft, else the published package).
      if (input.version === undefined || input.version === app.headVersion) return { app, version: app.headVersion, package: app.draftPackage ?? app.package }
      if (input.version === app.publishedVersion) return { app, version: app.publishedVersion, package: app.package }
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
      await assertAppAccess(ctx.db, app, ctx.session.user.id, `read`)
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
      if (pkg) await assertTeamUpstreams(ctx.db, input.teamId, pkg)
      // Release train 2026-10-23: hoisted so the CONFLICT message names a derived slug too.
      let slug = input.slug
      const name = input.name ?? APPS_COPY.untitled
      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          slug = input.slug ?? slugify(name, await takenSlugs(tx, input.teamId))
          const [app] = await tx
            .insert(apps)
            .values({
              teamId: input.teamId,
              slug,
              name,
              icon: input.icon ?? null,
              color: input.color ?? null,
              description: input.description ?? ``,
              createdBy: userId,
            })
            .returning()
          // #EXP-1730: an app comes with its agent.
          app!.agentId = await createAppAgentInTx(tx, app!, userId)
          let headVersion = 0
          if (pkg) {
            headVersion = (await writeVersionInTx(tx, app!, pkg as unknown as Record<string, unknown>, { note: input.note ?? null, sessionId: null, createdBy: userId, byOwner: false })).number
            // eApps V1: an app created WITH a package (MCP `apps upsert` on a
            // new slug) publishes it as v1; every later version is a draft.
            await publishVersionInTx(tx, app!, { number: headVersion, byOwner: false, userId })
          }
          return { txid: txId, app: { ...app!, headVersion, publishedVersion: headVersion, package: pkg ?? null } }
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
      const userId = ctx.session.user.id
      const member = await assertTeamMember(userId, input.teamId)
      const { issues, file } = await checkAppImport(input.text, { userId, role: member.role, members: await teamMemberIds(ctx.db, input.teamId) })
      // #VAPP-254: the owner's picker (apps.json `upstreamPickerCases`).
      const names = file ? [...declaredUpstreams(file.package)].sort() : []
      const teamServers = names.length ? await ctx.db.select({ id: mcpServers.id, name: mcpServers.name }).from(mcpServers).where(eq(mcpServers.teamId, input.teamId)).orderBy(asc(mcpServers.name)) : []
      const upstreams = names.map((name) => {
        const server = resolveUpstreamServer(name, null, teamServers)?.server ?? null
        return { name, serverId: server?.id ?? null, serverName: server?.name ?? null }
      })
      return { issues, name: file?.app.name ?? null, records: file?.records.length ?? 0, upstreams, servers: member.role === `owner` ? teamServers : [] }
    }),

  /** VAPP-121: a new app out of an app file (or a bare package): the
   *  identity from the file, the package as v1 noted "Imported", the records
   *  as the importer's within the team's quota. No grants: an owner grants
   *  on the settings page. Any member. */
  import: authedProcedure
    .input(importInputSchema.and(z.object({ upstreamMappings: z.array(z.object({ upstream: z.string().min(1).max(64), serverId: uuid })).max(64).default([]) })))
    .mutation(({ ctx, input }) => importAppText(ctx, input.teamId, input.text, APP_EXPORT_COPY.importNote, input.upstreamMappings)),

  /** VAPP-234 (app-templates.json): a new app out of a SIGNED template of
   *  the gallery: the pinned signature verified, then imported exactly like
   *  `import` (v1 published, the sample records only when asked). Any member. */
  importTemplate: authedProcedure
    .input(z.object({ teamId: uuid, template: z.string().min(1).max(64), includeRecords: z.boolean().default(false) }))
    .mutation(async ({ ctx, input }) => {
      const found = await templateImportText(input.template, input.includeRecords)
      if (`error` in found) throw new TRPCError({ code: `BAD_REQUEST`, message: found.error })
      return importAppText(ctx, input.teamId, found.text, APP_TEMPLATES_COPY.note)
    }),

  update: authedProcedure
    .input(appUpdateInputSchema)
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, { id: input.id })
      await assertCanManage(userId, app)
      // VAPP-120: a grant names only what the RUNNING package declares (review
      // of #1222: nothing is granted ahead of a version that would use it),
      // and only an OWNER grants (the ruling: the creator gets no grant power).
      if (input.grantedTools) await assertCanGrant(userId, app)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const { id, headVersion, ...patch } = input
        if (patch.grantedTools) {
          // VAPP-156: the row is re-read under its lock (a version write or a
          // publish takes the same lock), so a grant lands only on the version
          // the owner saw; a newer one = CONFLICT and the owner grants again.
          const [locked] = await tx
            .select({ headVersion: apps.headVersion, publishedVersion: apps.publishedVersion, package: apps.package, draftPackage: apps.draftPackage })
            .from(apps)
            .where(eq(apps.id, id))
            .for(`update`)
          if (!locked || grantVersion(locked) !== headVersion) {
            throw new TRPCError({ code: `CONFLICT`, message: `A new version of the app was published; review its tools and grant again` })
          }
          const declared = packageActions(grantPackage(locked))
          const undeclared = patch.grantedTools.filter((name) => !declared.includes(name))
          if (undeclared.length > 0) {
            throw new TRPCError({ code: `BAD_REQUEST`, message: `The app's package does not declare ${undeclared.join(`, `)}: a grant names only what the published version calls` })
          }
        }
        // VAPP-176: a DRAFT (no version yet) renamed = its slug follows the
        // name (its own slug not counted as taken); the editors key by id.
        const [current] = await tx.select({ slug: apps.slug, headVersion: apps.headVersion }).from(apps).where(eq(apps.id, id)).for(`update`)
        const slug =
          patch.name && current && isDraftApp(current)
            ? slugify(patch.name, (await takenSlugs(tx, app.teamId)).filter((taken) => taken !== current.slug))
            : undefined
        const [updated] = await tx
          .update(apps)
          .set({ ...patch, ...(patch.grantedTools ? { grantedTools: patch.grantedTools.map(storedAction) } : {}), ...(slug ? { slug } : {}), updatedAt: new Date() })
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
      await assertAppAccess(ctx.db, app, userId, `write`)
      const pkg = validateAppPackage(input.package)
      await assertTeamUpstreams(ctx.db, app.teamId, pkg, [...(await bindingsOf(ctx.db, app.id)).keys(), ...declaredUpstreams(app.package), ...declaredUpstreams(app.draftPackage)])
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
      await assertAppAccess(ctx.db, app, userId, `write`)
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

  /** eApps V1 (#VAPP-210): PUBLISH a version (default: the head; an older
   *  one = a rollback). Same access as writing a version; an owner's publish
   *  keeps the still-declared grants, anyone else's clears them. */
  publish: authedProcedure
    .input(appRefSchema.and(z.object({ number: z.number().int().min(1).optional() })))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, input)
      const { member } = await resolveTeamAccess(userId, app.teamId)
      await assertAppAccess(ctx.db, app, userId, `write`)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const { published, head } = await publishVersionInTx(tx, app, { number: input.number, byOwner: canGrantActions(member.role), userId })
        return { txid: txId, published, head }
      }).then((out) => {
        // eApps R8: start the sweep now (the watchdog resumes it every minute).
        void import(`@/lib/apps/record-sweep`).then((m) => m.runRecordSweeps({ appId: app.id })).catch((err) => console.error(`[apps] record sweep failed`, err))
        return out
      })
    }),

  // VAPP-122: an app's RECORDS (fixtures/app-records.json). Any member reads
  // and creates them (team data the app owns); VAPP-187: only a record's
  // author or a team owner patches or deletes it (`ownership`, the MCP
  // records_* ops run through here). A write is validated against the head
  // package's kind schema, capped per record and per team. #VAPP-193: every
  // check and query lives in the records service (`lib/apps/records-service`).
  records: router({
    list: authedProcedure
      .input(
        appRefSchema.and(
          z.object({
            kind: kindSchema.optional(),
            limit: z.number().int().min(1).max(RECORDS_QUERY.limitMax).default(RECORDS_QUERY.limitDefault),
            offset: z.number().int().min(0).default(0),
          })
        )
      )
      .query(({ ctx, input }) => listRecords(ctx.session.user.id, input, input, ctx.db)),

    /** #VAPP-216 (Q4): the full structured query (the JSON form, the same
     *  evaluator as the UI) over the kind's rows the member may read. */
    query: authedProcedure
      .input(appRefSchema.and(z.object({ kind: kindSchema, query: z.record(z.string(), z.unknown()) })))
      .query(({ ctx, input }) => queryRecords(ctx.session.user.id, input, input, ctx.db)),

    /** Create a record, or (with `id`) patch that record's data (shallow).
     *  VAPP-188: `author` = an on-behalf write (owners only): the create's
     *  author, or an update's reassignment; created_by/updated_by stay the
     *  real writer. */
    upsert: authedProcedure
      .input(
        appRefSchema.and(
          // VAPP-235 (O1): an outbox replay brings its client write id
          // (`writeId`, the dedupe key) and a create its client id (`newId`).
          z.object({ kind: kindSchema, data: recordDataSchema, recordId: uuid.optional(), newId: uuid.optional(), writeId: uuid.optional(), author: z.string().min(1).max(64).optional(), via: clientVia, viaApp: viaAppClaimSchema })
        )
      )
      .mutation(async ({ ctx, input }) => {
        const viaAppId = await recordsViaApp(ctx, input)
        return upsertRecord(ctx.session.user.id, input, { ...input, via: writeVia(ctx, input.via), ...(viaAppId ? { viaAppId } : {}) }, ctx.db)
      }),

    delete: authedProcedure
      .input(appRefSchema.and(z.object({ recordId: uuid, writeId: uuid.optional(), via: clientVia })))
      .mutation(({ ctx, input }) => deleteRecord(ctx.session.user.id, input, input.recordId, ctx.db, writeVia(ctx, input.via), input.writeId)),

    /** VAPP-215 (R4, app-records.json `history`): a record's events, newest
     *  first (its read policy; a deleted record keeps its history). */
    history: authedProcedure
      .input(appRefSchema.and(z.object({ recordId: uuid })))
      .query(({ ctx, input }) => listRecordEvents(ctx.session.user.id, input, input.recordId, ctx.db)),

    /** VAPP-215: History's Restore: the record as that event left it, as a
     *  new write (update policy). */
    restore: authedProcedure
      .input(appRefSchema.and(z.object({ recordId: uuid, eventId: uuid, via: clientVia })))
      .mutation(({ ctx, input }) => restoreRecord(ctx.session.user.id, input, { recordId: input.recordId, eventId: input.eventId, via: writeVia(ctx, input.via) }, ctx.db)),

    /** VAPP-215: the toast's Undo: each event's `before` as a new write,
     *  newest first, in one transaction. */
    undo: authedProcedure
      .input(appRefSchema.and(z.object({ eventIds: z.array(uuid).min(1).max(Math.max(ACTION_LIMITS.steps, APP_CSV_IMPORT.maxRows)), via: clientVia })))
      .mutation(({ ctx, input }) => undoEvents(ctx.session.user.id, input, { eventIds: input.eventIds, via: writeVia(ctx, input.via) }, ctx.db)),

    /** VAPP-194 (eApps E1): a package action's record writes (its plan's
     *  `writes`, passed through by every client host's `records.batch`) as
     *  ONE transaction: all land or none. */
    batch: authedProcedure
      .input(
        appRefSchema.and(
          z.object({
            writes: z
              .array(
                z.discriminatedUnion(`op`, [
                  z.object({ op: z.literal(`upsert`), kind: kindSchema, data: recordDataSchema, id: uuid.optional(), newId: uuid.optional(), author: z.string().min(1).max(64).optional() }),
                  z.object({ op: z.literal(`delete`), id: uuid }),
                ])
              )
              .min(1)
              .max(APP_CSV_IMPORT.maxRows),
            /** VAPP-228: `import` = the Data tab's CSV import (up to
             *  `csvImport.maxRows` writes); else a package action's batch. */
            writeId: uuid.optional(),
            via: z.enum([`action`, `import`]).optional(),
            viaApp: viaAppClaimSchema,
          })
        )
        .refine((input) => input.via === `import` || input.writes.length <= ACTION_LIMITS.steps, { message: `At most ${ACTION_LIMITS.steps} writes per action`, path: [`writes`] })
      )
      .mutation(async ({ ctx, input }) => commitRecordBatch(ctx.session.user.id, input, input.writes, ctx.db, input.writeId, input.via ?? `action`, await recordsViaApp(ctx, input))),
  }),

  /** #VAPP-236 (eApps A2): the `handoff` action (`exponential.apps.handoff`):
   *  runs one of the app's PUBLISHED `exposes` whose run is `agent` as the
   *  member (write access to the app), its brief source `handoff <tool>`,
   *  the session reading "via <app>" (@exp/emcp exposes.json `agent`). */
  handoff: authedProcedure
    .input(
      appRefSchema.and(
        z.object({
          tool: z.string().min(1).max(64),
          input: z.record(z.string(), z.unknown()).default({}),
          record: z.object({ kind: kindSchema, id: uuid }).optional(),
        })
      )
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const app = await loadApp(ctx.db, input)
      await assertTeamMember(userId, app.teamId)
      await assertAppAccess(ctx.db, app, userId, `write`)
      const pkg = (app.package ?? {}) as { exposes?: unknown; records?: Record<string, unknown>; upstreams?: unknown }
      const exposes = Array.isArray(pkg.exposes) ? (pkg.exposes as Expose[]) : []
      const index = exposes.findIndex((e) => e?.name === input.tool)
      const expose = exposes[index]
      const upstreams = Array.isArray(pkg.upstreams) ? pkg.upstreams.filter((u): u is string => typeof u === `string`) : []
      const broken = validateExposes(exposes, { kinds: Object.keys(pkg.records ?? {}), upstreams }).some((e) => e.index === null || e.index === index)
      if (!expose || broken || runVia(expose.run) !== `agent` || app.mcpDisabledTools.includes(expose.name)) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `${app.name} has no agent hand-off ${JSON.stringify(input.tool)}` })
      }
      const missing = (expose.input?.required ?? []).filter((key) => !Object.prototype.hasOwnProperty.call(input.input, key))
      if (missing.length > 0) throw new TRPCError({ code: `BAD_REQUEST`, message: `missing ${missing.join(`, `)}` })
      const out = await handOff({
        app,
        userId,
        run: expose.run as AgentRun,
        input: input.input,
        source: `handoff ${expose.name}`,
        ref: input.record ?? null,
      })
      if (`sessionId` in out) return { sessionId: out.sessionId }
      throw new TRPCError({ code: out.code === `rateLimited` ? `TOO_MANY_REQUESTS` : out.code === `agentMissing` ? `NOT_FOUND` : `INTERNAL_SERVER_ERROR`, message: out.error })
    }),

  /** #VAPP-230 (eApps M1): one `upstream:<server>/<tool>?…` read binding,
   *  run by the broker as the viewer with their own connection, cached 60 s
   *  (@exp/emcp upstream.json `binding`). A refusal is an answer, not an
   *  error: the host keeps the binding's default (and, disconnected, shows
   *  "Connect <server>"). */
  upstreamRead: authedProcedure
    .input(z.object({ id: uuid, source: z.string().trim().min(1).max(2048) }))
    .query(async ({ ctx, input }) => {
      const app = await loadApp(ctx.db, { id: input.id })
      await assertTeamMember(ctx.session.user.id, app.teamId)
      await assertAppAccess(ctx.db, app, ctx.session.user.id, `read`)
      return readUpstreamBinding(ctx.session.user.id, app, input.source)
    }),

  /** VAPP-176: leaving the editor of an EMPTY draft (no version, no editor
   *  session: nothing was ever sent) deletes it for good, so abandoned New
   *  app drafts never pile up. Only its creator; anything else = a no-op. */
  discardDraft: authedProcedure
    .input(z.object({ id: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const deleted = await tx
          .delete(apps)
          .where(
            and(
              eq(apps.id, input.id),
              eq(apps.createdBy, userId),
              eq(apps.headVersion, 0),
              sql`NOT EXISTS (SELECT 1 FROM ${sessions} WHERE ${sessions.appId} = ${apps.id})`
            )
          )
          .returning({ id: apps.id })
        return { txid: txId, discarded: deleted.length > 0 }
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
